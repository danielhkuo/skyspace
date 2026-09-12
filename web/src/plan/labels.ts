/**
 * Copy for warnings and fills lines. One place, so the board, the drawer and
 * the PDF say the same thing (`design-prompt-dnd.md` "Copy").
 */
import {
  formatCourseCode,
  shortTermLabel,
  type PlacementPreview,
  type Plan,
  type TermId,
  type Warning,
} from '../domain';

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
      return `Also in ${warning.value.terms.map(t => termName(plan, t)).join(' and ')}.`;
    case 'overSemesterLoad':
      return 'Above the 18 hours Rice allows without written approval. Not a cap.';
    case 'programUnavailable':
      return 'No reviewed rules for this program yet.';
    case 'programYearSubstituted':
      return `Evaluated with the ${warning.value.used}-${String(warning.value.used + 1).slice(2)} rules instead.`;
    case 'ruleChoiceUnmatched':
      return "Doesn't match this rule · your choice.";
    case 'ruleChoiceMissing':
      return warning.value.retired
        ? 'The rule you chose was retired in review.'
        : 'The rule you chose no longer exists.';
    case 'attributeUnknown':
      return 'We hold no distribution data for this course yet.';
    case 'prerequisite': {
      const prereq = formatCourseCode(warning.value.prerequisite);
      const problem = warning.value.problem;
      if (problem.kind === 'sameTerm') {
        return `Prerequisite ${prereq} is in the same term. Rice checks prerequisites at registration.`;
      }
      if (problem.kind === 'later') {
        return `Prerequisite ${prereq} comes later. Rice checks prerequisites at registration.`;
      }
      return `Prerequisite ${prereq} is not in this plan. Rice checks prerequisites at registration.`;
    }
    case 'prerequisiteUnparsed':
      return `Prerequisites: ${warning.value.published}`;
    case 'mutuallyExclusive':
      return warning.value.published;
    case 'seasonUnlikely':
      return `Usually offered in another season (${warning.value.termsSeen} terms seen).`;
    case 'selfCheck':
      return warning.value.text;
  }
}

/** The subject the warning is about, for the drawer's first column. */
export function warningSubject(warning: Warning): string {
  switch (warning.kind) {
    case 'fillsNoRequirement':
    case 'attributeUnknown':
    case 'prerequisite':
    case 'prerequisiteUnparsed':
    case 'seasonUnlikely':
      return formatCourseCode(warning.value.course);
    case 'duplicateCourse':
      return formatCourseCode(warning.value.course);
    case 'mutuallyExclusive':
      return formatCourseCode(warning.value.blocked);
    case 'selfCheck':
      return warning.value.label;
    case 'overSemesterLoad':
    case 'ruleChoiceUnmatched':
    case 'ruleChoiceMissing':
      return 'Term';
    case 'programUnavailable':
    case 'programYearSubstituted':
      return 'Program';
  }
}

/** Which term a warning belongs to, when it has one. */
export function warningTerm(warning: Warning): TermId | undefined {
  switch (warning.kind) {
    case 'fillsNoRequirement':
    case 'overSemesterLoad':
    case 'ruleChoiceUnmatched':
    case 'ruleChoiceMissing':
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
    case 'ruleChoiceUnmatched':
      return "Doesn't match this rule · your choice";
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
