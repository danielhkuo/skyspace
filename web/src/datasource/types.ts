/**
 * The data seam. Pages talk to a `DataSource` and nothing else; the demo
 * source reads the fixture and writes to this browser, and an API source
 * replaces it later without touching a page. Everything is async so the
 * pages are already shaped for a network round trip.
 */
import type {
  CatalogQuery,
  CourseCode,
  CourseInfo,
  Crn,
  Plan,
  PlanBundle,
  PlanId,
  Program,
  ProgramId,
  RuleId,
  Section,
  SectionPage,
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
  /** One page of sections: `GET /api/v1/sections` (`06-api.md`). Never the whole term. */
  searchSections(term: TermCode, query: CatalogQuery): Promise<SectionPage>;
  getSection(term: TermCode, crn: Crn): Promise<Section | undefined>;
  /** Every section of one course in the term, for the pane's "All sections". */
  courseSections(term: TermCode, code: CourseCode): Promise<Section[]>;
  /** Reference lists the rail offers: Rice's `SUBJECTS` and `SESSIONS`. */
  listSubjects(term: TermCode): Promise<string[]>;
  listPartsOfTerm(term: TermCode): Promise<string[]>;
  /** "Report this rule": a person reviews every report (`06-api.md`). */
  reportRule(report: RuleReport): Promise<void>;
  /** Every program a plan may name, for the pickers. The demo has three. */
  listPrograms(): Promise<Program[]>;
  /** Who is signed in, if anyone. The demo keeps a pretend session in this browser. */
  session(): Promise<Session | undefined>;
  signIn(email: string): Promise<Session>;
  signOut(): Promise<void>;
  /** Removes every plan, favorite and session. Cannot be undone. */
  deleteAccount(): Promise<void>;
  /** When Rice's site last answered; `staleSince` set means we are showing the last good data. */
  freshness(): Promise<{staleSince?: string}>;
};

export type Session = {email: string; name: string};

export type RuleReport = {
  program: ProgramId;
  rule: RuleId;
  label: string;
  sourceUrl: string;
  text: string;
  email?: string;
};
