/**
 * What `evaluate` returns, and the drag-time placement preview.
 * Mirrors `skyspace-core::evaluate`, `::warn` and `08-board-interaction.md`.
 */
import type {CourseCode, Credits} from './course';
import type {EntryId, PlanId, ProgramId, RequirementId, TermId} from './ids';
import type {SelfCheckReason, CreditOrigin, FillBasis} from './plan';
import type {SourceRef} from './program';
import type {CatalogYear, Season} from './term';

export type Outcome =
  | {outcome: 'met'}
  | {outcome: 'partial'}
  | {outcome: 'unmet'}
  | {outcome: 'needsStudentCheck'; confirmed?: SelfCheckReason};

export type Progress = {
  requirementsMet: number;
  requirementsCheckable: number;
  /** Requirements held only by a pinned card the filter rejects: "on your say-so", never met. */
  requirementsClaimed: number;
  creditsMet: Credits;
  creditsRequired: Credits;
  /** Above zero, `creditsRequired` is a lower bound and the interface says so. */
  creditsUnknown: number;
  selfChecks: number;
  selfChecksConfirmed: number;
};

export type RequirementReport = {
  requirement: RequirementId;
  label: string;
  source: SourceRef;
  outcome: Outcome;
  progress: Progress;
  /** Board order. Card ids, not codes: a lesson taken eight times is eight cards. */
  filledBy: EntryId[];
  /** The subset of `filledBy` that sits there by claim, not by match. */
  claimedBy: EntryId[];
  claimedIn?: TermId;
  children: RequirementReport[];
};

export type ProgramReport = {
  program: ProgramId;
  name: string;
  catalogYear: CatalogYear;
  evaluatedWith: CatalogYear;
  root: RequirementReport;
  progress: Progress;
  declaredCredits?: Credits;
};

export type PrereqProblem =
  | {kind: 'notInPlan'}
  | {kind: 'sameTerm'}
  | {kind: 'later'; value: {prerequisiteTerm: TermId}};

/** Adjacent tagging: `{kind, value}`. */
export type Warning =
  | {
      kind: 'fillsNoRequirement';
      value: {term: TermId; entry: EntryId; course: CourseCode};
    }
  | {kind: 'duplicateCourse'; value: {course: CourseCode; terms: TermId[]}}
  | {
      kind: 'overSemesterLoad';
      value: {term: TermId; planned: Credits; normal: Credits};
    }
  | {
      kind: 'programUnavailable';
      value: {program: ProgramId; catalogYear: CatalogYear};
    }
  | {
      kind: 'programYearSubstituted';
      value: {program: ProgramId; wanted: CatalogYear; used: CatalogYear};
    }
  | {
      kind: 'requirementChoiceUnmatched';
      value: {
        term: TermId;
        entry: EntryId;
        requirement: RequirementId;
        basis?: FillBasis;
      };
    }
  /** AP and IB credit never counts toward distribution or Analyzing Diversity (`rice-data.md` §10). */
  | {
      kind: 'incomingCreditIneligible';
      value: {entry: EntryId; requirement: RequirementId; origin: CreditOrigin};
    }
  /** A manual card whose hours the student typed: hours only, not a Rice course, and unverified. */
  | {kind: 'manualCredits'; value: {entry: EntryId; credits: Credits}}
  /** One card filling requirements in two programs; Rice limits the overlap and we cannot check it. */
  | {
      kind: 'doubleCounted';
      value: {entry: EntryId; course: CourseCode; programs: ProgramId[]};
    }
  | {
      kind: 'requirementChoiceMissing';
      value: {
        term: TermId;
        entry: EntryId;
        requirement: RequirementId;
        retired: boolean;
      };
    }
  | {
      kind: 'attributeUnknown';
      value: {term: TermId; entry: EntryId; course: CourseCode};
    }
  | {
      kind: 'prerequisite';
      value: {
        term: TermId;
        course: CourseCode;
        prerequisite: CourseCode;
        problem: PrereqProblem;
        publishedFor: CatalogYear;
      };
    }
  | {
      kind: 'prerequisiteUnparsed';
      value: {term: TermId; course: CourseCode; published: string};
    }
  | {
      kind: 'mutuallyExclusive';
      value: {
        blocked: CourseCode;
        blockedTerm: TermId;
        blocker: CourseCode;
        blockerTerm: TermId;
        published: string;
      };
    }
  | {
      kind: 'seasonUnlikely';
      value: {
        term: TermId;
        course: CourseCode;
        plannedIn: Season;
        termsSeen: number;
      };
    }
  /** A requirement the engine cannot verify, surfaced as its own row so it is never silent. */
  | {
      kind: 'selfCheck';
      value: {
        program: ProgramId;
        requirement: RequirementId;
        label: string;
        text: string;
      };
    };

export type Report = {
  plan: PlanId;
  engineVersion: string;
  programs: ProgramReport[];
  progress: Progress;
  warnings: Warning[];
};

export type PrereqVerdict =
  | {kind: 'satisfied'}
  | {kind: 'missing'; value: {courses: CourseCode[]}}
  | {kind: 'sameTerm'; value: {courses: CourseCode[]}}
  | {kind: 'unknown'};

/** What dropping a course into one term would mean (`08-board-interaction.md`). */
export type PlacementPreview = {
  term: TermId;
  prerequisites: PrereqVerdict;
  duplicateOf?: TermId;
  creditsAfter: Credits;
  fills: [ProgramId, RequirementId][];
};

/** Document-order flattening, the slice the warning functions read. */
export function flattenRequirementReports(report: Report): RequirementReport[] {
  const out: RequirementReport[] = [];
  const walk = (requirement: RequirementReport): void => {
    out.push(requirement);
    for (const child of requirement.children) {
      walk(child);
    }
  };
  for (const program of report.programs) {
    walk(program.root);
  }
  return out;
}
