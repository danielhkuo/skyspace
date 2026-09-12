#!/usr/bin/env node
/**
 * Pull degree requirements from Rice's General Announcements (ga.rice.edu).
 *
 * GA runs on CourseLeaf, so requirements are published as `table.sc_courselist`
 * — a regular structure of area headers, selection rules, course rows with
 * credit hours, and `or` alternatives. That maps almost directly onto the
 * requirement model Skyspace uses, which means we can auto-generate a first
 * draft of every program's rules instead of hand-reading prose.
 *
 * Two layers:
 *   1. One request to /programs-study/departments-programs/ -> every program URL
 *   2. One request per program -> structured requirement rows
 *
 * Usage:
 *   node scripts/pull-requirements.mjs --limit=3
 *   node scripts/pull-requirements.mjs --only=computer-science-bscs
 *   node scripts/pull-requirements.mjs --all --out=requirements.json
 *
 * THE OUTPUT IS A DRAFT. A human who knows the program has to check it against
 * the page before it drives anyone's graduation plan. Footnotes, prose caveats,
 * and "consult your advisor" rules do not survive this extraction.
 */

const ROOT = 'https://ga.rice.edu';
const INDEX = `${ROOT}/programs-study/departments-programs/`;
const UA = 'skyspace/0.1 (student project; contact: <your email>)';
const DELAY_MS = 200;

const args = Object.fromEntries(
  process.argv.slice(2).map(a => {
    const [k, v] = a.replace(/^--/, '').split('=');
    return [k, v ?? true];
  })
);

const wait = ms => new Promise(r => setTimeout(r, ms));

const get = async url => {
  const res = await fetch(url, { headers: { 'User-Agent': UA } });
  if (!res.ok) throw new Error(`${res.status} ${url}`);
  return res.text();
};

const txt = h =>
  (h ?? '')
    .replace(/<[^>]+>/g, ' ')
    .replace(/&#160;|&nbsp;/g, ' ')
    .replace(/&amp;/g, '&')
    .replace(/&#39;|&rsquo;/g, "'")
    .replace(/\s+/g, ' ')
    .trim();

/** Program pages are /school/department/program-degree/ — three path segments. */
const getProgramUrls = async () => {
  const html = await get(INDEX);
  const hrefs = [...html.matchAll(/href="(\/programs-study\/departments-programs\/[^"]+)"/g)].map(m => m[1]);
  return [...new Set(hrefs)]
    .filter(h => h.split('/').filter(Boolean).length === 5 && h.endsWith('/'))
    .map(h => ROOT + h);
};

const parseCourseLists = html => {
  const tables = [...html.matchAll(/<table class="sc_courselist"[\s\S]*?<\/table>/g)].map(m => m[0]);
  return tables.map(table => {
    const rows = [...table.matchAll(/<tr([^>]*)>([\s\S]*?)<\/tr>/g)];
    return rows
      .map(([, attrs, body]) => {
        const codeCell = (body.match(/class="codecol"[^>]*>([\s\S]*?)<\/td>/) || [])[1];
        const hoursCell = (body.match(/class="hourscol"[^>]*>([\s\S]*?)<\/td>/) || [])[1];
        const comment = (body.match(/class="courselistcomment[^"]*"[^>]*>([\s\S]*?)<\/span>/) || [])[1];
        const cells = [...body.matchAll(/<td([^>]*)>([\s\S]*?)<\/td>/g)];
        const titleCell = cells.find(([, a]) => !/codecol|hourscol/.test(a));

        const code = txt(codeCell);
        const hours = txt(hoursCell);
        const all = attrs + body;

        if (/areaheader/.test(all) && comment) return { kind: 'area', label: txt(comment) };
        if (/areasubheader/.test(all) && comment) return { kind: 'rule', label: txt(comment), hours };
        if (/orclass/.test(attrs)) return { kind: 'or', label: txt(body).replace(/^or\s+/i, ''), code: code || null };
        if (code) {
          return {
            kind: 'course',
            // cross-listings arrive as "STAT 310 / ECON 307"
            codes: code.split('/').map(c => c.trim()).filter(Boolean),
            title: txt(titleCell?.[2] ?? ''),
            hours: hours || null,
          };
        }
        if (comment) return { kind: 'comment', label: txt(comment) };
        const line = txt(body);
        if (/total credit hours/i.test(line)) return { kind: 'total', label: line, hours };
        return null;
      })
      .filter(Boolean)
      .filter(r => !(r.kind === 'comment' && /^Code Title Credit Hours$/i.test(r.label)));
  });
};

const parseProgram = (url, html) => {
  const title = txt((html.match(/<h1[^>]*>([\s\S]*?)<\/h1>/) || [])[1]);
  const lists = parseCourseLists(html);
  const rows = lists.flat();
  return {
    url,
    slug: url.split('/').filter(Boolean).pop(),
    title,
    tables: lists.length,
    counts: {
      areas: rows.filter(r => r.kind === 'area').length,
      rules: rows.filter(r => r.kind === 'rule').length,
      courses: rows.filter(r => r.kind === 'course').length,
    },
    requirements: rows,
  };
};

const main = async () => {
  let urls = await getProgramUrls();
  console.error(`${urls.length} program pages found`);

  if (args.only) urls = urls.filter(u => u.includes(args.only));
  if (args.limit) urls = urls.slice(0, Number(args.limit));
  if (!args.all && !args.only && !args.limit) urls = urls.slice(0, 5);

  const programs = [];
  for (const url of urls) {
    try {
      const program = parseProgram(url, await get(url));
      programs.push(program);
      const { areas, rules, courses } = program.counts;
      console.error(`  ${program.slug.padEnd(42)} areas:${areas} rules:${rules} courses:${courses}`);
    } catch (e) {
      console.error(`  ${url} FAILED: ${e.message}`);
    }
    await wait(DELAY_MS);
  }

  const out = { source: INDEX, pulledAt: new Date().toISOString(), count: programs.length, programs };
  if (args.out) {
    const { writeFileSync } = await import('node:fs');
    writeFileSync(args.out, JSON.stringify(out, null, 2));
    console.error(`wrote ${args.out}`);
  } else {
    console.log(JSON.stringify(out, null, 2));
  }
};

main().catch(e => {
  console.error(e);
  process.exit(1);
});
