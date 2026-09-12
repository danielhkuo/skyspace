/**
 * Requirement programs and rules, mirroring `skyspace-core::program`.
 * `RuleBody` and `CourseSelector` are internally tagged on `kind`.
 */
import type {CourseCode, CreditRange, Credits} from './course';
import type {ProgramId, RuleId} from './ids';
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

export type RuleBody =
  | {kind: 'all'; of: Rule[]}
  | {kind: 'select'; count: number; of: Rule[]}
  /** `semesters` is GA's "(minimum of 8 semesters)"; one slot per semester. */
  | {kind: 'course'; filter: CourseFilter; semesters: number}
  | {kind: 'credits'; minimum: Credits; scope: CreditScope; from: CourseFilter}
  | {kind: 'nonCourse'; nonCourseKind: NonCourseKind; description: string}
  | {kind: 'unverifiable'; text: string};

export type SourceRef = {
  url: string;
  anchor?: string;
};

export type Rule = {
  id: RuleId;
  label: string;
  hours?: CreditRange;
  source: SourceRef;
  body: RuleBody;
};

export type ProgramKind =
  'university' | 'major' | 'minor' | 'certificate' | 'concentration';

export type Program = {
  id: ProgramId;
  catalogYear: CatalogYear;
  slug: string;
  kind: ProgramKind;
  name: string;
  credential: string;
  totalCredits?: Credits;
  source: SourceRef;
  root: Rule;
  retiredRules: RuleId[];
};

/** Depth-first walk over a program's rules, document order. */
export function walkRules(rule: Rule, visit: (rule: Rule) => void): void {
  visit(rule);
  if (rule.body.kind === 'all' || rule.body.kind === 'select') {
    for (const child of rule.body.of) {
      walkRules(child, visit);
    }
  }
}

export function findRule(program: Program, id: RuleId): Rule | undefined {
  let found: Rule | undefined;
  walkRules(program.root, rule => {
    if (rule.id === id) {
      found = rule;
    }
  });
  return found;
}
