/**
 * One term's section choices during registration, mirroring
 * `skyspace-core::schedule` (`04-planning.md` "Schedules and conflict
 * detection"). Separate from a plan, works without an account, saved in the
 * browser until sign-in. Conflict detection is the engine's
 * (`engine.scheduleConflicts`); this file holds the shapes and the sums.
 */
import type {CourseCode, Credits} from './course';
import type {BusyId, ScheduleId} from './ids';
import type {Crn, Day, Meeting, Section} from './section';
import {creditRangeMin, sameCourse} from './course';
import type {TermCode} from './term';

export type Candidate = {
  course: CourseCode;
  /** Empty means none picked; a lecture plus a required lab is two entries. */
  sections: Crn[];
  /** Hide to test a combination without deleting. */
  visible: boolean;
  /** Stored by the server, never interpreted there; the grid maps it onto a hue. */
  colour: number;
};

/** P2 in `features/schedule.md`; in the type so the document shape is final. */
export type BusyBlock = {
  id: BusyId;
  label: string;
  /** Always a timed pattern, but the type is shared with sections. */
  meeting: Meeting;
};

export type TermSchedule = {
  id: ScheduleId;
  name: string;
  term: TermCode;
  /** No credit cap: thirty hours of options is the point. */
  candidates: Candidate[];
  busy: BusyBlock[];
};

export type MeetingOwner =
  {kind: 'section'; value: Crn} | {kind: 'busy'; value: BusyId};

/** The caller resolves CRNs to meetings from the catalog, so the engine does no lookups. */
export type ScheduledMeeting = {
  owner: MeetingOwner;
  meeting: Meeting;
  /** Same part of term = same dates, so time alone decides. */
  partOfTerm?: string;
};

export type Conflict = {a: MeetingOwner; b: MeetingOwner; days: Day[]};

export function sameOwner(a: MeetingOwner, b: MeetingOwner): boolean {
  return a.kind === b.kind && a.value === b.value;
}

/** The CRNs the grid asks the catalog for: every picked section, hidden ones included. */
export function scheduleCrns(schedule: TermSchedule): Crn[] {
  return schedule.candidates.flatMap(c => c.sections);
}

/** The picked sections of the visible candidates, in candidate order. */
export function visibleCrns(schedule: TermSchedule): Crn[] {
  return schedule.candidates.filter(c => c.visible).flatMap(c => c.sections);
}

export function findCandidate(
  schedule: TermSchedule,
  course: CourseCode,
): Candidate | undefined {
  return schedule.candidates.find(c => sameCourse(c.course, course));
}

/**
 * The live total the header shows: the visible candidates that have a
 * section picked, at the section's minimum hours. A candidate with no
 * section picked has no hours yet; a hidden one is out of the combination.
 */
export function visibleCredits(
  schedule: TermSchedule,
  sectionsByCrn: ReadonlyMap<Crn, Section>,
): Credits {
  let total = 0;
  for (const candidate of schedule.candidates) {
    if (!candidate.visible) {
      continue;
    }
    const first = candidate.sections
      .map(crn => sectionsByCrn.get(crn))
      .find(s => s !== undefined);
    if (first !== undefined) {
      total += creditRangeMin(first.listing.credits);
    }
  }
  return total;
}
