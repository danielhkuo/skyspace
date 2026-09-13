/**
 * `skyspace-core::schedule::find_conflicts`, interim TypeScript
 * (`04-planning.md`). Each pair once, in input order, so the result is
 * deterministic. Twelve visible courses at three meetings each is 36
 * meetings and 630 pair tests, so the grid needs no debounce.
 */
import {
  DAYS,
  type Conflict,
  type Day,
  type DateSpan,
  type MeetingTime,
  type ScheduledMeeting,
} from '../../domain';

const dateKey = (d: {year: number; month: number; day: number}): number =>
  d.year * 10000 + d.month * 100 + d.day;

/** Closed date spans; a shared day is an overlap. */
function datesOverlap(a: DateSpan, b: DateSpan): boolean {
  return (
    dateKey(a.start) <= dateKey(b.end) && dateKey(b.start) <= dateKey(a.end)
  );
}

/** Half-open `[start, end)`, so back-to-back classes do not conflict. */
function timesOverlap(a: MeetingTime, b: MeetingTime): boolean {
  return a.start < b.end && b.start < a.end;
}

function sharedDays(a: MeetingTime, b: MeetingTime): Day[] {
  const inB = new Set(b.days);
  return DAYS.filter(d => a.days.includes(d) && inB.has(d));
}

/**
 * Two meetings conflict when their days and times overlap, except that two
 * with different known parts of term and unknown dates do not: summer
 * sessions do not overlap. `unparsed` patterns are shown, never compared.
 */
function conflictDays(a: ScheduledMeeting, b: ScheduledMeeting): Day[] {
  if (
    a.meeting.pattern.kind !== 'timed' ||
    b.meeting.pattern.kind !== 'timed'
  ) {
    return [];
  }
  if (a.meeting.dates !== undefined && b.meeting.dates !== undefined) {
    if (!datesOverlap(a.meeting.dates, b.meeting.dates)) {
      return [];
    }
  } else if (
    a.partOfTerm !== undefined &&
    b.partOfTerm !== undefined &&
    a.partOfTerm !== b.partOfTerm
  ) {
    return [];
  }
  const ta = a.meeting.pattern.value;
  const tb = b.meeting.pattern.value;
  return timesOverlap(ta, tb) ? sharedDays(ta, tb) : [];
}

export function findConflictsInterim(meetings: ScheduledMeeting[]): Conflict[] {
  const out: Conflict[] = [];
  for (let i = 0; i < meetings.length; i += 1) {
    const a = meetings[i];
    if (a === undefined) {
      continue;
    }
    for (let j = i + 1; j < meetings.length; j += 1) {
      const b = meetings[j];
      if (b === undefined) {
        continue;
      }
      // A section's own lecture and lab never conflict with each other.
      if (a.owner.kind === b.owner.kind && a.owner.value === b.owner.value) {
        continue;
      }
      const days = conflictDays(a, b);
      if (days.length > 0) {
        out.push({a: a.owner, b: b.owner, days});
      }
    }
  }
  return out;
}
