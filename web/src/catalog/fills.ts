/**
 * "Fills in your plan" for one course: the requirements its card fills if it is on
 * the board, or the requirements it could fill if it is not. Paths read like the
 * board's fills line: "Core Requirements › COMP 140".
 */
import {
  isRiceTerm,
  sameCourse,
  shortTermLabel,
  walkRequirements,
  type CourseCode,
  type EntryId,
  type PlanBundle,
  type Report,
  type Requirement,
  type RequirementReport,
} from '../domain';
import {engine} from '../engine';

export type FillEntry = {program: string; path: string};

export type FillsSummary = {
  planName: string;
  /** "Fall 2026" when the course is on the board. */
  placedIn?: string;
  /** Requirements the placed card fills; empty when placed but filling nothing. */
  fills: FillEntry[];
  /** Requirements a not-yet-placed course matches. */
  candidates: FillEntry[];
};

function pathOf(area: string | undefined, label: string): string {
  return area === undefined || area === label
    ? label
    : `${area.replace(/ Requirements?$/i, '')} › ${label}`;
}

export function placedEntry(
  bundle: PlanBundle,
  code: CourseCode,
): {entry: EntryId; term: string} | undefined {
  for (const term of bundle.plan.terms) {
    if (isRiceTerm(term.kind)) {
      const hit = term.kind.rice.courses.find(c => sameCourse(c.course, code));
      if (hit !== undefined) {
        return {entry: hit.id, term: shortTermLabel(term.position)};
      }
    }
  }
  return undefined;
}

export function summarizeFills(
  bundle: PlanBundle,
  report: Report,
  code: CourseCode,
): FillsSummary {
  const placed = placedEntry(bundle, code);
  const summary: FillsSummary = {
    planName: bundle.plan.name,
    fills: [],
    candidates: [],
  };

  if (placed !== undefined) {
    summary.placedIn = placed.term;
    for (const program of report.programs) {
      const walk = (
        requirement: RequirementReport,
        area: string | undefined,
      ): void => {
        if (requirement.filledBy.includes(placed.entry)) {
          summary.fills.push({
            program: program.name,
            path: pathOf(area, requirement.label),
          });
        }
        for (const child of requirement.children) {
          walk(child, area ?? child.label);
        }
      };
      for (const area of program.root.children) {
        walk(area, area.label);
      }
    }
    return summary;
  }

  for (const program of bundle.programs) {
    const areas = new Map<Requirement, string>();
    const root = program.root;
    if (root.body.kind === 'all' || root.body.kind === 'select') {
      for (const area of root.body.of) {
        walkRequirements(area, requirement =>
          areas.set(requirement, area.label),
        );
      }
    }
    walkRequirements(root, requirement => {
      if (requirement.body.kind !== 'course') {
        return;
      }
      if (
        engine.requirementMatches(program, requirement.id, [code], bundle.facts)
          .length > 0
      ) {
        summary.candidates.push({
          program: program.name,
          path: pathOf(areas.get(requirement), requirement.label),
        });
      }
    });
  }
  return summary;
}
