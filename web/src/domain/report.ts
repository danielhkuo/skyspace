/**
 * What `evaluate` returns, and the drag-time placement preview.
 * Mirrors `skyspace-core::evaluate`, `::warn` and `08-board-interaction.md`.
 */
import type {CourseCode, Credits} from './course';
import type {EntryId, PlanId, ProgramId, RuleId, TermId} from './ids';
import type {SelfCheckReason, CreditOrigin, FillBasis} from './plan';
import type {SourceRef} from './program';
import type {CatalogYear, Season} from './term';

export type Outcome =
  | {outcome: 'met'}
  | {outcome: 'partial'}
  | {outcome: 'unmet'}
  | {outcome: 'needsStudentCheck'; confirmed?: SelfCheckReason};

export type Progress = {
  rulesMet: number;
  rulesCheckable: number;
  /** Rules held only by a pinned card the filter rejects: "on your say-so", never met. */
  rulesClaimed: number;
  creditsMet: Credits;
  creditsRequired: Credits;
  /** Above zero, `creditsRequired` is a lower bound and the interface says so. */
  creditsUnknown: number;
  selfChecks: number;
  selfChecksConfirmed: number;
};

export type RuleReport = {
  rule: RuleId;
  label: string;
  source: SourceRef;
  outcome: Outcome;
  progress: Progress;
  /** Board order. Card ids, not codes: a lesson taken eight times is eight cards. */
  filledBy: EntryId[];
  /** The subset of `filledBy` that sits there by claim, not by match. */
  claimedBy: EntryId[];
  claimedIn?: TermId;
  children: RuleReport[];
};

export type ProgramReport = {
  program: ProgramId;
  name: string;
  catalogYear: CatalogYear;
  evaluatedWith: CatalogYear;
  root: RuleReport;
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
      kind: 'ruleChoiceUnmatched';
      value: {term: TermId; entry: EntryId; rule: RuleId; basis?: FillBasis};
    }
  /** AP and IB credit never counts toward distribution or Analyzing Diversity (`rice-data.md` §10). */
  | {
      kind: 'incomingCreditIneligible';
      value: {entry: EntryId; rule: RuleId; origin: CreditOrigin};
    }
  /** One card filling rules in two programs; Rice limits the overlap and we cannot check it. */
  | {
      kind: 'doubleCounted';
      value: {entry: EntryId; course: CourseCode; programs: ProgramId[]};
    }
  | {
      kind: 'ruleChoiceMissing';
      value: {term: TermId; entry: EntryId; rule: RuleId; retired: boolean};
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
  /** A rule the engine cannot verify, surfaced as its own row so it is never silent. */
  | {
      kind: 'selfCheck';
      value: {program: ProgramId; rule: RuleId; label: string; text: string};
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
  fills: [ProgramId, RuleId][];
};

/** Document-order flattening, the slice the warning functions read. */
export function flattenRuleReports(report: Report): RuleReport[] {
  const out: RuleReport[] = [];
  const walk = (rule: RuleReport): void => {
    out.push(rule);
    for (const child of rule.children) {
      walk(child);
    }
  };
  for (const program of report.programs) {
    walk(program.root);
  }
  return out;
}
