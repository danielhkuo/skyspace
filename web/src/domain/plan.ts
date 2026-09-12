/**
 * The plan document, mirroring `skyspace-core::plan`. One plan is one route to
 * a degree. The engine warns about it and never refuses an edit.
 */
import type {CourseCode, Credits} from './course';
import type {EntryId, PlanId, ProgramId, RuleId, TermId} from './ids';
import type {CatalogYear, TermCode, TermPosition} from './term';

export type PlannedCourse = {
  id: EntryId;
  course: CourseCode;
  /** Editable: Rice publishes ranges. Zero when we hold no record, and zero is also real (recitals). */
  credits: Credits;
  /** At most one rule per program. Never edits the rule. */
  fills: RuleId[];
  note?: string;
};

export type CreditOrigin =
  | 'transfer'
  | 'advancedPlacement'
  | 'internationalBaccalaureate'
  | 'studyAbroad'
  | 'other';

export type ManualCourseCard = {
  id: EntryId;
  origin: CreditOrigin;
  /** The other institution's code. Free text; never parsed as a Rice code. */
  code: string;
  title: string;
  credits: Credits;
  institution?: string;
  riceEquivalent?: CourseCode;
  fills: RuleId[];
  note?: string;
};

/** Serde's externally tagged form: `{"rice": {...}}`, `{"away": {...}}`, `"off"`. */
export type TermKind =
  | {rice: {code?: TermCode; courses: PlannedCourse[]}}
  | {away: {cards: ManualCourseCard[]}}
  | 'off';

export type NonCourseClaim = {
  rule: RuleId;
  label: string;
};

export type PlanTerm = {
  id: TermId;
  position: TermPosition;
  label?: string;
  kind: TermKind;
  nonCourse: NonCourseClaim[];
};

export type SelfCheckReason =
  'transfer' | 'apOrIb' | 'studyAbroad' | 'advisorApproved' | 'other';

export type SelfCheck = {
  rule: RuleId;
  reason: SelfCheckReason;
  note?: string;
};

export type Plan = {
  id: PlanId;
  name: string;
  catalogYear: CatalogYear;
  matriculation: TermPosition;
  programs: ProgramId[];
  incomingCredit: ManualCourseCard[];
  terms: PlanTerm[];
  selfChecks: SelfCheck[];
};

export type TermKindName = 'rice' | 'away' | 'off';

export function termKindName(kind: TermKind): TermKindName {
  if (isRiceTerm(kind)) {
    return 'rice';
  }
  return isAwayTerm(kind) ? 'away' : 'off';
}

/** What an away term's label says about a card dropped into it. */
export function originForLabel(label: string | undefined): CreditOrigin {
  return /abroad/i.test(label ?? '') ? 'studyAbroad' : 'transfer';
}

export function isRiceTerm(
  kind: TermKind,
): kind is {rice: {code?: TermCode; courses: PlannedCourse[]}} {
  return typeof kind === 'object' && 'rice' in kind;
}

export function isAwayTerm(
  kind: TermKind,
): kind is {away: {cards: ManualCourseCard[]}} {
  return typeof kind === 'object' && 'away' in kind;
}

export function isOffTerm(kind: TermKind): kind is 'off' {
  return kind === 'off';
}

/** Saturating fold over a term's cards; `off` holds nothing. */
export function plannedCredits(term: PlanTerm): Credits {
  if (isRiceTerm(term.kind)) {
    return term.kind.rice.courses.reduce((sum, c) => sum + c.credits, 0);
  }
  if (isAwayTerm(term.kind)) {
    return term.kind.away.cards.reduce((sum, c) => sum + c.credits, 0);
  }
  return 0;
}
