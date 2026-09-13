/**
 * A section of a course in one term, mirroring `skyspace-core` (`02-domain.md`
 * "Sections"). Three layers because Rice publishes them from three places:
 * the listing, the detail page, and the live seat feed. A missing layer is
 * `undefined`, never zeros.
 */
import type {CourseCode, CreditRange} from './course';
import type {Attribute} from './program';
import type {TermCode} from './term';

/** Five digits; a leading zero survives because this is text. */
export type Crn = string;

export type Day = 'M' | 'T' | 'W' | 'R' | 'F' | 'S' | 'U';

export const DAYS: Day[] = ['M', 'T', 'W', 'R', 'F', 'S', 'U'];

/** Minutes after midnight, campus local. */
export type MinuteOfDay = number;

export type MeetingTime = {days: Day[]; start: MinuteOfDay; end: MinuteOfDay};

export type CalendarDate = {year: number; month: number; day: number};

export type DateSpan = {start: CalendarDate; end: CalendarDate};

/** Adjacent tagging. `unparsed` is shown, never used for conflicts, never dropped. */
export type MeetingPattern =
  {kind: 'timed'; value: MeetingTime} | {kind: 'unparsed'; value: string};

export type Meeting = {
  pattern: MeetingPattern;
  /** Missing only for a listing-sourced meeting the XML pull has not covered. */
  dates?: DateSpan;
};

export type FinalExam =
  | 'scheduled'
  | 'scheduledDeptRoom'
  | 'scheduledOnline'
  | 'takeHome'
  | 'deptSchedules'
  | 'noExam'
  | 'unknown';

export const FINAL_EXAM_LABEL: Record<FinalExam, string> = {
  scheduled: 'Scheduled final exam',
  scheduledDeptRoom: 'Scheduled final exam, department room',
  scheduledOnline: 'Scheduled final exam, online',
  takeHome: 'Take-home final',
  deptSchedules: 'Department schedules the final',
  noExam: 'No final exam',
  unknown: 'Final exam not published',
};

/** Display label and link, never a lookup key (`rice-data.md` §2). */
export type Instructor = {name: string; netId?: string};

export type Seats = {
  enrolled: number;
  capacity: number;
  waitlistCount: number;
  waitlistCapacity: number;
  /** Rice's own `time-now`, ISO 8601 with the campus offset. */
  asOf: string;
};

/** "Reserved Seats for Fall Semester 2026 Matriculants: 60 (2 Available)". */
export type SeatReservation = {
  label: string;
  capacity: number;
  available: number;
};

export type SeatStatus = 'open' | 'nearlyFull' | 'waitlistOpen' | 'full';

export const NEARLY_FULL_THRESHOLD = 5;

export type SectionListing = {
  crn: Crn;
  term: TermCode;
  code: CourseCode;
  /** Three characters; letters in position 1 or 3 (`001`, `S01`, `0F1`). */
  section: string;
  title: string;
  credits: CreditRange;
  /** A label, not a code: `Full Term`, `1st Half of Full Term`. */
  partOfTerm?: string;
  instructors: Instructor[];
  /** Empty means no schedule; two or more is a lecture plus lab. */
  meetings: Meeting[];
  finalExam: FinalExam;
};

export type SectionDetail = {
  longTitle: string;
  description: string;
  department: string;
  attributes: Attribute[];
  /** The section page's simplified form; the expression lives on the course. */
  prerequisitesText?: string;
  restrictions?: string;
  gradeMode?: string;
  methodOfInstruction?: string;
  courseType?: string;
  language?: string;
  reserved: SeatReservation[];
  /** The description's fixed-form sentences other than the exclusion: "Repeatable for Credit." */
  notes: string[];
  /** "Cannot register for COMP 318 if student has credit for COMP 310." */
  mutuallyExclusive?: string;
  hasSyllabus: boolean;
};

export type Section = {
  listing: SectionListing;
  /** Missing when the detail page has not been pulled. */
  detail?: SectionDetail;
  /** Missing when the CRN is not polled; rendered as "not polled", never as zero. */
  seats?: Seats;
};

// ------------------------------------------------------------------ seats

/** Saturates: Rice publishes over-enrolled sections. */
export function seatsOpen(seats: Seats): number {
  return Math.max(0, seats.capacity - seats.enrolled);
}

export function waitlistOpen(seats: Seats): number {
  return Math.max(0, seats.waitlistCapacity - seats.waitlistCount);
}

/** In one place so the catalog row, the class page and the PDF cannot disagree. */
export function seatStatus(seats: Seats): SeatStatus {
  const open = seatsOpen(seats);
  if (open > NEARLY_FULL_THRESHOLD) {
    return 'open';
  }
  if (open > 0) {
    return 'nearlyFull';
  }
  return waitlistOpen(seats) > 0 ? 'waitlistOpen' : 'full';
}

// --------------------------------------------------------------- meetings

/** Any meeting with parsed days and times. Only 42% of Fall 2026 sections qualify. */
export function isScheduled(listing: SectionListing): boolean {
  return listing.meetings.some(m => m.pattern.kind === 'timed');
}

export function timedMeetings(listing: SectionListing): MeetingTime[] {
  const out: MeetingTime[] = [];
  for (const m of listing.meetings) {
    if (m.pattern.kind === 'timed') {
      out.push(m.pattern.value);
    }
  }
  return out;
}

/** `870` -> `2:30 PM`; the meridiem is dropped when `withMeridiem` is false. */
export function formatMinute(minute: MinuteOfDay, withMeridiem = true): string {
  const h24 = Math.floor(minute / 60);
  const m = minute % 60;
  const h12 = h24 % 12 === 0 ? 12 : h24 % 12;
  const mm = m === 0 ? '' : `:${String(m).padStart(2, '0')}`;
  const suffix = withMeridiem ? (h24 < 12 ? ' AM' : ' PM') : '';
  return `${h12}${mm}${suffix}`;
}

/** `2:30–3:45 PM TR`, with the meridiem once when both ends share it. */
export function formatMeetingTime(time: MeetingTime): string {
  const sameHalf = time.start < 720 === time.end < 720;
  const start = formatMinute(time.start, !sameHalf);
  const end = formatMinute(time.end);
  return `${start}–${end} ${time.days.join('')}`;
}

export function formatMeeting(meeting: Meeting): string {
  return meeting.pattern.kind === 'timed'
    ? formatMeetingTime(meeting.pattern.value)
    : meeting.pattern.value;
}

/** All patterns joined with a middle dot; empty string for no schedule. */
export function formatMeetings(listing: SectionListing): string {
  return listing.meetings.map(formatMeeting).join(' · ');
}

const MONTH = [
  'Jan',
  'Feb',
  'Mar',
  'Apr',
  'May',
  'Jun',
  'Jul',
  'Aug',
  'Sep',
  'Oct',
  'Nov',
  'Dec',
];

export function formatDateSpan(span: DateSpan): string {
  const one = (d: CalendarDate): string =>
    `${MONTH[d.month - 1] ?? ''} ${d.day}`;
  return `${one(span.start)} – ${one(span.end)}`;
}

/** `2026-09-11T19:09:12-05:00` -> `7:09 PM`, in the stamp's own offset: Rice's clock, not ours. */
export function formatAsOf(iso: string): string {
  const match = /T(\d{2}):(\d{2})/.exec(iso);
  if (match === null) {
    return iso;
  }
  const minute = Number(match[1]) * 60 + Number(match[2]);
  return formatMinute(minute);
}

// ----------------------------------------------------------------- labels

export function formatInstructors(listing: SectionListing): string {
  return listing.instructors.map(i => i.name).join(', ');
}

export const ATTRIBUTE_LABEL: Record<Attribute, string> = {
  GRP1: 'Distribution Group I',
  GRP2: 'Distribution Group II',
  GRP3: 'Distribution Group III',
  AD: 'Analyzing Diversity',
};

export function isDistributionGroup(attribute: Attribute): boolean {
  return attribute !== 'AD';
}
