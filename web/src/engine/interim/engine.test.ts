import {describe, expect, it} from 'vitest';

import {formatCourseCode, parseCourseCode, type CourseCode} from '../../domain';
import {
  BSCS_ID,
  COMP_415_ENTRY,
  RULES,
  TERMS,
  bscsProgram,
  bundle,
} from '../../fixtures/csStats';
import {engine} from '../index';

const code = (raw: string): CourseCode => {
  const parsed = parseCourseCode(raw);
  if (parsed === null) {
    throw new Error(raw);
  }
  return parsed;
};

describe('evaluate (interim)', () => {
  const report = engine.evaluate(bundle);

  it('reports one program per plan program, in sidebar order', () => {
    expect(report.programs.map(p => p.name)).toEqual([
      'University',
      'Computer Science, BSCS',
      'Statistics minor',
    ]);
  });

  it('flags COMP 415 in the same term as its prerequisite', () => {
    const hits = report.warnings.filter(
      w =>
        w.kind === 'prerequisite' &&
        formatCourseCode(w.value.course) === 'COMP 415',
    );
    expect(hits).toHaveLength(1);
    const hit = hits[0];
    expect(hit?.kind === 'prerequisite' && hit.value.problem.kind).toBe(
      'sameTerm',
    );
  });

  it('never counts a self-check toward rules met', () => {
    const bscs = report.programs.find(p => p.program === BSCS_ID);
    expect(bscs?.progress.selfChecks).toBeGreaterThan(0);
    expect(bscs?.progress.rulesCheckable).toBeLessThan(30);
  });
});

describe('previewPlacement (interim)', () => {
  const report = engine.evaluate(bundle);
  const previews = engine.previewPlacement(
    bundle,
    report,
    code('COMP 415'),
    COMP_415_ENTRY,
    400,
  );
  const byTerm = new Map(previews.map(p => [p.term, p]));

  it('marks terms before COMP 382 as missing', () => {
    expect(byTerm.get(TERMS.fall25)?.prerequisites.kind).toBe('missing');
  });

  it('marks the term holding COMP 382 as same term', () => {
    expect(byTerm.get(TERMS.fall26)?.prerequisites.kind).toBe('sameTerm');
  });

  it('marks later terms as satisfied and ignores the moving card itself', () => {
    expect(byTerm.get(TERMS.spring27)?.prerequisites.kind).toBe('satisfied');
    expect(byTerm.get(TERMS.spring28)?.prerequisites.kind).toBe('satisfied');
    expect(byTerm.get(TERMS.spring27)?.duplicateOf).toBeUndefined();
  });

  it('skips off terms and reports credits after the drop', () => {
    expect(byTerm.get(TERMS.spring28)?.creditsAfter).toBe(400);
  });
});

describe('ruleMatches (interim)', () => {
  it('resolves ECON 307 to STAT 310 through the alias table', () => {
    const matched = engine.ruleMatches(
      bscsProgram,
      RULES.probStat,
      [code('ECON 307'), code('COMP 140')],
      bundle.facts,
    );
    expect(matched.map(formatCourseCode)).toEqual(['ECON 307']);
  });
});
