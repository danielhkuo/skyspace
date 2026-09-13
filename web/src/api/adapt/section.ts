/**
 * Sections and the catalog query: wire (`skyspace-api`) to domain and back.
 * The wire spells a CRN as a number, days as Rice's letters, `asOf` as Unix
 * seconds and `finalExam` in snake_case; the domain differs on every one
 * (gap report §2).
 */
import type {CatalogQuery, SectionPage} from '../../domain/catalog';
import type {
  Crn,
  Day,
  FinalExam,
  Instructor,
  Meeting,
  MeetingPattern,
  Seats,
  Section,
  SectionDetail,
  SectionListing,
} from '../../domain/section';
import type {TermCode} from '../../domain/term';
import type {Course as WireCourse} from '../generated/Course';
import type {Day as WireQueryDay} from '../generated/Day';
import type {FinalExam as WireFinalExam} from '../generated/FinalExam';
import type {Instructor as WireInstructor} from '../generated/Instructor';
import type {Meeting as WireMeeting} from '../generated/Meeting';
import type {MeetingPattern as WireMeetingPattern} from '../generated/MeetingPattern';
import type {Seats as WireSeats} from '../generated/Seats';
import type {Section as WireSection} from '../generated/Section';
import type {SectionDetail as WireSectionDetail} from '../generated/SectionDetail';
import type {SectionListing as WireSectionListing} from '../generated/SectionListing';
import type {SectionPage as WireSectionPage} from '../generated/SectionPage';
import type {Query} from '../http';
import {lookup, opt, optMap, unknownWire} from './util';

// -------------------------------------------------------------------- crn

/**
 * Five digits, zero-padded: the wire is a `u32` and cannot keep a leading
 * zero, so the padding is the client's rule (gap report §2 `Crn`).
 */
export function crnFromWire(n: number): Crn {
  return String(n).padStart(5, '0');
}

export function crnToWire(crn: Crn): number {
  const n = Number(crn);
  if (!Number.isInteger(n) || n < 0) {
    throw new Error(`unknown wire value for crn: ${JSON.stringify(crn)}`);
  }
  return n;
}

// ------------------------------------------------------------------- days

const DAY_LETTERS: Record<Day, true> = {
  M: true,
  T: true,
  W: true,
  R: true,
  F: true,
  S: true,
  U: true,
};

function isDay(letter: string): letter is Day {
  return Object.prototype.hasOwnProperty.call(DAY_LETTERS, letter);
}

/** `"MWF"` -> `['M', 'W', 'F']`; any other letter is a bug upstream. */
export function daysFromWire(s: string): Day[] {
  return [...s].map(letter => {
    if (!isDay(letter)) {
      throw new Error(`unknown wire value for day: ${JSON.stringify(letter)}`);
    }
    return letter;
  });
}

export function daysToWire(days: Day[]): string {
  return days.join('');
}

const DAY_QUERY: Record<Day, WireQueryDay> = {
  M: 'mon',
  T: 'tue',
  W: 'wed',
  R: 'thu',
  F: 'fri',
  S: 'sat',
  U: 'sun',
};

/** The query string's third day vocabulary: `M` -> `mon`. */
export function dayToQuery(d: Day): WireQueryDay {
  return DAY_QUERY[d];
}

// ------------------------------------------------------------- final exam

const FINAL_EXAM: Record<WireFinalExam, FinalExam> = {
  scheduled: 'scheduled',
  scheduled_dept_room: 'scheduledDeptRoom',
  scheduled_online: 'scheduledOnline',
  take_home: 'takeHome',
  dept_schedules: 'deptSchedules',
  no_exam: 'noExam',
  unknown: 'unknown',
};

export function finalExamFromWire(w: WireFinalExam): FinalExam {
  return lookup(FINAL_EXAM, w, 'finalExam');
}

// ------------------------------------------------------------------ as of

const CENTRAL_STANDARD_MINUTES = -360;
const CENTRAL_DAYLIGHT_MINUTES = -300;

/** The `n`th Sunday of a month, as a UTC epoch at the given UTC hour. */
function nthSundayUtc(year: number, month: number, n: number, hour: number) {
  const first = new Date(Date.UTC(year, month, 1));
  const firstSunday = 1 + ((7 - first.getUTCDay()) % 7);
  return Date.UTC(year, month, firstSunday + 7 * (n - 1), hour);
}

/**
 * US Central: daylight time from the second Sunday of March at 2:00 CST
 * (08:00 UTC) to the first Sunday of November at 2:00 CDT (07:00 UTC).
 */
function centralOffsetMinutes(ms: number): number {
  const year = new Date(ms).getUTCFullYear();
  const start = nthSundayUtc(year, 2, 2, 8);
  const end = nthSundayUtc(year, 10, 1, 7);
  return ms >= start && ms < end
    ? CENTRAL_DAYLIGHT_MINUTES
    : CENTRAL_STANDARD_MINUTES;
}

/** The numeric offset of an RFC 3339 stamp; `Z` and a malformed stamp give `undefined`. */
function offsetMinutesOf(rfc3339: string): number | undefined {
  const match = /([+-])(\d{2}):?(\d{2})$/.exec(rfc3339);
  if (match === null) {
    return undefined;
  }
  const sign = match[1] === '-' ? -1 : 1;
  return sign * (Number(match[2]) * 60 + Number(match[3]));
}

function formatOffset(minutes: number): string {
  const sign = minutes < 0 ? '-' : '+';
  const abs = Math.abs(minutes);
  const hh = String(Math.floor(abs / 60)).padStart(2, '0');
  const mm = String(abs % 60).padStart(2, '0');
  return `${sign}${hh}:${mm}`;
}

/**
 * Unix seconds -> ISO 8601 in Rice's offset, so `formatAsOf` shows Rice's
 * clock. The offset comes from `Freshness.riceAsOf` when it carries one,
 * else from the US Central daylight-time rule (the wire carries no zone).
 */
export function asOfFromWire(seconds: number, riceAsOf?: string): string {
  const ms = seconds * 1000;
  const offset =
    (riceAsOf === undefined ? undefined : offsetMinutesOf(riceAsOf)) ??
    centralOffsetMinutes(ms);
  const local = new Date(ms + offset * 60_000).toISOString().slice(0, 19);
  return `${local}${formatOffset(offset)}`;
}

function seatsFromWire(w: WireSeats, riceAsOf?: string): Seats {
  return {
    enrolled: w.enrolled,
    capacity: w.capacity,
    waitlistCount: w.waitlistCount,
    waitlistCapacity: w.waitlistCapacity,
    asOf: asOfFromWire(w.asOf, riceAsOf),
  };
}

// --------------------------------------------------------------- meetings

function patternFromWire(w: WireMeetingPattern): MeetingPattern {
  switch (w.kind) {
    case 'timed':
      return {
        kind: 'timed',
        value: {
          days: daysFromWire(w.value.days),
          start: w.value.start,
          end: w.value.end,
        },
      };
    case 'unparsed':
      return {kind: 'unparsed', value: w.value};
    default:
      return unknownWire('meetingPattern', w);
  }
}

function patternToWire(p: MeetingPattern): WireMeetingPattern {
  switch (p.kind) {
    case 'timed':
      return {
        kind: 'timed',
        value: {
          days: daysToWire(p.value.days),
          start: p.value.start,
          end: p.value.end,
        },
      };
    case 'unparsed':
      return {kind: 'unparsed', value: p.value};
    default:
      return unknownWire('meetingPattern', p);
  }
}

export function meetingFromWire(w: WireMeeting): Meeting {
  return {pattern: patternFromWire(w.pattern), ...opt('dates', w.dates)};
}

/** Busy blocks carry meetings back to the server inside a schedule. */
export function meetingToWire(m: Meeting): WireMeeting {
  return {pattern: patternToWire(m.pattern), ...opt('dates', m.dates)};
}

// ---------------------------------------------------------------- section

function instructorFromWire(w: WireInstructor): Instructor {
  return {name: w.name, ...opt('netId', w.netId)};
}

function listingFromWire(w: WireSectionListing): SectionListing {
  return {
    crn: crnFromWire(w.crn),
    term: w.term,
    code: w.code,
    section: w.section,
    title: w.title,
    credits: w.credits,
    ...opt('partOfTerm', w.partOfTerm),
    instructors: w.instructors.map(instructorFromWire),
    meetings: w.meetings.map(meetingFromWire),
    finalExam: finalExamFromWire(w.finalExam),
  };
}

/**
 * The domain's `notes` are the description's fixed-form sentences; the wire
 * keeps them as booleans on the course, so the sentences are rebuilt here.
 */
function notesFromCourse(course: WireCourse | undefined): string[] {
  if (course === undefined) {
    return [];
  }
  const notes: string[] = [];
  if (course.flags.repeatable) {
    notes.push('Repeatable for Credit.');
  }
  if (course.flags.instructorPermission) {
    notes.push('Instructor Permission Required.');
  }
  return notes;
}

function detailFromWire(
  w: WireSectionDetail,
  course: WireCourse | undefined,
): SectionDetail {
  return {
    longTitle: w.longTitle,
    description: w.description,
    department: w.department,
    attributes: w.attributes,
    ...opt('prerequisitesText', w.prerequisitesText),
    ...optMap('restrictions', w.restrictions, r => r.raw),
    ...opt('gradeMode', w.gradeMode),
    ...opt('methodOfInstruction', w.methodOfInstruction),
    ...opt('courseType', w.courseType),
    ...opt('language', w.language),
    reserved: w.reserved,
    notes: notesFromCourse(course),
    ...opt('mutuallyExclusive', course?.mutualExclusions[0]?.raw),
    hasSyllabus: w.hasSyllabus,
  };
}

/**
 * `course` fills the two detail fields the section page does not carry
 * (`notes`, `mutuallyExclusive`); without it they are empty. `fees` and
 * `fetchedAt` have no domain field and are dropped.
 */
export function sectionFromWire(
  w: WireSection,
  course?: WireCourse,
  riceAsOf?: string,
): Section {
  return {
    listing: listingFromWire(w.listing),
    ...optMap('detail', w.detail, d => detailFromWire(d, course)),
    ...optMap('seats', w.seats, s => seatsFromWire(s, riceAsOf)),
  };
}

/** `applied` and `suggestions` have no domain counterpart and are dropped. */
export function sectionPageFromWire(
  w: WireSectionPage,
  riceAsOf?: string,
): SectionPage {
  return {
    rows: w.rows.map(row => sectionFromWire(row, undefined, riceAsOf)),
    total: w.total,
    offset: w.offset,
    limit: w.limit,
    hasMore: w.hasMore,
    unscheduledHidden: w.unscheduledHidden,
    courseCount: w.courseCount,
  };
}

// ------------------------------------------------------------------ query

function nonEmpty<T extends string | number>(
  items: readonly T[],
): readonly T[] | undefined {
  return items.length === 0 ? undefined : items;
}

/**
 * The query string for `GET /sections`. Empty text and empty lists are
 * omitted; `scheduledOnly` is always sent because the server's default is
 * `true` and the rail's "show unscheduled" relies on an explicit `false`.
 */
export function catalogQueryToWire(term: TermCode, q: CatalogQuery): Query {
  const text = q.q.trim();
  return {
    term,
    q: text === '' ? undefined : text,
    subject: nonEmpty(q.subject),
    attr: nonEmpty(q.attr),
    level: nonEmpty(q.level),
    creditsMin: q.creditsMin,
    creditsMax: q.creditsMax,
    days: nonEmpty(q.days.map(dayToQuery)),
    startsAfter: q.startsAfter,
    endsBefore: q.endsBefore,
    partOfTerm: nonEmpty(q.partOfTerm),
    openSeatsOnly: q.openSeatsOnly,
    scheduledOnly: q.scheduledOnly,
    sort: q.sort,
    offset: q.offset,
    limit: q.limit,
  };
}
