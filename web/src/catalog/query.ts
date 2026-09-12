/**
 * The catalog's search state, shaped like the API's `CatalogQuery`
 * (`06-api.md`), and its round trip through the URL (`features/catalog.md`:
 * a filtered search must be shareable). The demo data source runs
 * `applyQuery` over the fixture; the API runs one SQL statement. Pages see
 * only `searchSections(query)`.
 */
import {
  courseKey,
  courseLevel,
  creditRangeMax,
  creditRangeMin,
  isScheduled,
  seatStatus,
  seatsOpen,
  timedMeetings,
  DAYS,
  DEFAULT_QUERY,
  LEVELS,
  MAX_LIMIT,
  type Attribute,
  type CatalogQuery,
  type Credits,
  type Crn,
  type Day,
  type MinuteOfDay,
  type Section,
  type SectionPage,
  type SortKey,
} from '../domain';

export {DEFAULT_QUERY, LEVELS, SORT_LABEL} from '../domain';
export type {CatalogQuery} from '../domain';

/** What the catalog URL holds: the query, and which section the pane shows. */
export type CatalogUrlState = {
  query: CatalogQuery;
  crn?: Crn;
};

const ATTRIBUTES: Attribute[] = ['GRP1', 'GRP2', 'GRP3', 'AD'];
const SORTS: SortKey[] = ['relevance', 'courseNumber', 'credits', 'openSeats'];

/** Lessons and recitals are the course load, so these rows are never hidden. */
const ALWAYS_SHOWN_SUBJECTS = new Set(['MUSI']);

// -------------------------------------------------------------------- URL

const list = (raw: string | null): string[] =>
  raw === null || raw === '' ? [] : raw.split(',').filter(s => s !== '');

const minutes = (raw: string | null): MinuteOfDay | undefined => {
  if (raw === null || !/^\d+$/.test(raw)) {
    return undefined;
  }
  const n = Number(raw);
  return n >= 0 && n < 1440 ? n : undefined;
};

/** Credits ride in the URL as hours (`cmin=1.5`), on the wire as hundredths. */
const hours = (raw: string | null): Credits | undefined =>
  raw !== null && /^\d+(\.\d{1,2})?$/.test(raw)
    ? Math.round(Number(raw) * 100)
    : undefined;

const formatHours = (credits: Credits): string =>
  String(credits / 100).replace(/\.0+$/, '');

export function parseCatalogUrl(params: URLSearchParams): CatalogUrlState {
  const sort = params.get('sort');
  const query: CatalogQuery = {
    ...DEFAULT_QUERY,
    q: params.get('q') ?? '',
    subject: list(params.get('subj')).map(s => s.toUpperCase()),
    attr: list(params.get('attr'))
      .map(s => s.toUpperCase())
      .filter((s): s is Attribute => (ATTRIBUTES as string[]).includes(s)),
    level: list(params.get('level'))
      .map(Number)
      .filter(n => (LEVELS as readonly number[]).includes(n)),
    days: (params.get('days') ?? '')
      .toUpperCase()
      .split('')
      .filter((d): d is Day => (DAYS as string[]).includes(d)),
    partOfTerm: list(params.get('pot')),
    openSeatsOnly: params.get('open') === '1',
    scheduledOnly: params.get('unsched') !== '1',
    sort: SORTS.includes(sort as SortKey) ? (sort as SortKey) : 'relevance',
  };
  const startsAfter = minutes(params.get('after'));
  const endsBefore = minutes(params.get('before'));
  const creditsMin = hours(params.get('cmin'));
  const creditsMax = hours(params.get('cmax'));
  if (startsAfter !== undefined) {
    query.startsAfter = startsAfter;
  }
  if (endsBefore !== undefined) {
    query.endsBefore = endsBefore;
  }
  if (creditsMin !== undefined) {
    query.creditsMin = creditsMin;
  }
  if (creditsMax !== undefined) {
    query.creditsMax = creditsMax;
  }
  const crn = params.get('crn');
  return crn !== null && /^\d{5}$/.test(crn) ? {query, crn} : {query};
}

/** Only non-default values are written, so an untouched catalog has a bare URL. Paging never enters the URL. */
export function serializeCatalogUrl(state: CatalogUrlState): URLSearchParams {
  const {query, crn} = state;
  const params = new URLSearchParams();
  if (query.q !== '') {
    params.set('q', query.q);
  }
  if (query.subject.length > 0) {
    params.set('subj', query.subject.join(','));
  }
  if (query.attr.length > 0) {
    params.set('attr', query.attr.join(','));
  }
  if (query.level.length > 0) {
    params.set('level', query.level.join(','));
  }
  if (query.days.length > 0) {
    params.set('days', query.days.join(''));
  }
  if (query.startsAfter !== undefined) {
    params.set('after', String(query.startsAfter));
  }
  if (query.endsBefore !== undefined) {
    params.set('before', String(query.endsBefore));
  }
  if (query.creditsMin !== undefined) {
    params.set('cmin', formatHours(query.creditsMin));
  }
  if (query.creditsMax !== undefined) {
    params.set('cmax', formatHours(query.creditsMax));
  }
  if (query.partOfTerm.length > 0) {
    params.set('pot', query.partOfTerm.join(','));
  }
  if (query.openSeatsOnly) {
    params.set('open', '1');
  }
  if (!query.scheduledOnly) {
    params.set('unsched', '1');
  }
  if (query.sort !== 'relevance') {
    params.set('sort', query.sort);
  }
  if (crn !== undefined) {
    params.set('crn', crn);
  }
  return params;
}

/** How many filters are set, for the tablet "Filters (3)" button. Search, sort and paging are not filters. */
export function activeFilterCount(query: CatalogQuery): number {
  return (
    query.subject.length +
    query.attr.length +
    query.level.length +
    (query.days.length > 0 ? 1 : 0) +
    (query.startsAfter === undefined ? 0 : 1) +
    (query.endsBefore === undefined ? 0 : 1) +
    (query.creditsMin === undefined && query.creditsMax === undefined ? 0 : 1) +
    query.partOfTerm.length +
    (query.openSeatsOnly ? 1 : 0)
  );
}

// ------------------------------------------------------------------- time

/** `9:00 AM`, `9am`, `15:00`, `3:30pm` -> minutes; `undefined` for anything else. */
export function parseTimeText(raw: string): MinuteOfDay | undefined {
  const match = /^\s*(\d{1,2})(?::(\d{2}))?\s*([ap])?\.?m?\.?\s*$/i.exec(raw);
  if (match === null) {
    return undefined;
  }
  let hour = Number(match[1]);
  const minute = match[2] === undefined ? 0 : Number(match[2]);
  const meridiem = match[3]?.toLowerCase();
  if (hour > 23 || minute > 59) {
    return undefined;
  }
  if (meridiem !== undefined) {
    if (hour > 12 || hour === 0) {
      return undefined;
    }
    hour = (hour % 12) + (meridiem === 'p' ? 12 : 0);
  }
  return hour * 60 + minute;
}

// ----------------------------------------------------------------- search

/** 0 exact code, 1 code prefix, 2 title, 3 instructor; `undefined` is no match. */
function searchRank(section: Section, q: string): number | undefined {
  const needle = q.trim().toLowerCase();
  if (needle === '') {
    return 0;
  }
  const code = courseKey(section.listing.code).toLowerCase();
  const compactCode = code.replace(' ', '');
  const compactNeedle = needle.replace(/[\s-]+/g, '');
  if (compactCode === compactNeedle) {
    return 0;
  }
  if (compactCode.startsWith(compactNeedle)) {
    return 1;
  }
  const words = needle.split(/\s+/);
  const title = section.listing.title.toLowerCase();
  if (words.every(w => title.includes(w))) {
    return 2;
  }
  const names = section.listing.instructors.map(i => i.name.toLowerCase());
  if (names.some(n => words.every(w => n.includes(w)))) {
    return 3;
  }
  return undefined;
}

function passesFilters(section: Section, query: CatalogQuery): boolean {
  const {listing, seats, detail} = section;
  if (
    query.subject.length > 0 &&
    !query.subject.includes(listing.code.subject.toUpperCase())
  ) {
    return false;
  }
  if (query.attr.length > 0) {
    const have = detail?.attributes ?? [];
    if (!query.attr.some(a => have.includes(a))) {
      return false;
    }
  }
  if (query.level.length > 0) {
    const level = Math.min(courseLevel(listing.code), 500);
    if (!query.level.includes(level)) {
      return false;
    }
  }
  const times = timedMeetings(listing);
  if (query.days.length > 0) {
    if (!times.some(t => t.days.some(d => query.days.includes(d)))) {
      return false;
    }
  }
  if (query.startsAfter !== undefined) {
    const after = query.startsAfter;
    if (times.length === 0 || times.some(t => t.start < after)) {
      return false;
    }
  }
  if (query.endsBefore !== undefined) {
    const before = query.endsBefore;
    if (times.length === 0 || times.some(t => t.end > before)) {
      return false;
    }
  }
  if (query.creditsMin !== undefined || query.creditsMax !== undefined) {
    const lo = query.creditsMin ?? 0;
    const hi = query.creditsMax ?? Number.MAX_SAFE_INTEGER;
    const {credits} = listing;
    const ok =
      credits.kind === 'either'
        ? credits.value.some(c => lo <= c && c <= hi)
        : creditRangeMin(credits) <= hi && lo <= creditRangeMax(credits);
    if (!ok) {
      return false;
    }
  }
  if (
    query.partOfTerm.length > 0 &&
    (listing.partOfTerm === undefined ||
      !query.partOfTerm.includes(listing.partOfTerm))
  ) {
    return false;
  }
  if (query.openSeatsOnly) {
    if (seats === undefined) {
      return false;
    }
    const status = seatStatus(seats);
    if (status === 'full' || status === 'waitlistOpen') {
      return false;
    }
  }
  return true;
}

/** The demo's stand-in for the API's SQL: filter, rank, sort, then one page. */
export function applyQuery(
  sections: Section[],
  query: CatalogQuery,
): SectionPage {
  const ranked: {section: Section; rank: number}[] = [];
  let unscheduledHidden = 0;
  for (const section of sections) {
    const rank = searchRank(section, query.q);
    if (rank === undefined || !passesFilters(section, query)) {
      continue;
    }
    if (
      query.scheduledOnly &&
      !isScheduled(section.listing) &&
      !ALWAYS_SHOWN_SUBJECTS.has(section.listing.code.subject.toUpperCase())
    ) {
      unscheduledHidden += 1;
      continue;
    }
    ranked.push({section, rank});
  }

  const byCode = (a: Section, b: Section): number =>
    courseKey(a.listing.code).localeCompare(courseKey(b.listing.code)) ||
    a.listing.section.localeCompare(b.listing.section);
  const open = (s: Section): number =>
    s.seats === undefined ? -1 : seatsOpen(s.seats);

  ranked.sort((x, y) => {
    switch (query.sort) {
      case 'relevance':
        return x.rank - y.rank || byCode(x.section, y.section);
      case 'courseNumber':
        return byCode(x.section, y.section);
      case 'credits':
        return (
          creditRangeMin(y.section.listing.credits) -
            creditRangeMin(x.section.listing.credits) ||
          byCode(x.section, y.section)
        );
      case 'openSeats':
        return (
          open(y.section) - open(x.section) || byCode(x.section, y.section)
        );
    }
  });

  const all = ranked.map(r => r.section);
  const limit = Math.max(1, Math.min(query.limit, MAX_LIMIT));
  const offset = Math.max(0, query.offset);
  const rows = all.slice(offset, offset + limit);
  return {
    rows,
    total: all.length,
    offset,
    limit,
    hasMore: offset + rows.length < all.length,
    unscheduledHidden,
    courseCount: new Set(all.map(s => courseKey(s.listing.code))).size,
  };
}

/** Distinct subjects in a term, for the rail's subject finder. The API has `SUBJECTS` for this. */
export function subjectsIn(sections: Section[]): string[] {
  return [
    ...new Set(sections.map(s => s.listing.code.subject.toUpperCase())),
  ].sort();
}

/** Distinct part-of-term labels in a term, in first-seen order. The API has `SESSIONS`. */
export function partsOfTermIn(sections: Section[]): string[] {
  const out: string[] = [];
  for (const s of sections) {
    const pot = s.listing.partOfTerm;
    if (pot !== undefined && !out.includes(pot)) {
      out.push(pot);
    }
  }
  return out;
}
