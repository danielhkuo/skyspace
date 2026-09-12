/**
 * The "CS + Stats minor" plan from the design artboards, as a `PlanBundle`.
 * Codes, titles, prerequisites and program rules are real Rice data
 * (`rice-data.md`); the student and their choices are invented.
 *
 * Fixture ids are stable strings, not UUIDs, so tests can name them.
 */
import type {
  CourseCode,
  CourseFacts,
  CourseFilter,
  CourseInfo,
  CreditRange,
  EntryId,
  Exclusion,
  ManualCourseCard,
  Plan,
  PlanBundle,
  PlanId,
  PlanTerm,
  PlannedCourse,
  Prerequisite,
  PrereqExpr,
  Program,
  ProgramId,
  Rule,
  RuleId,
  Season,
  TermId,
  TermPosition,
} from '../domain';
import {creditsFromHours, parseCourseCode} from '../domain';

const code = (raw: string): CourseCode => {
  const parsed = parseCourseCode(raw);
  if (parsed === null) {
    throw new Error(`bad fixture course code ${raw}`);
  }
  return parsed;
};

const fixed = (hours: number): CreditRange => ({
  kind: 'fixed',
  value: creditsFromHours(hours),
});

const GA = 'https://ga.rice.edu/programs-study/departments-programs/';

// ---------------------------------------------------------------- programs

export const UNIVERSITY_ID = 'prog-university' as ProgramId;
export const BSCS_ID = 'prog-bscs' as ProgramId;
export const STATS_MINOR_ID = 'prog-stat-minor' as ProgramId;

export const RULES = {
  fwis: 'rule-fwis' as RuleId,
  dg1: 'rule-dg1' as RuleId,
  dg1a: 'rule-dg1-a' as RuleId,
  dg1b: 'rule-dg1-b' as RuleId,
  dg1c: 'rule-dg1-c' as RuleId,
  dg1Depts: 'rule-dg1-depts' as RuleId,
  dg2: 'rule-dg2' as RuleId,
  dg2a: 'rule-dg2-a' as RuleId,
  dg2b: 'rule-dg2-b' as RuleId,
  dg2c: 'rule-dg2-c' as RuleId,
  dg3: 'rule-dg3' as RuleId,
  dg3a: 'rule-dg3-a' as RuleId,
  dg3b: 'rule-dg3-b' as RuleId,
  dg3c: 'rule-dg3-c' as RuleId,
  ad: 'rule-ad' as RuleId,
  lpap: 'rule-lpap' as RuleId,
  core: 'rule-core' as RuleId,
  calc: 'rule-calc' as RuleId,
  math102: 'rule-math102' as RuleId,
  math212: 'rule-math212' as RuleId,
  comp140: 'rule-comp140' as RuleId,
  comp182: 'rule-comp182' as RuleId,
  comp215: 'rule-comp215' as RuleId,
  comp222: 'rule-comp222' as RuleId,
  comp312: 'rule-comp312' as RuleId,
  comp321: 'rule-comp321' as RuleId,
  comp382: 'rule-comp382' as RuleId,
  probStat: 'rule-probstat' as RuleId,
  design: 'rule-design' as RuleId,
  breadth: 'rule-breadth' as RuleId,
  breadthA: 'rule-breadth-a' as RuleId,
  breadthB: 'rule-breadth-b' as RuleId,
  breadthC: 'rule-breadth-c' as RuleId,
  breadthD: 'rule-breadth-d' as RuleId,
  electives: 'rule-electives' as RuleId,
  footnote2: 'rule-footnote-2' as RuleId,
  freeElectives: 'rule-free-electives' as RuleId,
  minorRoot: 'rule-minor-root' as RuleId,
  stat310: 'rule-minor-stat310' as RuleId,
  stat315: 'rule-minor-stat315' as RuleId,
  stat410: 'rule-minor-stat410' as RuleId,
  stat413: 'rule-minor-stat413' as RuleId,
  minorElective1: 'rule-minor-el-1' as RuleId,
  minorElective2: 'rule-minor-el-2' as RuleId,
} as const;

const universitySource = {
  url: 'https://ga.rice.edu/undergraduate-students/academic-policies-procedures/graduation-requirements/',
};

const courseRule = (
  id: RuleId,
  label: string,
  include: CourseFilter,
): Rule => ({
  id,
  label,
  source: universitySource,
  body: {kind: 'course', filter: include, semesters: 1},
});

const codeRule = (id: RuleId, raw: string, hours?: number): Rule => ({
  id,
  label: raw,
  source: {url: `${GA}engineering/computer-science/computer-science-bscs/`},
  hours: hours === undefined ? undefined : fixed(hours),
  body: {
    kind: 'course',
    filter: {include: [{kind: 'code', code: code(raw)}], exclude: []},
    semesters: 1,
  },
});

const distributionGroup = (
  id: RuleId,
  slots: [RuleId, RuleId, RuleId],
  attribute: 'GRP1' | 'GRP2' | 'GRP3',
  label: string,
): Rule => ({
  id,
  label,
  source: universitySource,
  body: {
    kind: 'all',
    of: slots.map(slotId =>
      courseRule(slotId, label, {
        include: [{kind: 'attribute', attribute}],
        exclude: [],
      }),
    ),
  },
});

export const universityProgram: Program = {
  id: UNIVERSITY_ID,
  catalogYear: 2026,
  slug: 'university',
  kind: 'university',
  name: 'University',
  credential: '',
  totalCredits: creditsFromHours(31),
  source: universitySource,
  retiredRules: [],
  root: {
    id: 'rule-university-root' as RuleId,
    label: 'University graduation requirements',
    source: universitySource,
    body: {
      kind: 'all',
      of: [
        courseRule(RULES.fwis, 'FWIS', {
          include: [
            {kind: 'numberRange', subject: 'FWIS', low: 101, high: 299},
          ],
          exclude: [],
        }),
        {
          id: RULES.dg1,
          label: 'Distribution Group I',
          source: universitySource,
          body: {
            kind: 'all',
            of: [
              ...(
                distributionGroup(
                  'rule-dg1-inner' as RuleId,
                  [RULES.dg1a, RULES.dg1b, RULES.dg1c],
                  'GRP1',
                  'Distribution Group I',
                ).body as {kind: 'all'; of: Rule[]}
              ).of,
              {
                id: RULES.dg1Depts,
                label: 'From at least two departments',
                source: universitySource,
                body: {
                  kind: 'unverifiable',
                  text: 'The 3 courses in each group must include courses in at least two departments in that group.',
                },
              },
            ],
          },
        },
        distributionGroup(
          RULES.dg2,
          [RULES.dg2a, RULES.dg2b, RULES.dg2c],
          'GRP2',
          'Distribution Group II',
        ),
        distributionGroup(
          RULES.dg3,
          [RULES.dg3a, RULES.dg3b, RULES.dg3c],
          'GRP3',
          'Distribution Group III',
        ),
        // A credits rule with scope `any`, not a slot: the AD course is the
        // same card that fills a distribution slot, and must count for both.
        {
          id: RULES.ad,
          label: 'Analyzing Diversity',
          source: universitySource,
          body: {
            kind: 'credits',
            minimum: creditsFromHours(1),
            scope: 'any',
            from: {
              include: [{kind: 'attribute', attribute: 'AD'}],
              exclude: [],
            },
          },
        },
        courseRule(RULES.lpap, 'LPAP', {
          include: [{kind: 'subject', subject: 'LPAP'}],
          exclude: [],
        }),
      ],
    },
  },
};

const bscsSource = {
  url: `${GA}engineering/computer-science/computer-science-bscs/`,
};

export const bscsProgram: Program = {
  id: BSCS_ID,
  catalogYear: 2026,
  slug: 'computer-science-bscs',
  kind: 'major',
  name: 'Computer Science, BSCS',
  credential: 'BSCS',
  totalCredits: creditsFromHours(120),
  source: bscsSource,
  retiredRules: [],
  root: {
    id: 'rule-bscs-root' as RuleId,
    label: 'Bachelor of Science in Computer Science (BSCS)',
    source: bscsSource,
    body: {
      kind: 'all',
      of: [
        {
          id: RULES.core,
          label: 'Core Requirements',
          source: bscsSource,
          body: {
            kind: 'all',
            of: [
              {
                id: RULES.calc,
                label: 'MATH 101 or MATH 105',
                source: bscsSource,
                hours: fixed(3),
                body: {
                  kind: 'course',
                  filter: {
                    include: [
                      {kind: 'code', code: code('MATH 101')},
                      {kind: 'code', code: code('MATH 105')},
                    ],
                    exclude: [],
                  },
                  semesters: 1,
                },
              },
              codeRule(RULES.math102, 'MATH 102', 3),
              codeRule(RULES.math212, 'MATH 212', 3),
              codeRule(RULES.comp140, 'COMP 140', 4),
              codeRule(RULES.comp182, 'COMP 182', 4),
              codeRule(RULES.comp215, 'COMP 215', 4),
              codeRule(RULES.comp222, 'COMP 222', 4),
              codeRule(RULES.comp312, 'COMP 312', 4),
              codeRule(RULES.comp321, 'COMP 321', 4),
              codeRule(RULES.comp382, 'COMP 382', 4),
              {
                id: RULES.probStat,
                label:
                  'Select 1 course from ELEC 303, STAT 310 / ECON 307, STAT 311, STAT 312, STAT 315 / DSCI 301',
                source: bscsSource,
                hours: {
                  kind: 'range',
                  value: {min: creditsFromHours(3), max: creditsFromHours(4)},
                },
                body: {
                  kind: 'course',
                  filter: {
                    include: [
                      'ELEC 303',
                      'STAT 310',
                      'STAT 311',
                      'STAT 312',
                      'STAT 315',
                    ].map(raw => ({kind: 'code', code: code(raw)}) as const),
                    exclude: [],
                  },
                  semesters: 1,
                },
              },
            ],
          },
        },
        {
          id: RULES.design,
          label: 'Design Requirement',
          source: bscsSource,
          hours: fixed(4),
          body: {
            kind: 'course',
            filter: {
              include: [
                {kind: 'code', code: code('COMP 410')},
                {kind: 'code', code: code('COMP 413')},
              ],
              exclude: [],
            },
            semesters: 1,
          },
        },
        {
          id: RULES.breadth,
          label: 'Breadth Requirements',
          source: bscsSource,
          body: {
            kind: 'all',
            of: [
              codeRule(RULES.breadthA, 'COMP 318', 4),
              codeRule(RULES.breadthB, 'COMP 421', 4),
              codeRule(RULES.breadthC, 'COMP 411', 4),
              codeRule(RULES.breadthD, 'COMP 412', 4),
            ],
          },
        },
        {
          id: RULES.electives,
          label: 'Select 2 courses from COMP at the 300 level or above',
          source: bscsSource,
          hours: fixed(3),
          body: {
            kind: 'course',
            filter: {
              include: [
                {kind: 'numberRange', subject: 'COMP', low: 300, high: 699},
              ],
              exclude: [
                'COMP 312',
                'COMP 318',
                'COMP 321',
                'COMP 382',
                'COMP 410',
                'COMP 411',
                'COMP 412',
                'COMP 413',
                'COMP 421',
              ].map(raw => ({kind: 'code', code: code(raw)}) as const),
            },
            semesters: 2,
          },
        },
        {
          id: RULES.freeElectives,
          label: 'Additional credit hours to complete degree requirements',
          source: bscsSource,
          body: {
            kind: 'credits',
            minimum: creditsFromHours(17),
            scope: 'additional',
            from: {include: [], exclude: []},
          },
        },
        {
          id: RULES.footnote2,
          label: 'Footnote 2',
          source: bscsSource,
          body: {
            kind: 'unverifiable',
            text: 'At most 1 elective may be a research or independent study project (COMP 364, 390, 464, 490, 491). Students may take courses at the 500-level. However, the only 600-level courses that may be used as electives are COMP 631 and COMP 646.',
          },
        },
      ],
    },
  },
};

const minorSource = {url: `${GA}engineering/statistics/statistics-minor/`};

export const statsMinorProgram: Program = {
  id: STATS_MINOR_ID,
  catalogYear: 2026,
  slug: 'statistics-minor',
  kind: 'minor',
  name: 'Statistics minor',
  credential: 'Minor',
  totalCredits: creditsFromHours(18),
  source: minorSource,
  retiredRules: [],
  root: {
    id: RULES.minorRoot,
    label: 'Minor in Statistics',
    source: minorSource,
    body: {
      kind: 'all',
      of: [
        {...codeRule(RULES.stat310, 'STAT 310', 3), source: minorSource},
        {...codeRule(RULES.stat315, 'STAT 315', 3), source: minorSource},
        {...codeRule(RULES.stat410, 'STAT 410', 3), source: minorSource},
        {...codeRule(RULES.stat413, 'STAT 413', 3), source: minorSource},
        {
          id: RULES.minorElective1,
          label: 'STAT elective at the 400 level or above',
          source: minorSource,
          hours: fixed(3),
          body: {
            kind: 'course',
            filter: {
              include: [
                {kind: 'numberRange', subject: 'STAT', low: 400, high: 599},
              ],
              exclude: [
                {kind: 'code', code: code('STAT 410')},
                {kind: 'code', code: code('STAT 413')},
              ],
            },
            semesters: 2,
          },
        },
      ],
    },
  },
};

// -------------------------------------------------------------------- facts

type InfoSpec = {
  raw: string;
  title: string;
  credits: number | CreditRange;
  attributes?: CourseInfo['attributes'];
  repeatable?: boolean;
  seasons?: Season[];
  offeredNow?: boolean;
};

const info = (spec: InfoSpec): CourseInfo => ({
  code: code(spec.raw),
  title: spec.title,
  credits:
    typeof spec.credits === 'number' ? fixed(spec.credits) : spec.credits,
  attributes: spec.attributes ?? [],
  repeatable: spec.repeatable ?? false,
  seasonsOffered: spec.seasons ?? ['fall', 'spring'],
  termsObserved: 1,
  offeredNow: spec.offeredNow ?? true,
});

export const courseFacts: CourseFacts = {
  courses: [
    info({raw: 'MATH 101', title: 'Single variable calculus I', credits: 3}),
    info({raw: 'MATH 102', title: 'Single variable calculus II', credits: 3}),
    info({raw: 'MATH 105', title: 'AP/OTH credit in calculus I', credits: 3}),
    info({raw: 'MATH 212', title: 'Multivariable calculus', credits: 3}),
    info({
      raw: 'COMP 140',
      title: 'Computational thinking',
      credits: 4,
      attributes: ['GRP3'],
    }),
    info({
      raw: 'COMP 182',
      title: 'Algorithmic thinking',
      credits: 4,
      attributes: ['GRP3'],
    }),
    info({
      raw: 'COMP 215',
      title: 'Introduction to program design',
      credits: 4,
    }),
    info({
      raw: 'COMP 222',
      title: 'Intro to computer organization',
      credits: 4,
    }),
    info({
      raw: 'COMP 310',
      title: 'Advanced object-oriented programming',
      credits: 4,
      offeredNow: false,
    }),
    info({raw: 'COMP 312', title: 'Reasoning about programs', credits: 4}),
    info({raw: 'COMP 318', title: 'Concurrent programming', credits: 4}),
    info({
      raw: 'COMP 321',
      title: 'Introduction to computer systems',
      credits: 4,
    }),
    info({raw: 'COMP 382', title: 'Reasoning about algorithms', credits: 4}),
    info({raw: 'COMP 410', title: 'Software engineering', credits: 4}),
    info({
      raw: 'COMP 411',
      title: 'Principles of programming languages',
      credits: 4,
    }),
    info({raw: 'COMP 412', title: 'Compiler construction', credits: 4}),
    info({
      raw: 'COMP 413',
      title: 'Software design and engineering',
      credits: 4,
      offeredNow: false,
    }),
    info({
      raw: 'COMP 415',
      title: 'Distributed systems',
      credits: 4,
      offeredNow: false,
    }),
    info({
      raw: 'COMP 421',
      title: 'Operating systems and concurrent programming',
      credits: 4,
    }),
    info({
      raw: 'COMP 430',
      title: 'Introduction to database systems',
      credits: 4,
    }),
    info({raw: 'COMP 440', title: 'Artificial intelligence', credits: 3}),
    info({
      raw: 'COMP 490',
      title: 'Computer science projects',
      credits: {kind: 'range', value: {min: 100, max: 400}},
      repeatable: true,
    }),
    info({
      raw: 'STAT 310',
      title: 'Probability and statistics',
      credits: 3,
      attributes: ['GRP3'],
    }),
    info({
      raw: 'ECON 307',
      title: 'Probability and statistics',
      credits: 3,
      attributes: ['GRP3'],
    }),
    info({raw: 'STAT 315', title: 'Statistical inference', credits: 3}),
    info({raw: 'STAT 410', title: 'Linear regression', credits: 3}),
    info({raw: 'STAT 413', title: 'Generalized linear models', credits: 3}),
    info({raw: 'STAT 421', title: 'Applied time series', credits: 3}),
    info({
      raw: 'ELEC 303',
      title: 'Random signals in electrical engineering systems',
      credits: 3,
    }),
    info({raw: 'FWIS 149', title: 'What is college for', credits: 3}),
    info({
      raw: 'CHEM 121',
      title: 'General chemistry I',
      credits: 3,
      attributes: ['GRP3'],
    }),
    info({
      raw: 'PHYS 101',
      title: 'Mechanics',
      credits: 3,
      attributes: ['GRP3'],
    }),
    info({raw: 'LPAP 170', title: 'Yoga', credits: 1, repeatable: true}),
    info({
      raw: 'HIST 117',
      title: 'The world since 1492',
      credits: 3,
      attributes: ['GRP1', 'AD'],
    }),
    info({
      raw: 'HIST 246',
      title: 'Modern Latin America',
      credits: 3,
      attributes: ['GRP2', 'AD'],
      offeredNow: false,
    }),
    info({
      raw: 'ENGL 200',
      title: 'Introduction to literary study',
      credits: 3,
      attributes: ['GRP1'],
    }),
    info({
      raw: 'MUSI 117',
      title: 'Fundamentals of music',
      credits: 3,
      attributes: ['GRP2'],
    }),
    info({
      raw: 'PHIL 101',
      title: 'Introduction to philosophy',
      credits: 3,
      attributes: ['GRP1'],
    }),
    info({
      raw: 'HART 101',
      title: 'Introduction to art history',
      credits: 3,
      attributes: ['GRP1'],
    }),
    info({
      raw: 'ANTH 201',
      title: 'Introduction to cultural anthropology',
      credits: 3,
      attributes: ['GRP2', 'AD'],
    }),
    info({
      raw: 'SOCI 231',
      title: 'Race and ethnic relations',
      credits: 3,
      attributes: ['GRP2', 'AD'],
    }),
    info({
      raw: 'ECON 100',
      title: 'Principles of economics',
      credits: 3,
      attributes: ['GRP2'],
    }),
  ],
  aliases: [[code('ECON 307'), code('STAT 310')]],
};

const c = (raw: string): PrereqExpr => ({kind: 'course', value: code(raw)});
const all = (...xs: PrereqExpr[]): PrereqExpr => ({kind: 'all', value: xs});
const any = (...xs: PrereqExpr[]): PrereqExpr => ({kind: 'any', value: xs});

const requires = (
  raw: string,
  published: string,
  expr: PrereqExpr,
): Prerequisite => ({
  course: code(raw),
  fact: {kind: 'requires', value: {publishedFor: 2026, expr, published}},
});

export const prerequisites: Prerequisite[] = [
  requires('COMP 182', 'COMP 140', c('COMP 140')),
  requires('COMP 215', 'COMP 182', c('COMP 182')),
  requires('COMP 222', 'COMP 140', c('COMP 140')),
  requires('COMP 312', 'COMP 215', c('COMP 215')),
  requires('COMP 318', 'COMP 215', c('COMP 215')),
  requires(
    'COMP 321',
    'COMP 215 AND COMP 222',
    all(c('COMP 215'), c('COMP 222')),
  ),
  requires(
    'COMP 382',
    'COMP 182 AND COMP 215 AND (ELEC 303 OR STAT 310 OR ECON 307 OR STAT 311 OR STAT 312 OR STAT 315 OR DSCI 301)',
    all(
      c('COMP 182'),
      c('COMP 215'),
      any(
        c('ELEC 303'),
        c('STAT 310'),
        c('ECON 307'),
        c('STAT 311'),
        c('STAT 312'),
        c('STAT 315'),
        c('DSCI 301'),
      ),
    ),
  ),
  requires('COMP 415', 'COMP 382', c('COMP 382')),
  requires('COMP 421', 'COMP 321', c('COMP 321')),
  requires('COMP 430', 'COMP 215', c('COMP 215')),
  requires('COMP 440', 'COMP 182', c('COMP 182')),
  requires('STAT 315', 'STAT 310', c('STAT 310')),
  requires('STAT 410', 'STAT 310', c('STAT 310')),
  requires('STAT 413', 'STAT 410', c('STAT 410')),
  {
    course: code('COMP 140'),
    fact: {kind: 'noneRequired', value: {publishedFor: 2026}},
  },
  {
    course: code('STAT 310'),
    fact: {
      kind: 'requires',
      value: {publishedFor: 2026, expr: c('MATH 212'), published: 'MATH 212'},
    },
  },
];

export const exclusions: Exclusion[] = [
  {
    blocked: code('COMP 318'),
    blocker: code('COMP 310'),
    publishedFor: 2026,
    published:
      'Cannot register for COMP 318 if student has credit for COMP 310.',
  },
];

// --------------------------------------------------------------------- plan

export const PLAN_ID = 'plan-cs-stats' as PlanId;

export const TERMS = {
  fall24: 'term-fall-2024' as TermId,
  spring25: 'term-spring-2025' as TermId,
  fall25: 'term-fall-2025' as TermId,
  spring26: 'term-spring-2026' as TermId,
  fall26: 'term-fall-2026' as TermId,
  spring27: 'term-spring-2027' as TermId,
  fall27: 'term-fall-2027' as TermId,
  spring28: 'term-spring-2028' as TermId,
} as const;

let entrySeq = 0;
const entry = (raw: string): EntryId => {
  entrySeq += 1;
  return `entry-${raw.replace(' ', '-').toLowerCase()}-${entrySeq}` as EntryId;
};

const planned = (
  raw: string,
  hours: number,
  fills: RuleId[] = [],
): PlannedCourse => ({
  id: entry(raw),
  course: code(raw),
  credits: creditsFromHours(hours),
  fills,
});

const rice = (
  id: TermId,
  position: TermPosition,
  courses: PlannedCourse[],
  termCode?: string,
): PlanTerm => ({
  id,
  position,
  kind: {rice: {code: termCode, courses}},
  nonCourse: [],
});

export const COMP_415_ENTRY = entry('COMP 415');
export const TRAN_100_ENTRY = entry('TRAN 100');

const incoming: ManualCourseCard[] = [
  {
    id: entry('MATH 105'),
    origin: 'advancedPlacement',
    code: 'MATH 105',
    title: 'AP/OTH Credit in Calculus I',
    credits: creditsFromHours(3),
    riceEquivalent: code('MATH 105'),
    fills: [],
  },
  {
    id: TRAN_100_ENTRY,
    origin: 'transfer',
    code: 'TRAN 100',
    title: 'Intro Psychology, Houston CC',
    credits: creditsFromHours(3),
    institution: 'Houston Community College',
    fills: [RULES.dg1b],
    // No Rice code, so its pin is a claim, not a match; the student recorded
    // why. The sidebar shows it "on your say-so" and never counts it as met.
    claims: [
      {
        rule: RULES.dg1b,
        basis: {kind: 'registrarPosted'},
        note: 'Posted on my transfer evaluation',
      },
    ],
  },
];

export const plan: Plan = {
  id: PLAN_ID,
  name: 'CS + Stats minor',
  catalogYear: 2026,
  matriculation: {academicYear: 2025, season: 'fall'},
  programs: [UNIVERSITY_ID, BSCS_ID, STATS_MINOR_ID],
  incomingCredit: incoming,
  selfChecks: [{rule: RULES.dg1Depts, reason: 'other', note: 'HIST and PSYC'}],
  terms: [
    rice(
      TERMS.fall24,
      {academicYear: 2025, season: 'fall'},
      [
        planned('COMP 140', 4),
        planned('MATH 102', 3),
        planned('FWIS 149', 3),
        planned('CHEM 121', 3),
        planned('LPAP 170', 1),
      ],
      '202510',
    ),
    rice(
      TERMS.spring25,
      {academicYear: 2025, season: 'spring'},
      [
        planned('COMP 182', 4),
        planned('MATH 212', 3),
        planned('PHYS 101', 3),
        planned('HIST 117', 3),
      ],
      '202520',
    ),
    rice(
      TERMS.fall25,
      {academicYear: 2026, season: 'fall'},
      [
        planned('COMP 215', 4),
        planned('COMP 222', 4),
        planned('STAT 310', 3),
        planned('ENGL 200', 3),
      ],
      '202610',
    ),
    rice(
      TERMS.spring26,
      {academicYear: 2026, season: 'spring'},
      [
        planned('COMP 312', 4),
        planned('COMP 321', 4),
        planned('STAT 315', 3),
        planned('MUSI 117', 3),
      ],
      '202620',
    ),
    rice(
      TERMS.fall26,
      {academicYear: 2027, season: 'fall'},
      [
        planned('COMP 382', 4),
        planned('COMP 318', 4),
        planned('STAT 410', 3),
        planned('PHIL 101', 3),
        {
          id: COMP_415_ENTRY,
          course: code('COMP 415'),
          credits: creditsFromHours(4),
          fills: [],
        },
      ],
      '202710',
    ),
    rice(TERMS.spring27, {academicYear: 2027, season: 'spring'}, [
      planned('COMP 421', 4),
      planned('STAT 413', 3),
      planned('COMP 430', 4),
    ]),
    {
      id: TERMS.fall27,
      position: {academicYear: 2028, season: 'fall'},
      label: 'Study abroad — Madrid',
      kind: {
        away: {
          cards: [
            {
              id: entry('Distributed Systems'),
              origin: 'studyAbroad',
              code: 'Distributed Systems',
              title: 'Universidad Politécnica',
              credits: creditsFromHours(4),
              institution: 'Universidad Politécnica de Madrid',
              fills: [RULES.electives],
            },
            {
              id: entry('Spanish Literature'),
              origin: 'studyAbroad',
              code: 'Spanish Literature',
              title: 'Universidad Politécnica',
              credits: creditsFromHours(3),
              institution: 'Universidad Politécnica de Madrid',
              fills: [],
            },
          ],
        },
      },
      nonCourse: [],
    },
    rice(TERMS.spring28, {academicYear: 2028, season: 'spring'}, []),
  ],
};

export const bundle: PlanBundle = {
  plan,
  programs: [universityProgram, bscsProgram, statsMinorProgram],
  facts: courseFacts,
  prerequisites,
  exclusions,
  limits: {
    fallSpring: creditsFromHours(18),
    musicAndArchitecture: creditsFromHours(20),
  },
  today: {academicYear: 2027, season: 'fall'},
};

// -------------------------------------------------------------------- saved

/** Starred courses. One flat list; there are no folders. */
export const favorites: CourseCode[] = [
  'STAT 315',
  'COMP 430',
  'COMP 440',
  'HART 101',
  'PHIL 101',
  'ECON 100',
].map(code);

/** Stand-in for the catalog query a rule's suggestion strip makes (`08-board-interaction.md`). */
export const catalogCandidates: CourseCode[] = courseFacts.courses.map(
  i => i.code,
);
