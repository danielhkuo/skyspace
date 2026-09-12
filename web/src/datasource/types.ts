/**
 * The data seam. Pages talk to a `DataSource` and nothing else; the demo
 * source reads the fixture and writes to this browser, and an API source
 * replaces it later without touching a page. Everything is async so the
 * pages are already shaped for a network round trip.
 */
import type {
  CourseCode,
  CourseInfo,
  Crn,
  Plan,
  PlanBundle,
  PlanId,
  Section,
  TermCode,
} from '../domain';

export type DataSource = {
  kind: 'demo';
  /** The plan and everything the engine needs to evaluate it. */
  loadBundle(): Promise<PlanBundle>;
  savePlan(plan: Plan): Promise<void>;
  /** Forget every local edit to the plan; the next load starts fresh. */
  resetPlan(id: PlanId): Promise<void>;
  /** The student's favourited courses, in the order they were starred. */
  loadFavorites(): Promise<CourseCode[]>;
  saveFavorites(favorites: CourseCode[]): Promise<void>;
  /** Code or title match, ordered by code; empty query returns nothing. */
  searchCourses(query: string, limit: number): Promise<CourseInfo[]>;
  /** Courses the suggestion strip may offer for a rule (`08-board-interaction.md`). */
  catalogCandidates(): Promise<CourseCode[]>;
  /** The term the catalog shows; the demo has exactly one. */
  currentTerm(): Promise<{code: TermCode; label: string}>;
  /** Every section Rice lists for the term, in Rice's order. */
  listSections(term: TermCode): Promise<Section[]>;
  getSection(term: TermCode, crn: Crn): Promise<Section | undefined>;
};
