import {describe, expect, it} from 'vitest';

import {
  flattenRuleReports,
  formatCourseCode,
  isRiceTerm,
  parseCourseCode,
  type CourseCode,
} from '../../domain';
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

  it('counts an Analyzing Diversity course for its distribution slot too', () => {
    const university = report.programs.find(p => p.name === 'University');
    const find = (label: string) =>
      flattenRuleReports(report)
        .filter(r => r.label === label)
        .find(r =>
          university?.root.children.some(
            c => c === r || c.children.includes(r),
          ),
        );
    expect(find('Analyzing Diversity')?.outcome.outcome).toBe('met');
    expect(find('Distribution Group I')?.outcome.outcome).toBe('met');
  });

  it('gives the same report whatever order the terms are listed in', () => {
    const reversed = {
      ...bundle,
      plan: {...bundle.plan, terms: [...bundle.plan.terms].reverse()},
    };
    const other = engine.evaluate(reversed);
    expect(other.programs.map(p => p.progress.rulesMet)).toEqual(
      report.programs.map(p => p.progress.rulesMet),
    );
  });

  it('prefers the constrained slot: a card that fits two slots yields to one that fits one', () => {
    // COMP 140 fits Core › COMP 140 and Distribution Group III; COMP 182 fits
    // the group too. Both must land, so neither may be flagged as filling nothing.
    const fillsNothing = report.warnings.filter(
      w =>
        w.kind === 'fillsNoRequirement' &&
        ['COMP 140', 'COMP 182'].includes(formatCourseCode(w.value.course)),
    );
    expect(fillsNothing).toHaveLength(0);
  });

  it('holds a group at partial while a self-check inside it is unconfirmed', () => {
    const bscs = report.programs.find(p => p.program === BSCS_ID);
    const open = flattenRuleReports(report).find(
      r =>
        r.outcome.outcome === 'needsStudentCheck' &&
        r.outcome.confirmed === undefined,
    );
    expect(open).toBeDefined();
    expect(bscs?.root.outcome.outcome).not.toBe('met');
  });

  it('reports an empty plan as unmet, not partial', () => {
    const empty = engine.evaluate({
      ...bundle,
      plan: {...bundle.plan, terms: [], incomingCredit: [], selfChecks: []},
    });
    for (const program of empty.programs) {
      expect(program.root.outcome.outcome).toBe('unmet');
    }
  });

  it('says a prerequisite comes later when it is placed after the course', () => {
    const plan = bundle.plan;
    const fall26 = plan.terms.find(t => t.id === TERMS.fall26);
    const spring27 = plan.terms.find(t => t.id === TERMS.spring27);
    if (
      fall26 === undefined ||
      spring27 === undefined ||
      !isRiceTerm(fall26.kind) ||
      !isRiceTerm(spring27.kind)
    ) {
      throw new Error('fixture terms');
    }
    const comp382 = fall26.kind.rice.courses.find(
      c => formatCourseCode(c.course) === 'COMP 382',
    );
    if (comp382 === undefined) {
      throw new Error('COMP 382 in Fall 2026');
    }
    const moved = {
      ...plan,
      terms: plan.terms.map(t =>
        t.id === TERMS.fall26 && isRiceTerm(t.kind)
          ? {
              ...t,
              kind: {
                rice: {
                  ...t.kind.rice,
                  courses: t.kind.rice.courses.filter(c => c.id !== comp382.id),
                },
              },
            }
          : t.id === TERMS.spring27 && isRiceTerm(t.kind)
            ? {
                ...t,
                kind: {
                  rice: {
                    ...t.kind.rice,
                    courses: [...t.kind.rice.courses, comp382],
                  },
                },
              }
            : t,
      ),
    };
    const later = engine
      .evaluate({...bundle, plan: moved})
      .warnings.filter(
        w =>
          w.kind === 'prerequisite' &&
          formatCourseCode(w.value.course) === 'COMP 415' &&
          w.value.problem.kind === 'later',
      );
    expect(later).toHaveLength(1);
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
