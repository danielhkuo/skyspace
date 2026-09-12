/**
 * Three-valued prerequisite evaluation (the Kleene table in `02-domain.md`).
 * `unparsed` is unknown, and an unknown clause never resolves to satisfied.
 */
import {
  compareTermPosition,
  type CourseCode,
  type CourseFacts,
  type PrereqExpr,
  type TermPosition,
} from '../../domain';
import {earliest, type TakenIndex} from './taken';

export type Truth = 'satisfied' | 'missing' | 'unknown';

export type PrereqResult = {
  truth: Truth;
  /** Named courses that are absent, in the same term, or later. Empty when satisfied. */
  missing: CourseCode[];
  sameTerm: CourseCode[];
};

function courseTruth(
  code: CourseCode,
  target: TermPosition,
  index: TakenIndex,
  facts: CourseFacts,
): {truth: Truth; sameTerm: boolean} {
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
  return {truth: 'missing', sameTerm: order === 0};
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
      return {truth: 'unknown', missing: [], sameTerm: []};
    case 'course': {
      const {truth, sameTerm} = courseTruth(expr.value, target, index, facts);
      return {
        truth,
        missing: truth === 'missing' && !sameTerm ? [expr.value] : [],
        sameTerm: sameTerm ? [expr.value] : [],
      };
    }
    case 'all': {
      let truth: Truth = 'satisfied';
      const missing: CourseCode[] = [];
      const sameTerm: CourseCode[] = [];
      for (const child of expr.value) {
        const r = evaluatePrereq(child, target, index, facts);
        truth = allOf(truth, r.truth);
        missing.push(...r.missing);
        sameTerm.push(...r.sameTerm);
      }
      return {truth, missing, sameTerm};
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
        return {truth, missing: [], sameTerm: []};
      }
      // Nothing in the branch is met: name every option so the student can pick one.
      return {
        truth,
        missing: results.flatMap(r => r.missing),
        sameTerm: results.flatMap(r => r.sameTerm),
      };
    }
  }
}
