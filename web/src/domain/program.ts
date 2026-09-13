/**
 * Requirement programs and requirements, mirroring `skyspace-core::program`.
 * `RequirementBody` and `CourseSelector` are internally tagged on `kind`.
 */
import type {CourseCode, CreditRange, Credits} from './course';
import type {ProgramId, RequirementId} from './ids';
import type {CatalogYear} from './term';

export type Attribute = 'AD' | 'GRP1' | 'GRP2' | 'GRP3';

export type CourseSelector =
  | {kind: 'code'; code: CourseCode}
  | {kind: 'subject'; subject: string}
  | {kind: 'numberRange'; subject?: string; low: number; high: number}
  | {kind: 'attribute'; attribute: Attribute};

export type CourseFilter = {
  include: CourseSelector[];
  exclude: CourseSelector[];
};

export type CreditScope = 'any' | 'additional';

export type NonCourseKind = 'proficiencyExam' | 'portfolio' | 'other';

export type RequirementBody =
  | {kind: 'all'; of: Requirement[]}
  | {kind: 'select'; count: number; of: Requirement[]}
  /** `semesters` is GA's "(minimum of 8 semesters)"; one slot per semester. */
  | {kind: 'course'; filter: CourseFilter; semesters: number}
  | {kind: 'credits'; minimum: Credits; scope: CreditScope; from: CourseFilter}
  | {kind: 'nonCourse'; nonCourseKind: NonCourseKind; description: string}
  | {kind: 'unverifiable'; text: string}
  /**
   * A constraint across the sibling course slots: their cards must come from
   * at least `minimum` departments. Checked when every card's department is
   * known; a self-check otherwise. Proposed for `03-requirements.md`.
   */
  | {kind: 'distinctDepartments'; minimum: number; text: string};

export type SourceRef = {
  url: string;
  anchor?: string;
};

export type Requirement = {
  id: RequirementId;
  label: string;
  hours?: CreditRange;
  source: SourceRef;
  body: RequirementBody;
};

export type ProgramKind =
  'university' | 'major' | 'minor' | 'certificate' | 'concentration';

/**
 * What a picker needs to name a program, mirroring the API's
 * `ProgramSummary` (`06-api.md`). The rule tree comes with the plan's bundle.
 */
export type ProgramSummary = {
  id: ProgramId;
  slug: string;
  kind: ProgramKind;
  name: string;
  credential: string;
  /** Catalog years with a reviewed version on file. */
  catalogYears: CatalogYear[];
  totalCredits?: Credits;
};

export type Program = {
  id: ProgramId;
  catalogYear: CatalogYear;
  slug: string;
  kind: ProgramKind;
  name: string;
  credential: string;
  totalCredits?: Credits;
  source: SourceRef;
  root: Requirement;
  retiredRequirements: RequirementId[];
};

/** Depth-first walk over a program's requirements, document order. */
export function walkRequirements(
  requirement: Requirement,
  visit: (requirement: Requirement) => void,
): void {
  visit(requirement);
  if (requirement.body.kind === 'all' || requirement.body.kind === 'select') {
    for (const child of requirement.body.of) {
      walkRequirements(child, visit);
    }
  }
}

export function findRequirement(
  program: Program,
  id: RequirementId,
): Requirement | undefined {
  let found: Requirement | undefined;
  walkRequirements(program.root, requirement => {
    if (requirement.id === id) {
      found = requirement;
    }
  });
  return found;
}
