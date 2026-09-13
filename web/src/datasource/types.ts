/**
 * The data seam. Pages talk to a `DataSource` and nothing else; the demo
 * source reads the fixture and writes to this browser, and an API source
 * replaces it later without touching a page. Everything is async so the
 * pages are already shaped for a network round trip.
 */
import type {
  CatalogQuery,
  CatalogYear,
  CourseCode,
  CourseInfo,
  Crn,
  Plan,
  PlanBundle,
  PlanId,
  ProgramId,
  ProgramSummary,
  RequirementId,
  ScheduleId,
  Section,
  SectionPage,
  TermCode,
  TermSchedule,
} from '../domain';

export type DataSource = {
  /** `demo` reads fixtures and this browser; `api` talks to `skyspace-api` on the same origin. */
  kind: 'demo' | 'api';
  /**
   * The active plan and everything the engine needs to evaluate it.
   * Rejects with `UnauthenticatedError` when there is no signed-in account
   * to hold a plan (the demo always has one).
   */
  loadBundle(): Promise<PlanBundle>;
  /**
   * Replace the stored plan. Rejects with `StaleVersionError` when another
   * tab or device wrote it since this one loaded it; the caller must reload,
   * never overwrite.
   */
  savePlan(plan: Plan): Promise<void>;
  /** A new plan; the store may mint the id, so the caller keeps the returned plan. */
  createPlan(plan: Plan): Promise<Plan>;
  /** Forget every local edit to the plan; the next load starts fresh. */
  resetPlan(id: PlanId): Promise<void>;
  /** The student's favourited courses, in the order they were starred. */
  loadFavorites(): Promise<CourseCode[]>;
  saveFavorites(favorites: CourseCode[]): Promise<void>;
  /** Code or title match, ordered by code; empty query returns nothing. */
  searchCourses(query: string, limit: number): Promise<CourseInfo[]>;
  /** Courses the suggestion strip may offer for a requirement (`08-board-interaction.md`). */
  catalogCandidates(): Promise<CourseCode[]>;
  /** The term the catalog shows; the demo has exactly one. */
  currentTerm(): Promise<{code: TermCode; label: string}>;
  /** One page of sections: `GET /api/v1/sections` (`06-api.md`). Never the whole term. */
  searchSections(term: TermCode, query: CatalogQuery): Promise<SectionPage>;
  getSection(term: TermCode, crn: Crn): Promise<Section | undefined>;
  /** Many at once: the schedule grid always asks for many, and `/seats` is batch-only (`06-api.md`). Unknown CRNs are left out. */
  getSections(term: TermCode, crns: Crn[]): Promise<Section[]>;
  /** Every section of one course in the term, for the pane's "All sections". */
  courseSections(term: TermCode, code: CourseCode): Promise<Section[]>;
  /** Reference lists the rail offers: Rice's `SUBJECTS` and `SESSIONS`. */
  listSubjects(term: TermCode): Promise<string[]>;
  /** `code` is what a listing carries and the filter matches; `label` is what a person reads. */
  listPartsOfTerm(term: TermCode): Promise<{code: string; label: string}[]>;
  /**
   * The term's schedules and which one the page opens. A guest's live in this
   * browser (`06-api.md` "Guest mode"); the demo keeps everyone's there.
   */
  loadSchedules(
    term: TermCode,
  ): Promise<{schedules: TermSchedule[]; current?: ScheduleId}>;
  /**
   * One replaced document. An unknown id creates, and the store may mint
   * its own id, so the caller keeps the returned document, not the one it sent.
   */
  saveSchedule(schedule: TermSchedule): Promise<TermSchedule>;
  deleteSchedule(id: ScheduleId): Promise<void>;
  setCurrentSchedule(term: TermCode, id: ScheduleId): Promise<void>;
  /** "Report this requirement": a person reviews every report (`06-api.md`). */
  reportRequirement(report: RequirementErrorReport): Promise<void>;
  /** Every program a plan may name, for the pickers: summaries, not rule trees. */
  listPrograms(): Promise<ProgramSummary[]>;
  /** Who is signed in, if anyone. The demo keeps a pretend session in this browser. */
  session(): Promise<Session | undefined>;
  /**
   * Mail a six-digit code. Always resolves for a well-formed address, so an
   * address cannot be probed; rejects with `RateLimitedError` when asked too often.
   */
  requestSignInCode(email: string): Promise<void>;
  /** Trade the code for a session. Rejects with `InvalidCodeError` when it is wrong, expired or used up. */
  verifySignInCode(email: string, code: string): Promise<Session>;
  signOut(): Promise<void>;
  /**
   * Move what this browser built while signed out onto the account
   * (`06-api.md` "Guest mode"). Never overwrites; a name collision is renamed.
   */
  claimGuestData(guest: GuestData): Promise<void>;
  /** Removes every plan, favorite and session. Cannot be undone. */
  deleteAccount(): Promise<void>;
  /**
   * Whether we are showing the last good data because a pull is overdue.
   * `staleSince` is the last good run when one is on record.
   */
  freshness(): Promise<{stale: boolean; staleSince?: string}>;
};

/** `name` is derived from the address: the API stores no display name. */
export type Session = {email: string; name: string};

export type GuestData = {
  schedules: TermSchedule[];
  collections: {name: string; courses: CourseCode[]}[];
};

/** The wire body of `POST /api/v1/reports/rule`, exactly: it rejects unknown keys. */
export type RequirementErrorReport = {
  program?: ProgramId;
  requirement?: RequirementId;
  catalogYear?: CatalogYear;
  /** Free text; the dialog folds the requirement's label and source URL in. */
  message: string;
};

/** `savePlan` / `saveSchedule`: someone else wrote the document since we loaded it. */
export class StaleVersionError extends Error {
  constructor(what: string) {
    super(`${what} was changed elsewhere; reload before saving again`);
    this.name = 'StaleVersionError';
  }
}

/** A session-only method called while signed out (a guest has no plan, `08-board-interaction.md`). */
export class UnauthenticatedError extends Error {
  constructor(what: string) {
    super(`Sign in to ${what}`);
    this.name = 'UnauthenticatedError';
  }
}

/** Signed in, but the account holds no plan yet: onboarding creates one. */
export class NoPlanError extends Error {
  constructor() {
    super('This account has no plan yet');
    this.name = 'NoPlanError';
  }
}

export class InvalidCodeError extends Error {
  constructor() {
    super('That code is wrong, expired, or already used');
    this.name = 'InvalidCodeError';
  }
}

export class RateLimitedError extends Error {
  constructor(public readonly retryAfterSeconds: number | undefined) {
    super('Too many requests; try again later');
    this.name = 'RateLimitedError';
  }
}
