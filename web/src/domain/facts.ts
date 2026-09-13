/**
 * What the engine is given beside the plan: course facts, prerequisite and
 * exclusion rows, and the whole `PlanBundle`. Mirrors `skyspace-core`.
 */
import {
  courseKey,
  sameCourse,
  type CourseCode,
  type CreditRange,
  type Credits,
} from './course';
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

/** The first code the General Announcements print; aliases (cross-lists) resolve to it. */
export function canonical(facts: CourseFacts, code: CourseCode): CourseCode {
  for (const [alias, target] of facts.aliases) {
    if (sameCourse(alias, code)) {
      return target;
    }
  }
  return code;
}

/** What we hold about a course, by canonical code; `undefined` when never scraped. */
export function courseInfo(
  facts: CourseFacts,
  code: CourseCode,
): CourseInfo | undefined {
  const key = courseKey(canonical(facts, code));
  return facts.courses.find(c => courseKey(c.code) === key);
}

/**
 * What the catalog said about a course when the student placed it. Kept on
 * the card so a later catalog can be compared against it: the only way to
 * notice that a designation, hours or title changed after the fact
 * (`09-cannot-verify.md` A1, A6, A8).
 */
export type ObservedFacts = {
  /** ISO timestamp of the placement. */
  at: string;
  /** The plan's catalog year at the time. */
  catalogYear: CatalogYear;
  title: string;
  credits: CreditRange;
  attributes: Attribute[];
};

export function observeCourse(
  facts: CourseFacts,
  code: CourseCode,
  catalogYear: CatalogYear,
): ObservedFacts | undefined {
  const info = courseInfo(facts, code);
  if (info === undefined) {
    return undefined;
  }
  return {
    at: new Date().toISOString(),
    catalogYear,
    title: info.title,
    credits: info.credits,
    attributes: [...info.attributes].sort(),
  };
}
