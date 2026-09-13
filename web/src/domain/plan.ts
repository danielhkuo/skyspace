/**
 * The plan document, mirroring `skyspace-core::plan`. One plan is one route to
 * a degree. The engine warns about it and never refuses an edit.
 */
import type {ObservedFacts} from './facts';
import type {CourseCode, Credits} from './course';
import type {EntryId, PlanId, ProgramId, RequirementId, TermId} from './ids';
import type {CatalogYear, TermCode, TermPosition} from './term';

export type PlannedCourse = {
  id: EntryId;
  course: CourseCode;
  /** Editable: Rice publishes ranges. Zero when we hold no record, and zero is also real (recitals). */
  credits: Credits;
  /** At most one requirement per program. Never edits the requirement. */
  fills: RequirementId[];
  /** Why a pin that no longer matches should count. Without one, the pin is "unsure". */
  claims?: FillClaim[];
  /** The catalog's word on the course when it was placed; compared on every evaluation. */
  observed?: ObservedFacts;
  /** What this card was as a manual card, kept so Rice → Away → Rice loses nothing. */
  carried?: {
    origin: CreditOrigin;
    code: string;
    title: string;
    institution?: string;
  };
  note?: string;
};

/**
 * The student's stated reason for pinning a card to a requirement the filter
 * rejects. The engine never turns a claim into Met: it shows the card in
 * the slot, keeps the requirement out of the met count, and repeats the basis on
 * the PDF so an advisor can act on it. Proposed addition to `04-planning.md`.
 */
export type FillBasis =
  | {kind: 'earlierCatalog'; catalogYear?: CatalogYear}
  | {kind: 'advisorApproved'; who?: string; on?: string}
  | {kind: 'petitionGranted'; on?: string}
  | {kind: 'registrarPosted'; on?: string}
  | {kind: 'unsure'};

export type FillClaim = {
  requirement: RequirementId;
  basis: FillBasis;
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
  /**
   * Where the hours come from. Absent means the Rice equivalent's published
   * hours (the normal case: credit is posted as a Rice course). `manual` is
   * the student's own figure, which the engine flags: hours only, no course.
   */
  creditsSource?: 'equivalent' | 'manual';
  fills: RequirementId[];
  claims?: FillClaim[];
  note?: string;
};

/** Serde's externally tagged form: `{"rice": {...}}`, `{"away": {...}}`, `"off"`. */
export type TermKind =
  | {rice: {code?: TermCode; courses: PlannedCourse[]}}
  | {away: {cards: ManualCourseCard[]}}
  | 'off';

export type NonCourseClaim = {
  requirement: RequirementId;
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
  requirement: RequirementId;
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
