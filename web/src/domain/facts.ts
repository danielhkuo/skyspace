/**
 * What the engine is given beside the plan: course facts, prerequisite and
 * exclusion rows, and the whole `PlanBundle`. Mirrors `skyspace-core`.
 */
import type {CourseCode, CreditRange, Credits} from './course';
import type {EntryId, TermId} from './ids';
import type {Plan} from './plan';
import type {Attribute, Program} from './program';
import type {Season, TermPosition, CatalogYear} from './term';

export type CourseInfo = {
  code: CourseCode;
  title: string;
  credits: CreditRange;
  attributes: Attribute[];
  /** From Rice's "Repeatable for Credit." sentence. */
  repeatable: boolean;
  seasonsOffered: Season[];
  termsObserved: number;
  /** Offered in the current term; drives the suggestion chips' dot. */
  offeredNow: boolean;
};

/** Serialised as two vectors because a struct-keyed map cannot cross serde. */
export type CourseFacts = {
  courses: CourseInfo[];
  aliases: [CourseCode, CourseCode][];
};

/** Adjacent tagging: `{kind, value}`. */
export type PrereqExpr =
  | {kind: 'course'; value: CourseCode}
  | {kind: 'all'; value: PrereqExpr[]}
  | {kind: 'any'; value: PrereqExpr[]}
  | {kind: 'unparsed'; value: string};

export type PrereqFact =
  | {kind: 'unknown'}
  | {kind: 'noneRequired'; value: {publishedFor: CatalogYear}}
  | {
      kind: 'requires';
      value: {
        publishedFor: CatalogYear;
        expr: PrereqExpr;
        published: string;
        corequisite?: CourseCode;
      };
    };

export type Prerequisite = {
  course: CourseCode;
  fact: PrereqFact;
};

export type Exclusion = {
  blocked: CourseCode;
  blocker: CourseCode;
  publishedFor: CatalogYear;
  published: string;
};

/** Rice's normal semester load, for the informational note only (`04-planning.md`). */
export type CreditLimits = {
  fallSpring: Credits;
  musicAndArchitecture: Credits;
  summer?: Credits;
};

export type PlanBundle = {
  plan: Plan;
  programs: Program[];
  facts: CourseFacts;
  prerequisites: Prerequisite[];
  exclusions: Exclusion[];
  limits: CreditLimits;
  today: TermPosition;
};

/** A course's placement on the board, by earliest appearance. */
export type Placed = {
  when: {kind: 'incoming'} | {kind: 'term'; value: TermPosition};
  term?: TermId;
  entry?: EntryId;
};
