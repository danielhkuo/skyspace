/**
 * The catalog search contract, mirroring `skyspace-api::CatalogQuery` and
 * `SectionPage` (`06-api.md`) field for field. The API validates and runs
 * it as one SQL statement; the demo runs `catalog/query.ts` over a fixture.
 * `term` is not a field: the caller passes it beside the query.
 */
import type {Credits} from './course';
import type {Attribute} from './program';
import type {Day, MinuteOfDay, Section} from './section';

export type SortKey = 'relevance' | 'courseNumber' | 'credits' | 'openSeats';

export const SORT_LABEL: Record<SortKey, string> = {
  relevance: 'Relevance',
  courseNumber: 'Course number',
  credits: 'Credits',
  openSeats: 'Open seats',
};

/** Course levels the rail offers; `500` means 500 and above. */
export const LEVELS = [100, 200, 300, 400, 500] as const;

export const DEFAULT_LIMIT = 25;
export const MAX_LIMIT = 100;

/** Field for field the wire type; `term` is omitted because the page passes it. */
export type CatalogQuery = {
  q: string;
  subject: string[];
  attr: Attribute[];
  level: number[];
  /** Hundredths, inclusive; a range matches when it can be taken inside the bounds. */
  creditsMin?: Credits;
  creditsMax?: Credits;
  days: Day[];
  startsAfter?: MinuteOfDay;
  endsBefore?: MinuteOfDay;
  partOfTerm: string[];
  /** Outside the poll set is excluded, not treated as full. */
  openSeatsOnly: boolean;
  /** On by default: 58% of rows have no meeting time (`rice-data.md` §2). */
  scheduledOnly: boolean;
  sort: SortKey;
  offset: number;
  limit: number;
};

export const DEFAULT_QUERY: CatalogQuery = {
  q: '',
  subject: [],
  attr: [],
  level: [],
  days: [],
  partOfTerm: [],
  openSeatsOnly: false,
  scheduledOnly: true,
  sort: 'relevance',
  offset: 0,
  limit: DEFAULT_LIMIT,
};

/** One page of results, the API's `SectionPage` plus the two counts the rail shows. */
export type SectionPage = {
  rows: Section[];
  total: number;
  offset: number;
  limit: number;
  hasMore: boolean;
  /** Rows `scheduledOnly` hid, so nothing vanishes silently; MUSI is never hidden. */
  unscheduledHidden: number;
  /** Distinct courses across the whole match, not the page. */
  courseCount: number;
};
