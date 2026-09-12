/**
 * The engine seam. `web/` calls these three functions and nothing else; today
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
  Report,
  RequirementId,
} from '../domain';
import {evaluateInterim, INTERIM_ENGINE_VERSION} from './interim/evaluate';
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
  version: string;
};

export const engine: Engine = {
  evaluate: evaluateInterim,
  previewPlacement: previewPlacementInterim,
  requirementMatches: requirementMatchesInterim,
  version: INTERIM_ENGINE_VERSION,
};
