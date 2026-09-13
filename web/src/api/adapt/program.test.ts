import {describe, expect, it} from 'vitest';

import type {Program as WireProgram} from '../generated/Program';
import type {ProgramSummary as WireProgramSummary} from '../generated/ProgramSummary';
import {
  courseSelectorFromWire,
  nonCourseKindFromWire,
  programFromWire,
  programSummaryFromWire,
  requirementFromWire,
} from './program';

/** Every `RequirementBody`, `CourseSelector` and `NonCourseKind` kind. */
const wireProgram: WireProgram = {
  id: 'prog-cs',
  catalogYear: 2026,
  slug: 'computer-science-bs',
  kind: 'major',
  name: 'Computer Science',
  credential: 'BS',
  totalCredits: null,
  source: {url: 'https://ga.rice.edu/cs', anchor: null},
  review: {reviewedBy: 'someone', publishedAt: 1_757_600_000},
  root: {
    id: 'req-root',
    label: 'Degree requirements',
    hours: {kind: 'range', value: {min: 12000, max: 12600}},
    source: {url: 'https://ga.rice.edu/cs', anchor: 'requirements'},
    body: {
      kind: 'all',
      of: [
        {
          id: 'req-core',
          label: 'Core',
          source: {url: 'https://ga.rice.edu/cs'},
          body: {
            kind: 'course',
            filter: {
              include: [
                {kind: 'code', code: {subject: 'COMP', number: '140'}},
                {kind: 'subject', subject: 'COMP'},
                {kind: 'number_range', subject: 'COMP', low: 300, high: 499},
                {kind: 'number_range', low: 500, high: 699},
                {kind: 'attribute', attribute: 'GRP3'},
              ],
              exclude: [{kind: 'code', code: {subject: 'COMP', number: '100'}}],
            },
            semesters: 1,
          },
        },
        {
          id: 'req-select',
          label: 'Electives',
          source: {url: 'https://ga.rice.edu/cs'},
          body: {
            kind: 'select',
            count: 2,
            of: [
              {
                id: 'req-credits',
                label: 'Upper-level credits',
                source: {url: 'https://ga.rice.edu/cs'},
                body: {
                  kind: 'credits',
                  minimum: 900,
                  scope: 'additional',
                  from: {
                    include: [{kind: 'subject', subject: 'COMP'}],
                    exclude: [],
                  },
                },
              },
            ],
          },
        },
        {
          id: 'req-exam',
          label: 'Proficiency exam',
          source: {url: 'https://ga.rice.edu/cs'},
          body: {
            kind: 'non_course',
            nonCourseKind: 'proficiency_exam',
            description: 'Pass the exam',
          },
        },
        {
          id: 'req-portfolio',
          label: 'Portfolio',
          source: {url: 'https://ga.rice.edu/cs'},
          body: {
            kind: 'non_course',
            nonCourseKind: 'portfolio',
            description: 'x',
          },
        },
        {
          id: 'req-other',
          label: 'Other',
          source: {url: 'https://ga.rice.edu/cs'},
          body: {kind: 'non_course', nonCourseKind: 'other', description: 'y'},
        },
        {
          id: 'req-unverifiable',
          label: 'Advisor sign-off',
          source: {url: 'https://ga.rice.edu/cs'},
          body: {kind: 'unverifiable', text: 'See your advisor'},
        },
        {
          id: 'req-depts',
          label: 'Breadth',
          source: {url: 'https://ga.rice.edu/cs'},
          body: {
            kind: 'distinct_departments',
            minimum: 3,
            text: 'Three departments',
          },
        },
      ],
    },
  },
  retiredRequirements: ['req-old'],
};

describe('programFromWire', () => {
  it('renames body kinds, selector kinds and non-course kinds, and drops review', () => {
    const program = programFromWire(wireProgram);
    expect(program).not.toHaveProperty('review');
    expect(program).not.toHaveProperty('totalCredits');
    expect(program.source).toEqual({url: 'https://ga.rice.edu/cs'});
    expect(program.retiredRequirements).toEqual(['req-old']);
    expect(program.root.hours).toEqual({
      kind: 'range',
      value: {min: 12000, max: 12600},
    });
    expect(program.root.body.kind).toBe('all');
    if (program.root.body.kind !== 'all') {
      throw new Error('expected all');
    }
    expect(program.root.body.of.map(r => r.body.kind)).toEqual([
      'course',
      'select',
      'nonCourse',
      'nonCourse',
      'nonCourse',
      'unverifiable',
      'distinctDepartments',
    ]);
    const core = program.root.body.of[0]?.body;
    if (core?.kind !== 'course') {
      throw new Error('expected course');
    }
    expect(core.filter.include).toEqual([
      {kind: 'code', code: {subject: 'COMP', number: '140'}},
      {kind: 'subject', subject: 'COMP'},
      {kind: 'numberRange', subject: 'COMP', low: 300, high: 499},
      {kind: 'numberRange', low: 500, high: 699},
      {kind: 'attribute', attribute: 'GRP3'},
    ]);
    expect(core.filter.include[3]).not.toHaveProperty('subject');
    const kinds = program.root.body.of
      .map(r => r.body)
      .flatMap(b => (b.kind === 'nonCourse' ? [b.nonCourseKind] : []));
    expect(kinds).toEqual(['proficiencyExam', 'portfolio', 'other']);
    const select = program.root.body.of[1]?.body;
    if (select?.kind !== 'select') {
      throw new Error('expected select');
    }
    expect(select.of[0]?.body).toEqual({
      kind: 'credits',
      minimum: 900,
      scope: 'additional',
      from: {include: [{kind: 'subject', subject: 'COMP'}], exclude: []},
    });
  });

  it('keeps totalCredits when the wire carries one', () => {
    expect(
      programFromWire({...wireProgram, totalCredits: 12000}).totalCredits,
    ).toBe(12000);
  });

  it('throws on unknown kinds', () => {
    expect(() => nonCourseKindFromWire('bogus' as unknown as 'other')).toThrow(
      /unknown wire value for nonCourseKind/,
    );
    expect(() =>
      courseSelectorFromWire({kind: 'bogus'} as unknown as {
        kind: 'subject';
        subject: string;
      }),
    ).toThrow(/unknown wire value for courseSelector/);
    expect(() =>
      requirementFromWire({
        ...wireProgram.root,
        body: {kind: 'bogus'} as unknown as {
          kind: 'unverifiable';
          text: string;
        },
      }),
    ).toThrow(/unknown wire value for requirementBody/);
  });
});

describe('programSummaryFromWire', () => {
  const summary: WireProgramSummary = {
    id: 'prog-cs',
    slug: 'computer-science-bs',
    kind: 'major',
    name: 'Computer Science',
    credential: 'BS',
    catalogYears: [2025, 2026],
    totalCredits: null,
  };

  it('turns a null totalCredits into an absent key', () => {
    const out = programSummaryFromWire(summary);
    expect(out).toEqual({
      id: 'prog-cs',
      slug: 'computer-science-bs',
      kind: 'major',
      name: 'Computer Science',
      credential: 'BS',
      catalogYears: [2025, 2026],
    });
    expect(out).not.toHaveProperty('totalCredits');
    expect(
      programSummaryFromWire({...summary, totalCredits: 12000}).totalCredits,
    ).toBe(12000);
  });
});
