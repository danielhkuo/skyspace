/**
 * Copy for warnings and fills lines. One place, so the board, the drawer and
 * the PDF say the same thing (`design-prompt-dnd.md` "Copy").
 */
import {
  ATTRIBUTE_LABEL,
  formatCredits,
  formatCourseCode,
  type EntryId,
  isRiceTerm,
  isAwayTerm,
  shortTermLabel,
  walkRequirements,
  type CreditOrigin,
  type FillBasis,
  type PlacementPreview,
  type Plan,
  type Program,
  type RequirementId,
  type SelfCheckReason,
  type TermId,
  type Warning,
} from '../domain';

export const FILL_BASIS_LABEL: Record<FillBasis['kind'], string> = {
  earlierCatalog: 'It counted under the catalog year I took it',
  advisorApproved: 'My advisor approved the substitution',
  petitionGranted: 'A petition was granted',
  registrarPosted: 'The registrar posted it on my degree audit',
  unsure: "I'm not sure it counts",
};

const ORIGIN_NAME: Record<CreditOrigin, string> = {
  transfer: 'Transfer',
  advancedPlacement: 'AP',
  internationalBaccalaureate: 'IB',
  studyAbroad: 'Study abroad',
  other: 'Outside',
};

/** The first sentence of an unmatched-pin warning: what the student said, or that they said nothing. */
export function basisText(basis: FillBasis | undefined): string {
  switch (basis?.kind) {
    case 'earlierCatalog':
      return `Doesn't match this requirement in the plan's catalog year; you said it counted${basis.catalogYear === undefined ? '' : ` in ${basis.catalogYear}-${String(basis.catalogYear + 1).slice(2)}`}.`;
    case 'advisorApproved':
      return `Doesn't match this requirement as published; you recorded an advisor-approved substitution${basis.who === undefined ? '' : ` (${basis.who}${basis.on === undefined ? '' : `, ${basis.on}`})`}.`;
    case 'petitionGranted':
      return `Doesn't match this requirement as published; you recorded a granted petition${basis.on === undefined ? '' : ` (${basis.on})`}.`;
    case 'registrarPosted':
      return "Doesn't match this requirement as published; you recorded that the registrar posted it on your degree audit.";
    case 'unsure':
    case undefined:
      return "Doesn't match this requirement as published, and no reason is recorded.";
  }
}

export const SELF_CHECK_REASON_LABEL: Record<SelfCheckReason, string> = {
  transfer: 'Transfer credit',
  apOrIb: 'AP or IB',
  studyAbroad: 'Study abroad',
  advisorApproved: 'Advisor-approved substitution',
  other: 'Other',
};

/** "Core › COMP 140" for every requirement of a program: the area, then the requirement. */
export function requirementPaths(program: Program): Map<RequirementId, string> {
  const paths = new Map<RequirementId, string>();
  const root = program.root;
  if (root.body.kind !== 'all' && root.body.kind !== 'select') {
    paths.set(root.id, root.label);
    return paths;
  }
  for (const area of root.body.of) {
    const short = area.label.replace(/ Requirements?$/i, '');
    walkRequirements(area, requirement => {
      paths.set(
        requirement.id,
        requirement === area || requirement.label === area.label
          ? area.label
          : `${short} › ${requirement.label}`,
      );
    });
  }
  return paths;
}

export function termName(plan: Plan, id: TermId): string {
  const term = plan.terms.find(t => t.id === id);
  return term === undefined ? 'a term' : shortTermLabel(term.position);
}

/** The sentence a warning row shows. Says what Rice will do, not what the student did wrong. */
export function warningText(warning: Warning, plan: Plan): string {
  switch (warning.kind) {
    case 'fillsNoRequirement':
      return 'Fills no requirement.';
    case 'duplicateCourse':
      return `Also in ${[...new Set(warning.value.terms.map(t => termName(plan, t)))].join(' and ')}.`;
    case 'overSemesterLoad':
      return 'Above 18 hours without written approval. Not a hard cap.';
    case 'programUnavailable':
      return 'No reviewed requirements for this program yet.';
    case 'programYearSubstituted':
      return `Evaluated with the ${warning.value.used}-${String(warning.value.used + 1).slice(2)} requirements instead.`;
    case 'requirementChoiceUnmatched':
      return `${basisText(warning.value.basis)} We can't verify if this will count; check your degree audit in Esther or ask your advisor.`;
    case 'incomingCreditIneligible':
      return `${ORIGIN_NAME[warning.value.origin]} credit counts toward your total and your major, not toward distribution or Analyzing Diversity. Not counted here; your Esther degree audit shows what the registrar posted.`;
    case 'doubleCounted':
      return 'Counted toward a major and a minor or certificate. Check the General Announcements for any overlap limits.';
    case 'courseFactsChanged': {
      const v = warning.value;
      const year = `${v.observedYear}-${String(v.observedYear + 1).slice(2)}`;
      const parts: string[] = [];
      if (v.lost.length > 0) {
        parts.push(
          `carried ${v.lost.map(a => ATTRIBUTE_LABEL[a]).join(' and ')} when you added it (${year}) and no longer does`,
        );
      }
      if (v.gained.length > 0) {
        parts.push(
          `now carries ${v.gained.map(a => ATTRIBUTE_LABEL[a]).join(' and ')}, which it did not in ${year}`,
        );
      }
      if (v.creditsBefore !== undefined && v.creditsNow !== undefined) {
        parts.push(
          `was ${formatCredits(v.creditsBefore)} hours in ${year} and is ${formatCredits(v.creditsNow)} now`,
        );
      }
      return `The catalog changed: it ${parts.join('; ')}. Check your degree audit in Esther, or pin it to the requirement with "counted under the catalog year I took it".`;
    }
    case 'manualCredits':
      return `${formatCredits(warning.value.credits)} hours entered by hand: it counts as hours, not as a specific course. We can't verify what the registrar will post; check your transfer evaluation in Esther.`;
    case 'requirementChoiceMissing':
      return warning.value.retired
        ? 'The requirement you chose was retired in review.'
        : 'The requirement you chose no longer exists.';
    case 'attributeUnknown':
      return 'We hold no distribution data for this course yet.';
    case 'prerequisite': {
      const prereq = formatCourseCode(warning.value.prerequisite);
      const problem = warning.value.problem;
      if (problem.kind === 'sameTerm') {
        return `Prerequisite ${prereq} is in the same term.`;
      }
      if (problem.kind === 'later') {
        return `Prerequisite ${prereq} comes later.`;
      }
      return `Prerequisite ${prereq} is not in this plan.`;
    }
    case 'prerequisiteUnparsed':
      return `Prerequisites: ${warning.value.published}`;
    case 'mutuallyExclusive':
      return warning.value.published;
    case 'seasonUnlikely':
      return `Usually offered in another season (${warning.value.termsSeen} terms seen).`;
    case 'selfCheck':
      return "We can't verify this. Not counted until you confirm it with a reason.";
  }
}

/** A card's code on the board, for warnings that name an entry rather than a course. */
function entryCode(plan: Plan, entry: EntryId): string {
  for (const card of plan.incomingCredit) {
    if (card.id === entry) {
      return card.code;
    }
  }
  for (const term of plan.terms) {
    if (isRiceTerm(term.kind)) {
      const hit = term.kind.rice.courses.find(c => c.id === entry);
      if (hit !== undefined) {
        return formatCourseCode(hit.course);
      }
    } else if (isAwayTerm(term.kind)) {
      const hit = term.kind.away.cards.find(c => c.id === entry);
      if (hit !== undefined) {
        return hit.code;
      }
    }
  }
  return 'Card';
}

/** A pin the student already explained: kept, but out of the loud count. */
/**
 * What the warnings table and its badge show. A self-check is not a defect
 * in the plan: it is a requirement Skyspace cannot verify, and the sidebar's
 * "Check yourself" row is where it is answered. Listing every one as a
 * warning made an empty plan open with eleven of them.
 */
export function planWarnings(warnings: Warning[]): Warning[] {
  return warnings.filter(w => w.kind !== 'selfCheck');
}

export function isRecordedClaim(warning: Warning): boolean {
  return (
    (warning.kind === 'requirementChoiceUnmatched' &&
      warning.value.basis !== undefined &&
      warning.value.basis.kind !== 'unsure') ||
    warning.kind === 'doubleCounted'
  );
}

/** The card a warning is about, when it names one. */
export function warningEntry(warning: Warning): EntryId | undefined {
  switch (warning.kind) {
    case 'fillsNoRequirement':
    case 'requirementChoiceUnmatched':
    case 'requirementChoiceMissing':
    case 'attributeUnknown':
    case 'incomingCreditIneligible':
    case 'doubleCounted':
    case 'manualCredits':
    case 'courseFactsChanged':
      return warning.value.entry;
    default:
      return undefined;
  }
}

/** The subject the warning is about, for the panel's first column. */
export function warningSubject(warning: Warning, plan: Plan): string {
  switch (warning.kind) {
    case 'fillsNoRequirement':
    case 'attributeUnknown':
    case 'prerequisite':
    case 'prerequisiteUnparsed':
    case 'seasonUnlikely':
      return formatCourseCode(warning.value.course);
    case 'duplicateCourse':
    case 'courseFactsChanged':
      return formatCourseCode(warning.value.course);
    case 'mutuallyExclusive':
      return formatCourseCode(warning.value.blocked);
    case 'selfCheck':
      return warning.value.label;
    case 'incomingCreditIneligible':
    case 'requirementChoiceUnmatched':
    case 'requirementChoiceMissing':
    case 'manualCredits':
      return entryCode(plan, warning.value.entry);
    case 'doubleCounted':
      return formatCourseCode(warning.value.course);
    case 'overSemesterLoad':
      return 'Term';
    case 'programUnavailable':
    case 'programYearSubstituted':
      return 'Program';
  }
}

/** Which term a warning belongs to, when it has one. */
export function warningTerm(warning: Warning): TermId | undefined {
  switch (warning.kind) {
    case 'courseFactsChanged':
    case 'fillsNoRequirement':
    case 'overSemesterLoad':
    case 'requirementChoiceUnmatched':
    case 'requirementChoiceMissing':
    case 'attributeUnknown':
    case 'prerequisite':
    case 'prerequisiteUnparsed':
    case 'seasonUnlikely':
      return warning.value.term;
    case 'mutuallyExclusive':
      return warning.value.blockedTerm;
    case 'duplicateCourse':
      return warning.value.terms[0];
    default:
      return undefined;
  }
}

/** The short chip a card shows. Empty for warnings the fills line already covers. */
export function warningChip(warning: Warning): string | undefined {
  switch (warning.kind) {
    case 'prerequisite': {
      const prereq = formatCourseCode(warning.value.prerequisite);
      return warning.value.problem.kind === 'sameTerm'
        ? `Prerequisite ${prereq} is in the same term`
        : `Prerequisite ${prereq} is not in this plan`;
    }
    case 'mutuallyExclusive':
      return `Mutually exclusive with ${formatCourseCode(warning.value.blocker)}`;
    case 'duplicateCourse':
      return 'Also elsewhere in the plan';
    case 'requirementChoiceUnmatched':
      return 'On your say-so · not verified';
    case 'incomingCreditIneligible':
      return 'AP/IB: not for distribution';
    case 'doubleCounted':
      return 'Also counts toward a minor';
    case 'manualCredits':
      return 'Hours by hand · not verified';
    case 'courseFactsChanged':
      return 'Catalog changed since you added it';
    case 'prerequisiteUnparsed':
      return 'Prerequisites could not be read';
    default:
      return undefined;
  }
}

/** The preview line a term header shows while a card is in the air. */
export function previewLine(
  preview: PlacementPreview,
  plan: Plan,
): {text: string; tone: 'amber' | 'green' | 'neutral'} {
  if (preview.duplicateOf !== undefined) {
    return {
      text: `already in ${termName(plan, preview.duplicateOf)}`,
      tone: 'amber',
    };
  }
  const p = preview.prerequisites;
  switch (p.kind) {
    case 'satisfied':
      return {text: '✓ prerequisites met', tone: 'green'};
    case 'sameTerm':
      return {
        text: `⚠ ${p.value.courses.map(formatCourseCode).join(', ')} is in this term`,
        tone: 'amber',
      };
    case 'missing':
      return {
        text: `⚠ before ${p.value.courses.map(formatCourseCode).join(', ')}`,
        tone: 'amber',
      };
    case 'unknown':
      return {text: 'prerequisites unknown', tone: 'neutral'};
  }
}
