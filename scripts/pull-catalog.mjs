#!/usr/bin/env node
/**
 * Pull the Rice course catalog for a term from courses.rice.edu.
 *
 * Three data sources, all public, no auth:
 *   1. !SWKSCAT.info?action=SUBJECTS   -> XML list of subject codes
 *   2. !SWKSCAT.cat?p_action=QUERY     -> HTML listing, one request per subject
 *   3. !SWKSCAT.live?action=ENROLLMENT -> XML seats, one small request per CRN
 *
 * The listing gives CRN, course code, section, title, instructors (with NetIDs),
 * meeting time and days, final exam status, credits, and part of term.
 * It does NOT give distribution group, description, restrictions, grade mode, or
 * enrollment counts — those need the per-CRN detail page (p_action=COURSE), which
 * changes rarely. Meeting *location* is no longer published at all.
 *
 * Usage:
 *   node scripts/pull-catalog.mjs --term=202710 --subjects=COMP,MATH
 *   node scripts/pull-catalog.mjs --term=202710 --all --seats --out=fa26.json
 *
 * Note: the HTML parsing here is regex over a very regular server-rendered table.
 * Fine for a spike; swap in a real parser (node-html-parser) for production.
 */

const BASE = 'https://courses.rice.edu/courses/courses/!SWKSCAT';
const INFO = 'https://courses.rice.edu/courses/!SWKSCAT';
const UA = 'skyspace/0.1 (student project; contact: <your email>)';
const DELAY_MS = 150;

const args = Object.fromEntries(
  process.argv.slice(2).map(a => {
    const [k, v] = a.replace(/^--/, '').split('=');
    return [k, v ?? true];
  })
);

const TERM = args.term ?? '202710';
const YEAR = args.year ?? `20${TERM.slice(2, 4)}`;

const wait = ms => new Promise(r => setTimeout(r, ms));

const get = async url => {
  const res = await fetch(url, { headers: { 'User-Agent': UA } });
  if (!res.ok) throw new Error(`${res.status} ${url}`);
  return res.text();
};

const decode = s =>
  (s ?? '')
    .replace(/&#38;|&amp;/g, '&')
    .replace(/&#39;|&apos;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&nbsp;/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();

const cell = (row, cls) => {
  const m = row.match(new RegExp(`class="${cls}"[^>]*>([\\s\\S]*?)</td>`));
  return m ? m[1] : '';
};

const text = html => decode(html.replace(/<[^>]+>/g, ' '));

/** "2:30PM - 3:45PM TR" -> { start, end, days: ['T','R'] } */
const parseMeeting = raw => {
  const m = raw.match(/(\d{1,2}:\d{2}[AP]M)\s*-\s*(\d{1,2}:\d{2}[AP]M)\s*([MTWRFSU]*)/);
  if (!m) return raw ? { raw } : null;
  return { start: m[1], end: m[2], days: (m[3] || '').split(''), raw: raw.trim() };
};

const parseRows = html =>
  html
    .split(/<tr[^>]*>/)
    .filter(row => /class="cls-crn"/.test(row))
    .map(row => {
      const crn = cell(row, 'cls-crn').match(/>(\d+)</)?.[1];
      const crs = cell(row, 'cls-crs');
      const subject = crs.match(/>([A-Z]{2,5})<\/a>/)?.[1];
      const rest = text(crs.replace(/<a[\s\S]*?<\/a>/, ''));
      const [courseNumber, section] = rest.split(/\s+/);

      const instructors = [...cell(row, 'cls-ins').matchAll(/p_netid=([\w-]+)"[^>]*>([^<]*)</g)].map(
        ([, netid, name]) => ({ netid, name: decode(name) })
      );

      const mtg = cell(row, 'cls-mtg');
      // Rice emits one inner <div> per meeting pattern (COMP 222: "3:00PM - 3:50PM MWF" then "4:00PM - 5:15PM R").
      const clasInner = mtg.match(/data-mtg-type="CLAS"[^>]*>([\s\S]*?)<\/div>\s*<\/div>/)?.[1] ?? '';
      const meetings = [...clasInner.matchAll(/<div[^>]*>([\s\S]*?)<\/div>/g)]
        .map(([, inner]) => parseMeeting(decode(inner.replace(/<[^>]+>/g, ''))))
        .filter(Boolean);
      const finalExam = decode(mtg.match(/data-mtg-type="FINL"[^>]*>([\s\S]*?)<\/div>\s*<\/div>/)?.[1]?.replace(/<[^>]+>/g, '') ?? '');

      return {
        crn,
        subject,
        courseNumber,
        section,
        code: `${subject} ${courseNumber}`,
        title: text(cell(row, 'cls-ttl')),
        partOfTerm: text(cell(row, 'cls-ses')),
        instructors,
        meetings,
        finalExam: finalExam || null,
        credits: text(cell(row, 'cls-crd')),
        detailUrl: `${BASE}.cat?p_action=COURSE&p_term=${TERM}&p_crn=${crn}`,
      };
    })
    .filter(s => s.crn && s.subject);

const getSubjects = async () => {
  const xml = await get(`${INFO}.info?action=SUBJECTS&term=${TERM}&year=${YEAR}`);
  return [...xml.matchAll(/<SUBJECT code="([^"]+)"/g)].map(m => m[1]);
};

const getSeats = async crn => {
  const xml = await get(`${INFO}.live?action=ENROLLMENT&crn=${crn}&term=${TERM}`);
  const a = n => xml.match(new RegExp(`${n}="([^"]*)"`))?.[1];
  return {
    enrolled: Number(a('current-enrolled')),
    capacity: Number(a('max-enrolled')),
    waitCount: Number(a('wait-count')),
    waitCapacity: Number(a('wait-capacity')),
    asOf: a('time-now'),
  };
};

const main = async () => {
  const subjects = args.all
    ? await getSubjects()
    : String(args.subjects ?? 'COMP').split(',').map(s => s.trim());

  console.error(`term ${TERM} (year ${YEAR}) · ${subjects.length} subject(s)`);

  const sections = [];
  for (const subject of subjects) {
    try {
      const html = await get(`${BASE}.cat?p_action=QUERY&p_term=${TERM}&p_subj=${subject}`);
      const rows = parseRows(html);
      sections.push(...rows);
      console.error(`  ${subject.padEnd(5)} ${String(rows.length).padStart(4)} sections`);
    } catch (e) {
      console.error(`  ${subject.padEnd(5)} FAILED: ${e.message}`);
    }
    await wait(DELAY_MS);
  }

  if (args.seats) {
    console.error(`fetching live seats for ${sections.length} sections...`);
    for (const s of sections) {
      try {
        s.seats = await getSeats(s.crn);
      } catch (e) {
        s.seats = { error: e.message };
      }
      await wait(DELAY_MS);
    }
  }

  const out = { term: TERM, year: YEAR, pulledAt: new Date().toISOString(), count: sections.length, sections };
  if (args.out) {
    const { writeFileSync } = await import('node:fs');
    writeFileSync(args.out, JSON.stringify(out, null, 2));
    console.error(`wrote ${args.out} (${sections.length} sections)`);
  } else {
    console.log(JSON.stringify(out, null, 2));
  }
};

main().catch(e => {
  console.error(e);
  process.exit(1);
});
