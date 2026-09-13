/**
 * Plans, both ways: the browser reads a plan and writes it back, so every
 * kind rename here has an inverse and `planToWire(planFromWire(w))` must
 * reproduce `w`. The bundle is read-only. Renames per gap report §2:
 * `FillBasis`, `CreditOrigin`, `SelfCheckReason`, `PrereqFact`, and
 * `ObservedFacts.at` (Unix seconds on the wire, ISO 8601 in the domain).
 */
import type {
  CreditLimits,
  Exclusion,
  ObservedFacts,
  PlanBundle,
  PrereqFact,
  Prerequisite,
} from '../../domain/facts';
import type {
  EntryId,
  PlanId,
  ProgramId,
  RequirementId,
  TermId,
} from '../../domain/ids';
import type {
  CreditOrigin,
  FillBasis,
  FillClaim,
  ManualCourseCard,
  NonCourseClaim,
  Plan,
  PlanTerm,
  PlannedCourse,
  SelfCheck,
  SelfCheckReason,
  TermKind,
} from '../../domain/plan';
import type {CreditLimits as WireCreditLimits} from '../generated/CreditLimits';
import type {CreditOrigin as WireCreditOrigin} from '../generated/CreditOrigin';
import type {Exclusion as WireExclusion} from '../generated/Exclusion';
import type {FillBasis as WireFillBasis} from '../generated/FillBasis';
import type {FillClaim as WireFillClaim} from '../generated/FillClaim';
import type {ManualCourseCard as WireManualCourseCard} from '../generated/ManualCourseCard';
import type {NonCourseClaim as WireNonCourseClaim} from '../generated/NonCourseClaim';
import type {ObservedFacts as WireObservedFacts} from '../generated/ObservedFacts';
import type {Plan as WirePlan} from '../generated/Plan';
import type {PlanBundle as WirePlanBundle} from '../generated/PlanBundle';
import type {PlanTerm as WirePlanTerm} from '../generated/PlanTerm';
import type {PlannedCourse as WirePlannedCourse} from '../generated/PlannedCourse';
import type {PrereqFact as WirePrereqFact} from '../generated/PrereqFact';
import type {Prerequisite as WirePrerequisite} from '../generated/Prerequisite';
import type {SelfCheck as WireSelfCheck} from '../generated/SelfCheck';
import type {SelfCheckReason as WireSelfCheckReason} from '../generated/SelfCheckReason';
import type {TermKind as WireTermKind} from '../generated/TermKind';
import {courseFactsFromWire} from './course';
import {programFromWire} from './program';
import {invert, lookup, opt, optMap, unknownWire} from './util';

// ------------------------------------------------------------ kind renames

const CREDIT_ORIGIN: Record<WireCreditOrigin, CreditOrigin> = {
  transfer: 'transfer',
  advanced_placement: 'advancedPlacement',
  international_baccalaureate: 'internationalBaccalaureate',
  study_abroad: 'studyAbroad',
  other: 'other',
};
const CREDIT_ORIGIN_TO_WIRE = invert(CREDIT_ORIGIN);

export function creditOriginFromWire(w: WireCreditOrigin): CreditOrigin {
  return lookup(CREDIT_ORIGIN, w, 'creditOrigin');
}

export function creditOriginToWire(o: CreditOrigin): WireCreditOrigin {
  return CREDIT_ORIGIN_TO_WIRE[o];
}

const SELF_CHECK_REASON: Record<WireSelfCheckReason, SelfCheckReason> = {
  transfer: 'transfer',
  ap_or_ib: 'apOrIb',
  study_abroad: 'studyAbroad',
  advisor_approved: 'advisorApproved',
  other: 'other',
};
const SELF_CHECK_REASON_TO_WIRE = invert(SELF_CHECK_REASON);

export function selfCheckReasonFromWire(
  w: WireSelfCheckReason,
): SelfCheckReason {
  return lookup(SELF_CHECK_REASON, w, 'selfCheckReason');
}

export function selfCheckReasonToWire(r: SelfCheckReason): WireSelfCheckReason {
  return SELF_CHECK_REASON_TO_WIRE[r];
}

export function fillBasisFromWire(w: WireFillBasis): FillBasis {
  switch (w.kind) {
    case 'earlier_catalog':
      return {kind: 'earlierCatalog', ...opt('catalogYear', w.catalogYear)};
    case 'advisor_approved':
      return {
        kind: 'advisorApproved',
        ...opt('who', w.who),
        ...opt('on', w.on),
      };
    case 'petition_granted':
      return {kind: 'petitionGranted', ...opt('on', w.on)};
    case 'registrar_posted':
      return {kind: 'registrarPosted', ...opt('on', w.on)};
    case 'unsure':
      return {kind: 'unsure'};
    default:
      return unknownWire('fillBasis', w);
  }
}

export function fillBasisToWire(b: FillBasis): WireFillBasis {
  switch (b.kind) {
    case 'earlierCatalog':
      return {kind: 'earlier_catalog', ...opt('catalogYear', b.catalogYear)};
    case 'advisorApproved':
      return {
        kind: 'advisor_approved',
        ...opt('who', b.who),
        ...opt('on', b.on),
      };
    case 'petitionGranted':
      return {kind: 'petition_granted', ...opt('on', b.on)};
    case 'registrarPosted':
      return {kind: 'registrar_posted', ...opt('on', b.on)};
    case 'unsure':
      return {kind: 'unsure'};
    default:
      return unknownWire('fillBasis', b);
  }
}

// ------------------------------------------------------------- timestamps

/** Unix seconds -> ISO 8601 UTC. Whole seconds only, so the inverse is exact. */
export function timestampFromWire(seconds: number): string {
  return new Date(seconds * 1000).toISOString();
}

export function timestampToWire(iso: string): number {
  const ms = Date.parse(iso);
  if (Number.isNaN(ms)) {
    throw new Error(`unknown wire value for timestamp: ${JSON.stringify(iso)}`);
  }
  return Math.floor(ms / 1000);
}

function observedFromWire(w: WireObservedFacts): ObservedFacts {
  return {
    at: timestampFromWire(w.at),
    catalogYear: w.catalogYear,
    title: w.title,
    credits: w.credits,
    attributes: w.attributes,
  };
}

function observedToWire(o: ObservedFacts): WireObservedFacts {
  return {
    at: timestampToWire(o.at),
    catalogYear: o.catalogYear,
    title: o.title,
    credits: o.credits,
    attributes: o.attributes,
  };
}

// ------------------------------------------------------------------ cards

function fillClaimFromWire(w: WireFillClaim): FillClaim {
  return {
    requirement: w.requirement as RequirementId,
    basis: fillBasisFromWire(w.basis),
    ...opt('note', w.note),
  };
}

function fillClaimToWire(c: FillClaim): WireFillClaim {
  return {
    requirement: c.requirement,
    basis: fillBasisToWire(c.basis),
    ...opt('note', c.note),
  };
}

function plannedCourseFromWire(w: WirePlannedCourse): PlannedCourse {
  return {
    id: w.id as EntryId,
    course: w.course,
    credits: w.credits,
    fills: w.fills.map(id => id as RequirementId),
    ...optMap('claims', w.claims, claims => claims.map(fillClaimFromWire)),
    ...optMap('observed', w.observed, observedFromWire),
    ...optMap('carried', w.carried, c => ({
      origin: creditOriginFromWire(c.origin),
      code: c.code,
      title: c.title,
      ...opt('institution', c.institution),
    })),
    ...opt('note', w.note),
  };
}

function plannedCourseToWire(p: PlannedCourse): WirePlannedCourse {
  return {
    id: p.id,
    course: p.course,
    credits: p.credits,
    fills: p.fills,
    ...optMap('claims', p.claims, claims => claims.map(fillClaimToWire)),
    ...optMap('observed', p.observed, observedToWire),
    ...optMap('carried', p.carried, c => ({
      origin: creditOriginToWire(c.origin),
      code: c.code,
      title: c.title,
      ...opt('institution', c.institution),
    })),
    ...opt('note', p.note),
  };
}

function manualCardFromWire(w: WireManualCourseCard): ManualCourseCard {
  return {
    id: w.id as EntryId,
    origin: creditOriginFromWire(w.origin),
    code: w.code,
    title: w.title,
    credits: w.credits,
    ...opt('institution', w.institution),
    ...opt('riceEquivalent', w.riceEquivalent),
    ...opt('creditsSource', w.creditsSource),
    fills: w.fills.map(id => id as RequirementId),
    ...optMap('claims', w.claims, claims => claims.map(fillClaimFromWire)),
    ...opt('note', w.note),
  };
}

function manualCardToWire(m: ManualCourseCard): WireManualCourseCard {
  return {
    id: m.id,
    origin: creditOriginToWire(m.origin),
    code: m.code,
    title: m.title,
    credits: m.credits,
    ...opt('institution', m.institution),
    ...opt('riceEquivalent', m.riceEquivalent),
    ...opt('creditsSource', m.creditsSource),
    fills: m.fills,
    ...optMap('claims', m.claims, claims => claims.map(fillClaimToWire)),
    ...opt('note', m.note),
  };
}

// ------------------------------------------------------------------ terms

function termKindFromWire(w: WireTermKind): TermKind {
  if (w === 'off') {
    return 'off';
  }
  if ('rice' in w) {
    return {
      rice: {
        ...opt('code', w.rice.code),
        courses: w.rice.courses.map(plannedCourseFromWire),
      },
    };
  }
  if ('away' in w) {
    return {away: {cards: w.away.cards.map(manualCardFromWire)}};
  }
  return unknownWire('termKind', w);
}

function termKindToWire(k: TermKind): WireTermKind {
  if (k === 'off') {
    return 'off';
  }
  if ('rice' in k) {
    return {
      rice: {
        ...opt('code', k.rice.code),
        courses: k.rice.courses.map(plannedCourseToWire),
      },
    };
  }
  return {away: {cards: k.away.cards.map(manualCardToWire)}};
}

function nonCourseClaimFromWire(w: WireNonCourseClaim): NonCourseClaim {
  return {requirement: w.requirement as RequirementId, label: w.label};
}

function planTermFromWire(w: WirePlanTerm): PlanTerm {
  return {
    id: w.id as TermId,
    position: w.position,
    ...opt('label', w.label),
    kind: termKindFromWire(w.kind),
    nonCourse: w.nonCourse.map(nonCourseClaimFromWire),
  };
}

function planTermToWire(t: PlanTerm): WirePlanTerm {
  return {
    id: t.id,
    position: t.position,
    ...opt('label', t.label),
    kind: termKindToWire(t.kind),
    nonCourse: t.nonCourse.map(c => ({
      requirement: c.requirement,
      label: c.label,
    })),
  };
}

function selfCheckFromWire(w: WireSelfCheck): SelfCheck {
  return {
    requirement: w.requirement as RequirementId,
    reason: selfCheckReasonFromWire(w.reason),
    ...opt('note', w.note),
  };
}

function selfCheckToWire(s: SelfCheck): WireSelfCheck {
  return {
    requirement: s.requirement,
    reason: selfCheckReasonToWire(s.reason),
    ...opt('note', s.note),
  };
}

// ------------------------------------------------------------------- plan

export function planFromWire(w: WirePlan): Plan {
  return {
    id: w.id as PlanId,
    name: w.name,
    catalogYear: w.catalogYear,
    matriculation: w.matriculation,
    programs: w.programs.map(id => id as ProgramId),
    incomingCredit: w.incomingCredit.map(manualCardFromWire),
    terms: w.terms.map(planTermFromWire),
    selfChecks: w.selfChecks.map(selfCheckFromWire),
  };
}

export function planToWire(p: Plan): WirePlan {
  return {
    id: p.id,
    name: p.name,
    catalogYear: p.catalogYear,
    matriculation: p.matriculation,
    programs: p.programs,
    incomingCredit: p.incomingCredit.map(manualCardToWire),
    terms: p.terms.map(planTermToWire),
    selfChecks: p.selfChecks.map(selfCheckToWire),
  };
}

// ----------------------------------------------------------------- bundle

export function prereqFactFromWire(w: WirePrereqFact): PrereqFact {
  switch (w.kind) {
    case 'unknown':
      return {kind: 'unknown'};
    case 'none_required':
      return {
        kind: 'noneRequired',
        value: {publishedFor: w.value.publishedFor},
      };
    case 'requires':
      return {
        kind: 'requires',
        value: {
          publishedFor: w.value.publishedFor,
          expr: w.value.expr,
          published: w.value.published,
          ...opt('corequisite', w.value.corequisite),
        },
      };
    default:
      return unknownWire('prereqFact', w);
  }
}

function prerequisiteFromWire(w: WirePrerequisite): Prerequisite {
  return {course: w.course, fact: prereqFactFromWire(w.fact)};
}

function exclusionFromWire(w: WireExclusion): Exclusion {
  return {
    blocked: w.blocked,
    blocker: w.blocker,
    publishedFor: w.publishedFor,
    published: w.published,
  };
}

function creditLimitsFromWire(w: WireCreditLimits): CreditLimits {
  return {
    fallSpring: w.fallSpring,
    musicAndArchitecture: w.musicAndArchitecture,
    ...opt('summer', w.summer),
  };
}

/** `invalidations` has no domain field yet and is dropped (gap report §4). */
export function bundleFromWire(w: WirePlanBundle): PlanBundle {
  return {
    plan: planFromWire(w.plan),
    programs: w.programs.map(programFromWire),
    facts: courseFactsFromWire(w.facts),
    prerequisites: w.prerequisites.map(prerequisiteFromWire),
    exclusions: w.exclusions.map(exclusionFromWire),
    limits: creditLimitsFromWire(w.limits),
    today: w.today,
  };
}
