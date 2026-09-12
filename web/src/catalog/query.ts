/**
 * The catalog's search state, and the pure function that applies it to a
 * list of sections. The state lives in the URL (`features/catalog.md`: a
 * filtered search must be shareable), so this module owns the round trip
 * between `CatalogQuery` and `URLSearchParams` as well.
 */
import {
  courseKey,
  courseLevel,
  creditRangeMax,
  creditRangeMin,
  creditsFromHours,
  isScheduled,
  seatStatus,
  seatsOpen,
  timedMeetings,
  DAYS,
  type Attribute,
  type Crn,
  type Day,
  type MinuteOfDay,
  type Section,
} from '../domain';

export type SortKey = 'relevance' | 'code' | 'credits' | 'openSeats';

export const SORT_LABEL: Record<SortKey, string> = {
  relevance: 'Relevance',
  code: 'Course number',
  credits: 'Credits',
  openSeats: 'Open seats',
};

/** Course levels the rail offers; `500` means 500 and above. */
export const LEVELS = [100, 200, 300, 400, 500] as const;

export type CatalogQuery = {
  q: string;
  subjects: string[];
  distribution: Attribute[];
  levels: number[];
  days: Day[];
  startsAfter?: MinuteOfDay;
  endsBefore?: MinuteOfDay;
  /** Credit hours; a range matches when it can be taken for this many. */
  creditHours?: number;
  partOfTerm?: string;
  openOnly: boolean;
  /** Off by default: 58% of rows have no meeting time (`rice-data.md` §2). */
  showUnscheduled: boolean;
  sort: SortKey;
  /** The section open in the detail pane. */
  crn?: Crn;
};

export const EMPTY_QUERY: CatalogQuery = {
  q: '',
  subjects: [],
  distribution: [],
  levels: [],
  days: [],
  openOnly: false,
  showUnscheduled: false,
  sort: 'relevance',
};

const ATTRIBUTES: Attribute[] = ['GRP1', 'GRP2', 'GRP3', 'AD'];
const SORTS: SortKey[] = ['relevance', 'code', 'credits', 'openSeats'];

/** Subjects whose rows are never hidden as unscheduled: lessons and recitals are the course load. */
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

export function parseQuery(params: URLSearchParams): CatalogQuery {
  const sort = params.get('sort');
  const cr = params.get('cr');
  const crn = params.get('crn');
  const pot = params.get('pot');
  const q: CatalogQuery = {
    q: params.get('q') ?? '',
    subjects: list(params.get('subj')).map(s => s.toUpperCase()),
    distribution: list(params.get('dist'))
      .map(s => s.toUpperCase())
      .filter((s): s is Attribute => (ATTRIBUTES as string[]).includes(s)),
    levels: list(params.get('level'))
      .map(Number)
      .filter(n => (LEVELS as readonly number[]).includes(n)),
    days: (params.get('days') ?? '')
      .toUpperCase()
      .split('')
      .filter((d): d is Day => (DAYS as string[]).includes(d)),
    openOnly: params.get('open') === '1',
    showUnscheduled: params.get('unsched') === '1',
    sort: SORTS.includes(sort as SortKey) ? (sort as SortKey) : 'relevance',
  };
  const after = minutes(params.get('after'));
  const before = minutes(params.get('before'));
  if (after !== undefined) {
    q.startsAfter = after;
  }
  if (before !== undefined) {
    q.endsBefore = before;
  }
  if (cr !== null && /^\d+(\.\d+)?$/.test(cr)) {
    q.creditHours = Number(cr);
  }
  if (pot !== null && pot !== '') {
    q.partOfTerm = pot;
  }
  if (crn !== null && /^\d{5}$/.test(crn)) {
    q.crn = crn;
  }
  return q;
}

/** Only non-default values are written, so an untouched catalog has a bare URL. */
export function serializeQuery(query: CatalogQuery): URLSearchParams {
  const params = new URLSearchParams();
  if (query.q !== '') {
    params.set('q', query.q);
  }
  if (query.subjects.length > 0) {
    params.set('subj', query.subjects.join(','));
  }
  if (query.distribution.length > 0) {
    params.set('dist', query.distribution.join(','));
  }
  if (query.levels.length > 0) {
    params.set('level', query.levels.join(','));
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
  if (query.creditHours !== undefined) {
    params.set('cr', String(query.creditHours));
  }
  if (query.partOfTerm !== undefined) {
    params.set('pot', query.partOfTerm);
  }
  if (query.openOnly) {
    params.set('open', '1');
  }
  if (query.showUnscheduled) {
    params.set('unsched', '1');
  }
  if (query.sort !== 'relevance') {
    params.set('sort', query.sort);
  }
  if (query.crn !== undefined) {
    params.set('crn', query.crn);
  }
  return params;
}

/** How many filters are set, for the tablet "Filters (3)" button. The search box and the pane are not filters. */
export function activeFilterCount(query: CatalogQuery): number {
  return (
    query.subjects.length +
    query.distribution.length +
    query.levels.length +
    (query.days.length > 0 ? 1 : 0) +
    (query.startsAfter === undefined ? 0 : 1) +
    (query.endsBefore === undefined ? 0 : 1) +
    (query.creditHours === undefined ? 0 : 1) +
    (query.partOfTerm === undefined ? 0 : 1) +
    (query.openOnly ? 1 : 0)
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
    query.subjects.length > 0 &&
    !query.subjects.includes(listing.code.subject.toUpperCase())
  ) {
    return false;
  }
  if (query.distribution.length > 0) {
    const have = detail?.attributes ?? [];
    if (!query.distribution.some(a => have.includes(a))) {
      return false;
    }
  }
  if (query.levels.length > 0) {
    const level = Math.min(courseLevel(listing.code), 500);
    if (!query.levels.includes(level)) {
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
  if (query.creditHours !== undefined) {
    const want = creditsFromHours(query.creditHours);
    const {credits} = listing;
    const ok =
      credits.kind === 'either'
        ? credits.value.includes(want)
        : creditRangeMin(credits) <= want && want <= creditRangeMax(credits);
    if (!ok) {
      return false;
    }
  }
  if (
    query.partOfTerm !== undefined &&
    listing.partOfTerm !== query.partOfTerm
  ) {
    return false;
  }
  if (query.openOnly) {
    // Not polled is not the same as full, but "open seats only" cannot vouch for it.
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

export type CatalogResult = {
  rows: Section[];
  /** Rows the unscheduled rule hid; shown as a count so nothing vanishes silently. */
  hiddenUnscheduled: number;
  courseCount: number;
};

export function applyQuery(
  sections: Section[],
  query: CatalogQuery,
): CatalogResult {
  const ranked: {section: Section; rank: number}[] = [];
  let hiddenUnscheduled = 0;
  for (const section of sections) {
    const rank = searchRank(section, query.q);
    if (rank === undefined || !passesFilters(section, query)) {
      continue;
    }
    if (
      !query.showUnscheduled &&
      !isScheduled(section.listing) &&
      !ALWAYS_SHOWN_SUBJECTS.has(section.listing.code.subject.toUpperCase())
    ) {
      hiddenUnscheduled += 1;
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
      case 'code':
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

  const rows = ranked.map(r => r.section);
  const courses = new Set(rows.map(r => courseKey(r.listing.code)));
  return {rows, hiddenUnscheduled, courseCount: courses.size};
}

/** Distinct subjects in the term, for the rail's subject finder. */
export function subjectsIn(sections: Section[]): string[] {
  return [
    ...new Set(sections.map(s => s.listing.code.subject.toUpperCase())),
  ].sort();
}

/** Distinct part-of-term labels in the term, in first-seen order. */
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
