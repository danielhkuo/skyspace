/**
 * The engine seam. `web/` calls these functions and nothing else; today
 * they run the interim TypeScript evaluator, later `skyspace-wasm`
 * (`07-wasm-testing.md`). Swapping the implementation must not touch a page.
 */
import type {
  CourseCode,
  CourseFacts,
  Credits,
  EntryId,
  PlacementPreview,
  PlanBundle,
  Program,
  Conflict,
  Report,
  RequirementId,
  ScheduledMeeting,
} from '../domain';
import {evaluateInterim, INTERIM_ENGINE_VERSION} from './interim/evaluate';
import {findConflictsInterim} from './interim/schedule';
import {
  previewPlacementInterim,
  requirementMatchesInterim,
} from './interim/preview';

export type Engine = {
  evaluate: (bundle: PlanBundle) => Report;
  previewPlacement: (
    bundle: PlanBundle,
    report: Report | undefined,
    course: CourseCode,
    moving: EntryId | undefined,
    credits: Credits,
  ) => PlacementPreview[];
  requirementMatches: (
    program: Program,
    requirement: RequirementId,
    codes: CourseCode[],
    facts: CourseFacts,
  ) => CourseCode[];
  /** `schedule_conflicts`: every overlapping pair of meetings, for the week grid. */
  scheduleConflicts: (meetings: ScheduledMeeting[]) => Conflict[];
  version: string;
};

export const engine: Engine = {
  evaluate: evaluateInterim,
  previewPlacement: previewPlacementInterim,
  requirementMatches: requirementMatchesInterim,
  scheduleConflicts: findConflictsInterim,
  version: INTERIM_ENGINE_VERSION,
};
