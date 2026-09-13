/**
 * The sample schedule the demo opens with: the `Schedule Builder` artboard's
 * candidates on the sections `fallSections.ts` holds. COMP 140 002 and
 * COMP 222's Thursday lab overlap on purpose, so the grid has a conflict to
 * show; FWIS 149 starts hidden. Not a plan: schedules are one term's
 * section choices (`features/README.md`).
 */
import {
  parseCourseCode,
  type Candidate,
  type CourseCode,
  type ScheduleId,
  type TermSchedule,
} from '../domain';
import {FALL_2026} from './fallSections';

const code = (raw: string): CourseCode => {
  const parsed = parseCourseCode(raw);
  if (parsed === null) {
    throw new Error(`bad fixture course code ${raw}`);
  }
  return parsed;
};

const candidate = (
  raw: string,
  crn: string,
  colour: number,
  visible = true,
): Candidate => ({course: code(raw), sections: [crn], visible, colour});

export const FALL_SCHEDULE_ID =
  'a1f0c3d2-6b7e-4f11-9c2a-0d5e8b4a7f31' as ScheduleId;

export const fallSchedule: TermSchedule = {
  id: FALL_SCHEDULE_ID,
  name: 'Schedule A',
  term: FALL_2026,
  candidates: [
    candidate('COMP 140', '12423', 0),
    candidate('COMP 222', '12455', 7),
    candidate('STAT 310', '14211', 5),
    candidate('MUSI 117', '13400', 3),
    candidate('FWIS 149', '11990', 9, false),
    candidate('LPAP 170', '12951', 2),
  ],
  busy: [],
};
