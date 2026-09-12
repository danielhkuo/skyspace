/**
 * `preview_placement` and `requirement_matches` (`08-board-interaction.md`), in
 * TypeScript until the wasm exports land. Reads the same index and filter the
 * evaluator reads, so preview and post-drop warning agree.
 */
import {
  courseKey,
  isAwayTerm,
  isOffTerm,
  isRiceTerm,
  plannedCredits,
  walkRequirements,
  type CourseCode,
  type CourseFacts,
  type CourseFilter,
  type Credits,
  type EntryId,
  type PlacementPreview,
  type PlanBundle,
  type PrereqVerdict,
  type Program,
  type Report,
  type RequirementId,
  type RequirementReport,
} from '../../domain';
import {canonical, courseInfo, filterMatches} from './filter';
import {evaluatePrereq} from './prereq';
import {buildTakenIndex, earliest} from './taken';

function unmetRequirements(report: Report | undefined): Set<RequirementId> {
  const out = new Set<RequirementId>();
  if (report === undefined) {
    return out;
  }
  const walk = (r: RequirementReport): void => {
    if (r.outcome.outcome !== 'met') {
      out.add(r.requirement);
    }
    r.children.forEach(walk);
  };
  report.programs.forEach(p => walk(p.root));
  return out;
}

/** Which requirements' progress would rise if `code` joined the plan: a filter pass, not a matching. */
export function requirementsRaisedBy(
  bundle: PlanBundle,
  report: Report | undefined,
  code: CourseCode,
): [Program['id'], RequirementId][] {
  const canon = canonical(bundle.facts, code);
  const open = unmetRequirements(report);
  const out: [Program['id'], RequirementId][] = [];
  for (const program of bundle.programs) {
    walkRequirements(program.root, requirement => {
      if (requirement.body.kind !== 'course' || !open.has(requirement.id)) {
        return;
      }
      if (
        filterMatches(requirement.body.filter, canon, bundle.facts) === 'yes'
      ) {
        out.push([program.id, requirement.id]);
      }
    });
  }
  return out;
}

export function previewPlacementInterim(
  bundle: PlanBundle,
  report: Report | undefined,
  course: CourseCode,
  moving: EntryId | undefined,
  credits: Credits,
): PlacementPreview[] {
  const {plan, facts} = bundle;
  const canon = canonical(facts, course);
  const index = buildTakenIndex(plan, facts, moving);
  const row = bundle.prerequisites.find(
    p => courseKey(canonical(facts, p.course)) === courseKey(canon),
  );
  const info = courseInfo(facts, canon);
  const already = earliest(index, facts, canon);
  const duplicateOf =
    already?.term !== undefined && info?.repeatable !== true
      ? already.term
      : undefined;
  const fills = requirementsRaisedBy(bundle, report, canon);

  const out: PlacementPreview[] = [];
  for (const term of plan.terms) {
    if (isOffTerm(term.kind)) {
      continue;
    }
    const movingHere =
      moving !== undefined &&
      isRiceTerm(term.kind) &&
      term.kind.rice.courses.some(c => c.id === moving);
    const creditsAfter = plannedCredits(term) + (movingHere ? 0 : credits);
    let prerequisites: PrereqVerdict = {kind: 'unknown'};
    if (isAwayTerm(term.kind)) {
      prerequisites = {kind: 'unknown'};
    } else if (row === undefined || row.fact.kind === 'unknown') {
      prerequisites = {kind: 'unknown'};
    } else if (row.fact.kind === 'noneRequired') {
      prerequisites = {kind: 'satisfied'};
    } else {
      const r = evaluatePrereq(
        row.fact.value.expr,
        term.position,
        index,
        facts,
      );
      if (r.truth === 'satisfied') {
        prerequisites = {kind: 'satisfied'};
      } else if (r.truth === 'unknown') {
        prerequisites = {kind: 'unknown'};
      } else if (r.sameTerm.length > 0 && r.missing.length === 0) {
        prerequisites = {kind: 'sameTerm', value: {courses: r.sameTerm}};
      } else {
        prerequisites = {
          kind: 'missing',
          value: {courses: [...r.missing, ...r.sameTerm]},
        };
      }
    }
    out.push({
      term: term.id,
      prerequisites,
      duplicateOf: duplicateOf === term.id ? undefined : duplicateOf,
      creditsAfter,
      fills,
    });
  }
  return out;
}

/** Which of `codes` match `requirement`'s filter, canonicalised through `facts`. */
export function requirementMatchesInterim(
  program: Program,
  requirementId: RequirementId,
  codes: CourseCode[],
  facts: CourseFacts,
): CourseCode[] {
  let filter: CourseFilter | undefined;
  walkRequirements(program.root, requirement => {
    if (
      requirement.id === requirementId &&
      requirement.body.kind === 'course'
    ) {
      filter = requirement.body.filter;
    }
  });
  const f = filter;
  if (f === undefined) {
    return [];
  }
  return codes.filter(
    code => filterMatches(f, canonical(facts, code), facts) === 'yes',
  );
}
