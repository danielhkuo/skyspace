/**
 * Three-valued prerequisite evaluation (the Kleene table in `02-domain.md`).
 * `unparsed` is unknown, and an unknown clause never resolves to satisfied.
 */
import {
  compareTermPosition,
  type CourseCode,
  type CourseFacts,
  type PrereqExpr,
  type TermId,
  type TermPosition,
} from '../../domain';
import {earliest, type TakenIndex} from './taken';

export type Truth = 'satisfied' | 'missing' | 'unknown';

export type PrereqResult = {
  truth: Truth;
  /** Named courses absent from the plan. Empty when satisfied. */
  missing: CourseCode[];
  sameTerm: CourseCode[];
  /** Named courses placed in a later term than the course that needs them. */
  later: {course: CourseCode; term: TermId}[];
};

type CourseVerdict = {truth: Truth; sameTerm: boolean; later?: TermId};

const NONE: Omit<PrereqResult, 'truth'> = {
  missing: [],
  sameTerm: [],
  later: [],
};

function courseTruth(
  code: CourseCode,
  target: TermPosition,
  index: TakenIndex,
  facts: CourseFacts,
): CourseVerdict {
  const placed = earliest(index, facts, code);
  if (placed === undefined) {
    return {truth: 'missing', sameTerm: false};
  }
  if (placed.when.kind === 'incoming') {
    return {truth: 'satisfied', sameTerm: false};
  }
  const order = compareTermPosition(placed.when.value, target);
  if (order < 0) {
    return {truth: 'satisfied', sameTerm: false};
  }
  if (order === 0) {
    return {truth: 'missing', sameTerm: true};
  }
  return {truth: 'missing', sameTerm: false, later: placed.term};
}

function allOf(a: Truth, b: Truth): Truth {
  if (a === 'missing' || b === 'missing') {
    return 'missing';
  }
  if (a === 'unknown' || b === 'unknown') {
    return 'unknown';
  }
  return 'satisfied';
}

function anyOf(a: Truth, b: Truth): Truth {
  if (a === 'satisfied' || b === 'satisfied') {
    return 'satisfied';
  }
  if (a === 'unknown' || b === 'unknown') {
    return 'unknown';
  }
  return 'missing';
}

/** Evaluate `expr` for a course placed in `target`. */
export function evaluatePrereq(
  expr: PrereqExpr,
  target: TermPosition,
  index: TakenIndex,
  facts: CourseFacts,
): PrereqResult {
  switch (expr.kind) {
    case 'unparsed':
      return {truth: 'unknown', ...NONE};
    case 'course': {
      const v = courseTruth(expr.value, target, index, facts);
      if (v.truth !== 'missing') {
        return {truth: v.truth, ...NONE};
      }
      if (v.sameTerm) {
        return {truth: 'missing', ...NONE, sameTerm: [expr.value]};
      }
      if (v.later !== undefined) {
        return {
          truth: 'missing',
          ...NONE,
          later: [{course: expr.value, term: v.later}],
        };
      }
      return {truth: 'missing', ...NONE, missing: [expr.value]};
    }
    case 'all': {
      const out: PrereqResult = {truth: 'satisfied', ...NONE};
      for (const child of expr.value) {
        const r = evaluatePrereq(child, target, index, facts);
        out.truth = allOf(out.truth, r.truth);
        out.missing = [...out.missing, ...r.missing];
        out.sameTerm = [...out.sameTerm, ...r.sameTerm];
        out.later = [...out.later, ...r.later];
      }
      return out;
    }
    case 'any': {
      let truth: Truth = 'missing';
      const results = expr.value.map(child =>
        evaluatePrereq(child, target, index, facts),
      );
      for (const r of results) {
        truth = anyOf(truth, r.truth);
      }
      if (truth === 'satisfied') {
        return {truth, ...NONE};
      }
      // Nothing in the branch is met. One warning, not one per option: the
      // nearest miss speaks for the group (same term, then later, then absent),
      // because the wire `Prerequisite` names a single course.
      const first =
        results.find(r => r.sameTerm.length > 0) ??
        results.find(r => r.later.length > 0) ??
        results.find(r => r.missing.length > 0);
      return first === undefined ? {truth, ...NONE} : {...first, truth};
    }
  }
}
