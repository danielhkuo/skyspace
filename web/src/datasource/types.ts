/**
 * The data seam. Pages talk to a `DataSource` and nothing else; the demo
 * source reads the fixture and writes to this browser, and an API source
 * replaces it later without touching a page. Everything is async so the
 * pages are already shaped for a network round trip.
 */
import type {CourseCode, CourseInfo, Plan, PlanBundle, PlanId} from '../domain';

export type SavedCollection = {
  id: string;
  name: string;
  courses: CourseCode[];
};

export type DataSource = {
  kind: 'demo';
  /** The plan and everything the engine needs to evaluate it. */
  loadBundle(): Promise<PlanBundle>;
  savePlan(plan: Plan): Promise<void>;
  /** Forget every local edit to the plan; the next load starts fresh. */
  resetPlan(id: PlanId): Promise<void>;
  loadCollections(): Promise<SavedCollection[]>;
  saveCollections(collections: SavedCollection[]): Promise<void>;
  /** Code or title match, ordered by code; empty query returns nothing. */
  searchCourses(query: string, limit: number): Promise<CourseInfo[]>;
  /** Courses the suggestion strip may offer for a rule (`08-board-interaction.md`). */
  catalogCandidates(): Promise<CourseCode[]>;
};
