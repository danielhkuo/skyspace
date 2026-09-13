import {describe, expect, it} from 'vitest';

import type {Plan as WirePlan} from '../generated/Plan';
import type {PlanBundle as WirePlanBundle} from '../generated/PlanBundle';
import type {Program as WireProgram} from '../generated/Program';
import {
  bundleFromWire,
  creditOriginFromWire,
  creditOriginToWire,
  fillBasisFromWire,
  planFromWire,
  planToWire,
  prereqFactFromWire,
  selfCheckReasonFromWire,
  timestampFromWire,
  timestampToWire,
} from './plan';

/** Every `FillBasis`, `CreditOrigin` and `SelfCheckReason` kind, with and without optionals. */
const wirePlan: WirePlan = {
  id: 'plan-1',
  name: 'CS + Stats',
  catalogYear: 2026,
  matriculation: {academicYear: 2027, season: 'fall'},
  programs: ['prog-cs', 'prog-stat'],
  incomingCredit: [
    {
      id: 'card-ap',
      origin: 'advanced_placement',
      code: 'AP CSA',
      title: 'Computer Science A',
      credits: 300,
      riceEquivalent: {subject: 'COMP', number: '140'},
      fills: ['req-intro'],
      claims: [
        {
          requirement: 'req-intro',
          basis: {kind: 'registrar_posted', on: '2026-06-01'},
          note: 'Posted on transcript',
        },
      ],
    },
    {
      id: 'card-ib',
      origin: 'international_baccalaureate',
      code: 'IB Math HL',
      title: 'Mathematics HL',
      credits: 400,
      creditsSource: 'manual',
      fills: [],
    },
    {
      id: 'card-transfer',
      origin: 'transfer',
      code: 'MATH 2413',
      title: 'Calculus I',
      credits: 400,
      institution: 'Houston Community College',
      fills: ['req-calc'],
      claims: [
        {
          requirement: 'req-calc',
          basis: {kind: 'earlier_catalog', catalogYear: 2024},
        },
        {requirement: 'req-calc', basis: {kind: 'earlier_catalog'}},
      ],
    },
    {
      id: 'card-other',
      origin: 'other',
      code: 'X',
      title: 'Something',
      credits: 0,
      fills: [],
      note: 'unclear',
    },
  ],
  terms: [
    {
      id: 'term-1',
      position: {academicYear: 2027, season: 'fall'},
      label: 'First semester',
      kind: {
        rice: {
          code: '202710',
          courses: [
            {
              id: 'entry-1',
              course: {subject: 'COMP', number: '182'},
              credits: 400,
              fills: ['req-algo'],
              claims: [
                {
                  requirement: 'req-algo',
                  basis: {
                    kind: 'advisor_approved',
                    who: 'Dr. X',
                    on: '2026-09-01',
                  },
                },
                {requirement: 'req-algo', basis: {kind: 'advisor_approved'}},
                {
                  requirement: 'req-algo',
                  basis: {kind: 'petition_granted', on: '2026-09-02'},
                },
                {requirement: 'req-algo', basis: {kind: 'petition_granted'}},
                {requirement: 'req-algo', basis: {kind: 'registrar_posted'}},
                {requirement: 'req-algo', basis: {kind: 'unsure'}},
              ],
              observed: {
                at: 1_757_600_000,
                catalogYear: 2026,
                title: 'Algorithmic Thinking',
                credits: {kind: 'fixed', value: 400},
                attributes: ['GRP3'],
              },
              note: 'Fun',
            },
            {
              id: 'entry-2',
              course: {subject: 'MATH', number: '212'},
              credits: 300,
              fills: [],
              carried: {
                origin: 'study_abroad',
                code: 'MATH 2',
                title: 'Multivariable',
                institution: 'Oxford',
              },
            },
            {
              id: 'entry-3',
              course: {subject: 'STAT', number: '310'},
              credits: 300,
              fills: [],
              carried: {origin: 'transfer', code: 'STAT', title: 'Stats'},
            },
          ],
        },
      },
      nonCourse: [{requirement: 'req-lpap', label: 'LPAP 100'}],
    },
    {
      id: 'term-2',
      position: {academicYear: 2027, season: 'spring'},
      kind: {rice: {courses: []}},
      nonCourse: [],
    },
    {
      id: 'term-3',
      position: {academicYear: 2027, season: 'summer'},
      label: 'Study abroad',
      kind: {
        away: {
          cards: [
            {
              id: 'card-away',
              origin: 'study_abroad',
              code: 'HIST 1',
              title: 'History',
              credits: 300,
              fills: [],
            },
          ],
        },
      },
      nonCourse: [],
    },
    {
      id: 'term-4',
      position: {academicYear: 2028, season: 'fall'},
      kind: 'off',
      nonCourse: [],
    },
  ],
  selfChecks: [
    {requirement: 'req-a', reason: 'transfer', note: 'n'},
    {requirement: 'req-b', reason: 'ap_or_ib'},
    {requirement: 'req-c', reason: 'study_abroad'},
    {requirement: 'req-d', reason: 'advisor_approved'},
    {requirement: 'req-e', reason: 'other'},
  ],
};

describe('plan round trip', () => {
  it('planToWire(planFromWire(w)) reproduces the wire document key for key', () => {
    expect(planToWire(planFromWire(wirePlan))).toStrictEqual(wirePlan);
  });

  it('renames kinds inbound', () => {
    const plan = planFromWire(wirePlan);
    expect(plan.incomingCredit.map(c => c.origin)).toEqual([
      'advancedPlacement',
      'internationalBaccalaureate',
      'transfer',
      'other',
    ]);
    expect(plan.selfChecks.map(s => s.reason)).toEqual([
      'transfer',
      'apOrIb',
      'studyAbroad',
      'advisorApproved',
      'other',
    ]);
    const first = plan.terms[0]?.kind;
    if (first === undefined || first === 'off' || !('rice' in first)) {
      throw new Error('expected a rice term');
    }
    expect(first.rice.courses[0]?.claims?.map(c => c.basis.kind)).toEqual([
      'advisorApproved',
      'advisorApproved',
      'petitionGranted',
      'petitionGranted',
      'registrarPosted',
      'unsure',
    ]);
    expect(first.rice.courses[0]?.observed?.at).toBe(
      '2025-09-11T14:13:20.000Z',
    );
    expect(first.rice.courses[1]?.carried?.origin).toBe('studyAbroad');
  });

  it('never writes an undefined key', () => {
    const plan = planFromWire(wirePlan);
    expect(plan.terms[1]).not.toHaveProperty('label');
    expect(plan.incomingCredit[1]).not.toHaveProperty('institution');
    expect(plan.selfChecks[1]).not.toHaveProperty('note');
  });
});

describe('kind tables', () => {
  it('invert each other', () => {
    for (const w of [
      'transfer',
      'advanced_placement',
      'international_baccalaureate',
      'study_abroad',
      'other',
    ] as const) {
      expect(creditOriginToWire(creditOriginFromWire(w))).toBe(w);
    }
  });

  it('throw for a wire value the table does not know', () => {
    expect(() =>
      creditOriginFromWire('bogus' as unknown as 'transfer'),
    ).toThrow(/unknown wire value for creditOrigin/);
    expect(() =>
      selfCheckReasonFromWire('bogus' as unknown as 'other'),
    ).toThrow(/unknown wire value/);
  });

  it('fillBasisFromWire keeps the inner fields', () => {
    expect(
      fillBasisFromWire({kind: 'advisor_approved', who: 'X', on: null}),
    ).toEqual({kind: 'advisorApproved', who: 'X'});
  });
});

describe('timestamps', () => {
  it('round-trip whole seconds', () => {
    expect(timestampToWire(timestampFromWire(1_757_600_000))).toBe(
      1_757_600_000,
    );
    expect(timestampToWire('2026-09-11T19:09:12.500-05:00')).toBe(
      Date.UTC(2026, 8, 12, 0, 9, 12) / 1000,
    );
  });

  it('reject an unparsable stamp', () => {
    expect(() => timestampToWire('yesterday')).toThrow(/unknown wire value/);
  });
});

describe('bundleFromWire', () => {
  const wireProgram: WireProgram = {
    id: 'prog-cs',
    catalogYear: 2026,
    slug: 'computer-science-bs',
    kind: 'major',
    name: 'Computer Science',
    credential: 'BS',
    totalCredits: 12000,
    source: {url: 'https://ga.rice.edu/cs', anchor: 'requirements'},
    review: {reviewedBy: 'someone', publishedAt: 1_757_600_000},
    root: {
      id: 'req-root',
      label: 'Degree requirements',
      source: {url: 'https://ga.rice.edu/cs'},
      body: {kind: 'all', of: []},
    },
    retiredRequirements: [],
  };

  const wireBundle: WirePlanBundle = {
    plan: wirePlan,
    programs: [wireProgram],
    facts: {
      courses: [
        {
          code: {subject: 'COMP', number: '182'},
          title: 'Algorithmic Thinking',
          credits: {kind: 'fixed', value: 400},
          attributes: [],
          department: null,
          repeatable: false,
          seasonsOffered: ['fall', 'spring'],
          termsObserved: 6,
          offeredNow: true,
        },
      ],
      aliases: [
        [
          {subject: 'ELEC', number: '220'},
          {subject: 'COMP', number: '222'},
        ],
      ],
    },
    prerequisites: [
      {course: {subject: 'COMP', number: '140'}, fact: {kind: 'unknown'}},
      {
        course: {subject: 'COMP', number: '182'},
        fact: {kind: 'none_required', value: {publishedFor: 2026}},
      },
      {
        course: {subject: 'COMP', number: '215'},
        fact: {
          kind: 'requires',
          value: {
            publishedFor: 2026,
            expr: {
              kind: 'all',
              value: [
                {kind: 'course', value: {subject: 'COMP', number: '182'}},
                {kind: 'unparsed', value: 'or equivalent'},
              ],
            },
            published: 'COMP 182 or equivalent',
            corequisite: {subject: 'COMP', number: '140'},
          },
        },
      },
    ],
    exclusions: [
      {
        blocked: {subject: 'COMP', number: '140'},
        blocker: {subject: 'COMP', number: '182'},
        publishedFor: 2026,
        published: 'Cannot register',
      },
    ],
    limits: {fallSpring: 1700, musicAndArchitecture: 2000, summer: null},
    invalidations: [
      {
        kind: 'course_not_offered',
        value: {course: {subject: 'COMP', number: '140'}, lastSeen: '202610'},
      },
    ],
    today: {academicYear: 2027, season: 'fall'},
  };

  it('drops invalidations and review and renames prerequisite facts', () => {
    const bundle = bundleFromWire(wireBundle);
    expect(bundle).not.toHaveProperty('invalidations');
    expect(bundle.programs[0]).not.toHaveProperty('review');
    expect(bundle.programs[0]?.id).toBe('prog-cs');
    expect(bundle.prerequisites.map(p => p.fact.kind)).toEqual([
      'unknown',
      'noneRequired',
      'requires',
    ]);
    expect(bundle.prerequisites[2]?.fact).toEqual({
      kind: 'requires',
      value: {
        publishedFor: 2026,
        expr:
          wireBundle.prerequisites[2]?.fact.kind === 'requires'
            ? wireBundle.prerequisites[2].fact.value.expr
            : undefined,
        published: 'COMP 182 or equivalent',
        corequisite: {subject: 'COMP', number: '140'},
      },
    });
    expect(bundle.facts.courses[0]).not.toHaveProperty('department');
    expect(bundle.facts.courses[0]?.seasonsOffered).toEqual(['fall', 'spring']);
    expect(bundle.facts.aliases).toEqual(wireBundle.facts.aliases);
    expect(bundle.exclusions).toEqual(wireBundle.exclusions);
    expect(bundle.limits).toEqual({
      fallSpring: 1700,
      musicAndArchitecture: 2000,
    });
    expect(bundle.today).toEqual(wireBundle.today);
    expect(planToWire(bundle.plan)).toStrictEqual(wirePlan);
  });

  it('prereqFactFromWire throws on an unknown kind', () => {
    expect(() =>
      prereqFactFromWire({kind: 'bogus'} as unknown as {kind: 'unknown'}),
    ).toThrow();
  });
});
