import {describe, expect, it} from 'vitest';

import {
  flattenRequirementReports,
  formatCourseCode,
  isRiceTerm,
  parseCourseCode,
  type CourseCode,
} from '../../domain';
import {
  BSCS_ID,
  COMP_415_ENTRY,
  REQS,
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
      flattenRequirementReports(report)
        .filter(r => r.label === label)
        .find(r =>
          university?.root.children.some(
            c => c === r || c.children.includes(r),
          ),
        );
    expect(find('Analyzing Diversity')?.outcome.outcome).toBe('met');
    // Group I holds a transfer card pinned by claim: shown, never met.
    const groupOne = find('Distribution Group I');
    expect(groupOne?.outcome.outcome).toBe('partial');
    expect(groupOne?.children.flatMap(c => c.claimedBy)).toHaveLength(1);
    expect(university?.progress.requirementsClaimed).toBe(1);
  });

  it('never lets AP or IB credit fill a distribution slot', () => {
    const ineligible = report.warnings.filter(
      w => w.kind === 'incomingCreditIneligible',
    );
    expect(ineligible).toHaveLength(0);
    const apOnGroup = flattenRequirementReports(report).some(
      r =>
        r.label.startsWith('Distribution Group') &&
        r.filledBy.some(e => String(e).includes('math-105')),
    );
    expect(apOnGroup).toBe(false);
  });

  it('gives the same report whatever order the terms are listed in', () => {
    const reversed = {
      ...bundle,
      plan: {...bundle.plan, terms: [...bundle.plan.terms].reverse()},
    };
    const other = engine.evaluate(reversed);
    expect(other.programs.map(p => p.progress.requirementsMet)).toEqual(
      report.programs.map(p => p.progress.requirementsMet),
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
    const open = flattenRequirementReports(report).find(
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

  it('flags a course whose catalog designation changed since it was placed', () => {
    // HIST 117 carries GRP1 and AD today. Pretend it also carried GRP2 when
    // the student placed it: the snapshot disagrees, so the card is flagged.
    const plan = bundle.plan;
    const withSnapshot = {
      ...plan,
      terms: plan.terms.map(t =>
        isRiceTerm(t.kind)
          ? {
              ...t,
              kind: {
                rice: {
                  ...t.kind.rice,
                  courses: t.kind.rice.courses.map(c =>
                    formatCourseCode(c.course) === 'HIST 117'
                      ? {
                          ...c,
                          observed: {
                            at: '2024-08-20T00:00:00Z',
                            catalogYear: 2024,
                            title: 'The world since 1492',
                            credits: {kind: 'fixed' as const, value: 300},
                            attributes: [
                              'AD' as const,
                              'GRP1' as const,
                              'GRP2' as const,
                            ],
                          },
                        }
                      : c,
                  ),
                },
              },
            }
          : t,
      ),
    };
    const changed = engine
      .evaluate({...bundle, plan: withSnapshot})
      .warnings.filter(w => w.kind === 'courseFactsChanged');
    expect(changed).toHaveLength(1);
    expect(
      changed[0]?.kind === 'courseFactsChanged' && changed[0].value.lost,
    ).toEqual(['GRP2']);
    // An unchanged snapshot is silent.
    expect(
      report.warnings.filter(w => w.kind === 'courseFactsChanged'),
    ).toHaveLength(0);
  });

  it('checks the two-department constraint from department data, and asks only when a department is unknown', () => {
    // With the transfer card in a Group I slot (no department on record) the
    // constraint is a self-check; without it, three departments are known
    // and the constraint is simply met.
    const label = 'From at least two departments';
    const withTransfer = flattenRequirementReports(report).find(
      r => r.label === label,
    );
    expect(withTransfer?.outcome.outcome).toBe('needsStudentCheck');
    const noTransfer = engine.evaluate({
      ...bundle,
      plan: {
        ...bundle.plan,
        incomingCredit: bundle.plan.incomingCredit.filter(
          c => c.code !== 'TRAN 100',
        ),
      },
    });
    const checked = flattenRequirementReports(noTransfer).find(
      r => r.label === label,
    );
    expect(checked?.outcome.outcome).toBe('met');
    expect(checked?.progress.requirementsCheckable).toBe(1);
    expect(
      noTransfer.warnings.some(
        w => w.kind === 'selfCheck' && w.value.label === label,
      ),
    ).toBe(false);
  });

  it('never counts a self-check toward requirements met', () => {
    const bscs = report.programs.find(p => p.program === BSCS_ID);
    expect(bscs?.progress.selfChecks).toBeGreaterThan(0);
    expect(bscs?.progress.requirementsCheckable).toBeLessThan(30);
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

describe('requirementMatches (interim)', () => {
  it('resolves ECON 307 to STAT 310 through the alias table', () => {
    const matched = engine.requirementMatches(
      bscsProgram,
      REQS.probStat,
      [code('ECON 307'), code('COMP 140')],
      bundle.facts,
    );
    expect(matched.map(formatCourseCode)).toEqual(['ECON 307']);
  });
});
