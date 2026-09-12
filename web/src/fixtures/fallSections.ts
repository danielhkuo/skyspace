/**
 * Fall 2026 (`202710`) sections for the courses `csStats.ts` knows, plus a
 * few MUSI rows so the "hide unscheduled" rule has something to hide. Mock
 * data for the demo and tests: shapes and label formats follow
 * `rice-data.md` §2; CRNs, instructors, times and seat counts are invented.
 * An invented CRN must never be linked to courses.rice.edu: Rice reuses
 * CRNs across terms and 12950 is a real, unrelated section. The UI checks
 * `dataSource.kind` before it offers a Rice link.
 */
import {
  creditsFromHours,
  parseCourseCode,
  type Attribute,
  type CourseCode,
  type CreditRange,
  type Day,
  type FinalExam,
  type Meeting,
  type Section,
  type SectionDetail,
  type Seats,
  type TermCode,
} from '../domain';

export const FALL_2026: TermCode = '202710';
export const FALL_2026_LABEL = 'Fall Semester 2026';

const AS_OF = '2026-09-11T19:09:12-05:00';
const FULL_TERM = {
  start: {year: 2026, month: 8, day: 24},
  end: {year: 2026, month: 12, day: 4},
};

const code = (raw: string): CourseCode => {
  const parsed = parseCourseCode(raw);
  if (parsed === null) {
    throw new Error(`bad fixture course code ${raw}`);
  }
  return parsed;
};

const minute = (h: number, m: number): number => h * 60 + m;

/** `meet('MWF', 13, 0, 13, 50)`; 24-hour input. */
const meet = (
  days: string,
  h1: number,
  m1: number,
  h2: number,
  m2: number,
): Meeting => ({
  pattern: {
    kind: 'timed',
    value: {
      days: days.split('') as Day[],
      start: minute(h1, m1),
      end: minute(h2, m2),
    },
  },
  dates: FULL_TERM,
});

const seats = (
  enrolled: number,
  capacity: number,
  waitlistCount = 0,
  waitlistCapacity = 0,
): Seats => ({
  enrolled,
  capacity,
  waitlistCount,
  waitlistCapacity,
  asOf: AS_OF,
});

type Spec = {
  crn: string;
  raw: string;
  section?: string;
  title: string;
  credits?: number | CreditRange;
  instructors?: string[];
  meetings?: Meeting[];
  finalExam?: FinalExam;
  partOfTerm?: string;
  seats?: Seats;
  detail?: Partial<SectionDetail> & {description: string};
  attributes?: Attribute[];
};

const section = (spec: Spec): Section => {
  const credits: CreditRange =
    typeof spec.credits === 'object'
      ? spec.credits
      : {kind: 'fixed', value: creditsFromHours(spec.credits ?? 3)};
  const c = code(spec.raw);
  return {
    listing: {
      crn: spec.crn,
      term: FALL_2026,
      code: c,
      section: spec.section ?? '001',
      title: spec.title,
      credits,
      partOfTerm: spec.partOfTerm ?? 'Full Term',
      instructors: (spec.instructors ?? []).map(name => ({
        name,
        netId: name.split(',')[0]?.toLowerCase().slice(0, 4),
      })),
      meetings: spec.meetings ?? [],
      finalExam: spec.finalExam ?? 'scheduled',
    },
    detail:
      spec.detail === undefined
        ? undefined
        : {
            longTitle: spec.title,
            department: departmentOf(c.subject),
            attributes: spec.attributes ?? [],
            gradeMode: 'Standard Letter',
            methodOfInstruction: 'Face to Face',
            courseType: 'Lecture',
            language: 'English',
            reserved: [],
            notes: [],
            hasSyllabus: false,
            ...spec.detail,
          },
    seats: spec.seats,
  };
};

function departmentOf(subject: string): string {
  const names: Record<string, string> = {
    COMP: 'Computer Science',
    MATH: 'Mathematics',
    STAT: 'Statistics',
    ELEC: 'Electrical & Computer Eng',
    ECON: 'Economics',
    FWIS: 'First Year Writing Intensive',
    CHEM: 'Chemistry',
    PHYS: 'Physics & Astronomy',
    LPAP: 'Lifetime Physical Activity',
    HIST: 'History',
    ENGL: 'English',
    MUSI: 'Music',
    PHIL: 'Philosophy',
    HART: 'Art History',
    ANTH: 'Anthropology',
    SOCI: 'Sociology',
  };
  return names[subject] ?? subject;
}

const UNDERGRAD = 'Enrollment is limited to Undergraduate level students.';

export const fallSections: Section[] = [
  section({
    crn: '12422',
    raw: 'COMP 140',
    title: 'Computational thinking',
    credits: 4,
    instructors: ['Warren, Joe'],
    meetings: [meet('TR', 14, 30, 15, 45)],
    seats: seats(67, 72),
    attributes: ['GRP3'],
    detail: {
      courseType: 'Lecture/Laboratory',
      description:
        'Fundamental concepts of computer science and programming, using Python. Problem solving by decomposition, abstraction and the design of algorithms, with laboratory work applying those ideas to data from the sciences and social sciences.',
      restrictions: `Enrollment limited to students with a classification of Freshman or Sophomore. ${UNDERGRAD}`,
      reserved: [
        {label: 'Fall Semester 2026 Matriculants', capacity: 60, available: 2},
      ],
      hasSyllabus: true,
    },
  }),
  section({
    crn: '12423',
    raw: 'COMP 140',
    section: '002',
    title: 'Computational thinking',
    credits: 4,
    instructors: ['Warren, Joe'],
    meetings: [meet('TR', 16, 0, 17, 15)],
    seats: seats(72, 72),
    attributes: ['GRP3'],
    detail: {
      courseType: 'Lecture/Laboratory',
      description:
        'Fundamental concepts of computer science and programming, using Python. Problem solving by decomposition, abstraction and the design of algorithms, with laboratory work applying those ideas to data from the sciences and social sciences.',
      restrictions: `Enrollment limited to students with a classification of Freshman or Sophomore. ${UNDERGRAD}`,
      hasSyllabus: true,
    },
  }),
  section({
    crn: '12430',
    raw: 'COMP 182',
    title: 'Algorithmic thinking',
    credits: 4,
    instructors: ['Kavraki, Lydia'],
    meetings: [meet('MWF', 13, 0, 13, 50)],
    seats: seats(148, 160),
    attributes: ['GRP3'],
    detail: {
      description:
        'Introduction to the mathematical foundations of computer science: sets, relations, functions, induction, recursion, counting, graphs and trees, with algorithms that use them.',
      prerequisitesText: 'COMP 140',
      restrictions: UNDERGRAD,
    },
  }),
  section({
    crn: '12441',
    raw: 'COMP 215',
    title: 'Introduction to program design',
    credits: 4,
    instructors: ['Wong, Stephen'],
    meetings: [meet('MWF', 10, 0, 10, 50), meet('R', 16, 0, 17, 15)],
    seats: seats(120, 120, 6, 20),
    detail: {
      courseType: 'Lecture/Laboratory',
      description:
        'Object-oriented design and programming in Java: abstraction, encapsulation, design patterns, testing, and the design of larger programs.',
      prerequisitesText: 'COMP 182',
      restrictions: UNDERGRAD,
    },
  }),
  section({
    crn: '12455',
    raw: 'COMP 222',
    title: 'Intro to computer organization',
    credits: 4,
    instructors: ['Rixner, Scott'],
    meetings: [meet('MWF', 15, 0, 15, 50), meet('R', 16, 0, 17, 15)],
    seats: seats(94, 96),
    detail: {
      description:
        'How computers execute programs: data representation, machine language, the memory hierarchy, and an introduction to systems programming in C.',
      prerequisitesText: 'COMP 140',
      restrictions: UNDERGRAD,
    },
  }),
  section({
    crn: '12460',
    raw: 'COMP 312',
    title: 'Software engineering methodology',
    credits: 4,
    instructors: ['Wong, Stephen'],
    meetings: [meet('TR', 13, 0, 14, 15)],
    seats: seats(31, 40),
    detail: {
      description:
        'Design and implementation of software systems in teams: requirements, architecture, design patterns, testing, and delivery.',
      prerequisitesText: 'COMP 215',
    },
  }),
  section({
    crn: '12462',
    raw: 'COMP 318',
    title: 'Concurrent programming in Java',
    credits: 4,
    instructors: ['Sarkar, Vivek'],
    meetings: [meet('MW', 16, 0, 17, 15)],
    seats: seats(52, 60),
    detail: {
      description:
        'Foundations of concurrent and parallel programming: tasks, threads, synchronization, data races, and structured parallelism in Java.',
      prerequisitesText: 'COMP 215',
      mutuallyExclusive:
        'Cannot register for COMP 318 if student has credit for COMP 310.',
    },
  }),
  section({
    crn: '12468',
    raw: 'COMP 321',
    title: 'Introduction to computer systems',
    credits: 4,
    instructors: ['Cox, Alan'],
    meetings: [meet('TR', 10, 50, 12, 5)],
    seats: seats(88, 90),
    detail: {
      description:
        'Systems programming in C: processes, memory, linking, signals, and networking, with an emphasis on what the hardware and operating system provide.',
      prerequisitesText: 'COMP 215 AND COMP 222',
    },
  }),
  section({
    crn: '12312',
    raw: 'COMP 382',
    title: 'Reasoning about algorithms',
    credits: 4,
    instructors: ['Nakhleh, Luay'],
    meetings: [meet('TR', 10, 50, 12, 5), meet('R', 17, 30, 18, 45)],
    seats: seats(113, 120),
    detail: {
      description:
        'Design and analysis of algorithms: divide and conquer, dynamic programming, greedy algorithms, graph algorithms, and NP-completeness.',
      prerequisitesText:
        'COMP 182 AND COMP 215 AND (ELEC 303 OR STAT 310 OR ECON 307 OR STAT 311 OR STAT 312 OR STAT 315 OR DSCI 301)',
    },
  }),
  section({
    crn: '12475',
    raw: 'COMP 410',
    title: 'Software engineering project',
    credits: 4,
    instructors: ['Wong, Stephen'],
    meetings: [meet('MW', 14, 30, 15, 45)],
    seats: seats(24, 30),
    detail: {
      description:
        'A semester-long team project for an external client, from requirements through delivery.',
      prerequisitesText: 'COMP 312',
      notes: ['Instructor Permission Required.'],
    },
  }),
  section({
    crn: '12481',
    raw: 'COMP 421',
    title: 'Operating systems and concurrent programming',
    credits: 4,
    instructors: ['Cox, Alan'],
    meetings: [meet('MWF', 11, 0, 11, 50)],
    seats: seats(60, 60, 12, 12),
    detail: {
      description:
        'Processes, threads, scheduling, virtual memory, file systems, and the implementation of a small operating system kernel.',
      prerequisitesText: 'COMP 321',
    },
  }),
  section({
    crn: '12488',
    raw: 'COMP 430',
    title: 'Introduction to database systems',
    credits: 4,
    instructors: ['Jermaine, Christopher'],
    meetings: [meet('TR', 9, 25, 10, 40)],
    seats: seats(61, 80),
    detail: {
      description:
        'Relational data model, SQL, query processing, transactions, and an introduction to distributed data systems.',
      prerequisitesText: 'COMP 215',
    },
  }),
  section({
    crn: '12490',
    raw: 'COMP 440',
    title: 'Artificial intelligence',
    credits: 3,
    instructors: ['Vardi, Moshe'],
    meetings: [meet('TR', 13, 0, 14, 15)],
    seats: seats(71, 75),
    detail: {
      description:
        'Search, knowledge representation, planning, reasoning under uncertainty, and an introduction to machine learning.',
      prerequisitesText: 'COMP 182',
    },
  }),
  section({
    crn: '12501',
    raw: 'COMP 490',
    title: 'Computer science projects',
    credits: {kind: 'range', value: {min: 100, max: 400}},
    instructors: ['Nakhleh, Luay'],
    finalExam: 'noExam',
    detail: {
      courseType: 'Independent Study',
      description:
        'Individual or small-group projects under the supervision of a faculty member.',
      notes: ['Repeatable for Credit.', 'Instructor Permission Required.'],
    },
  }),
  section({
    crn: '12502',
    raw: 'COMP 490',
    section: '002',
    title: 'Computer science projects',
    credits: {kind: 'range', value: {min: 100, max: 400}},
    instructors: ['Jermaine, Christopher'],
    finalExam: 'noExam',
  }),
  section({
    crn: '13010',
    raw: 'MATH 101',
    title: 'Single variable calculus I',
    credits: 3,
    instructors: ['Hardt, Robert'],
    meetings: [meet('MWF', 9, 0, 9, 50)],
    seats: seats(30, 35),
    attributes: ['GRP3'],
    detail: {
      description:
        'Limits, continuity, derivatives, and the beginnings of integration.',
    },
  }),
  section({
    crn: '13014',
    raw: 'MATH 102',
    title: 'Single variable calculus II',
    credits: 3,
    instructors: ['Wolf, Michael'],
    meetings: [meet('MWF', 10, 0, 10, 50)],
    seats: seats(34, 35),
    attributes: ['GRP3'],
    detail: {
      description:
        'Techniques of integration, sequences and series, and an introduction to differential equations.',
      prerequisitesText: 'MATH 101',
    },
  }),
  section({
    crn: '13030',
    raw: 'MATH 212',
    title: 'Multivariable calculus',
    credits: 3,
    instructors: ['Goldman, Ron'],
    meetings: [meet('TR', 9, 25, 10, 40)],
    seats: seats(38, 45),
    attributes: ['GRP3'],
    detail: {
      description:
        'Vectors, partial derivatives, multiple integrals, and the theorems of Green, Gauss and Stokes.',
      prerequisitesText: 'MATH 102',
    },
  }),
  section({
    crn: '14210',
    raw: 'STAT 310',
    title: 'Probability and statistics',
    credits: 3,
    instructors: ['Scott, David'],
    meetings: [meet('MWF', 13, 0, 13, 50)],
    seats: seats(96, 100),
    attributes: ['GRP3'],
    detail: {
      description:
        'Probability, random variables, distributions, estimation, and hypothesis testing for students in engineering and the sciences.',
      prerequisitesText: 'MATH 212',
      notes: ['Cross-list: ECON 307.'],
    },
  }),
  section({
    crn: '14211',
    raw: 'STAT 310',
    section: '002',
    title: 'Probability and statistics',
    credits: 3,
    instructors: ['Scott, David'],
    meetings: [meet('MWF', 14, 0, 14, 50)],
    seats: seats(70, 100),
    attributes: ['GRP3'],
    detail: {
      description:
        'Probability, random variables, distributions, estimation, and hypothesis testing for students in engineering and the sciences.',
      prerequisitesText: 'MATH 212',
      notes: ['Cross-list: ECON 307.'],
    },
  }),
  section({
    crn: '14220',
    raw: 'STAT 315',
    title: 'Statistical inference',
    credits: 3,
    instructors: ['Cox, Dennis'],
    meetings: [meet('TR', 13, 0, 14, 15)],
    seats: seats(28, 40),
    detail: {
      description: 'Estimation, confidence intervals, likelihood, and testing.',
      prerequisitesText: 'STAT 310',
    },
  }),
  section({
    crn: '14230',
    raw: 'STAT 410',
    title: 'Linear regression',
    credits: 3,
    instructors: ['Ensor, Katherine'],
    meetings: [meet('MW', 16, 0, 17, 15)],
    seats: seats(35, 35),
    detail: {
      description:
        'Simple and multiple linear regression, diagnostics, model selection, and applications.',
      prerequisitesText: 'STAT 310',
    },
  }),
  section({
    crn: '14236',
    raw: 'STAT 413',
    title: 'Generalized linear models',
    credits: 3,
    instructors: ['Ensor, Katherine'],
    meetings: [meet('TR', 16, 0, 17, 15)],
    seats: seats(12, 30),
    detail: {
      description:
        'Logistic and Poisson regression, exponential families, and model checking.',
      prerequisitesText: 'STAT 410',
    },
  }),
  section({
    crn: '14240',
    raw: 'STAT 421',
    title: 'Applied time series',
    credits: 3,
    instructors: ['Ensor, Katherine'],
    meetings: [meet('MWF', 11, 0, 11, 50)],
    seats: seats(9, 30),
    detail: {
      description: 'ARIMA models, forecasting, and spectral methods.',
      prerequisitesText: 'STAT 410',
    },
  }),
  section({
    crn: '11890',
    raw: 'ELEC 303',
    title: 'Random signals in electrical engineering systems',
    credits: 3,
    instructors: ['Baraniuk, Richard'],
    meetings: [meet('MWF', 14, 0, 14, 50)],
    seats: seats(50, 60),
    detail: {
      description:
        'Probability and random processes with applications to signals and systems.',
    },
  }),
  section({
    crn: '11720',
    raw: 'ECON 307',
    title: 'Probability and statistics',
    credits: 3,
    instructors: ['Scott, David'],
    meetings: [meet('MWF', 13, 0, 13, 50)],
    seats: seats(20, 30),
    attributes: ['GRP3'],
    detail: {
      description:
        'Probability, random variables, distributions, estimation, and hypothesis testing for students in engineering and the sciences.',
      prerequisitesText: 'MATH 212',
      notes: ['Cross-list: STAT 310.'],
    },
  }),
  section({
    crn: '11705',
    raw: 'ECON 100',
    title: 'Principles of economics',
    credits: 3,
    instructors: ['Sickles, Robin'],
    meetings: [meet('TR', 9, 25, 10, 40)],
    seats: seats(180, 200),
    attributes: ['GRP2'],
    detail: {
      description:
        'Introduction to microeconomics and macroeconomics: markets, prices, national income, money, and policy.',
    },
  }),
  section({
    crn: '11990',
    raw: 'FWIS 149',
    section: 'S01',
    title: 'What is college for',
    credits: 3,
    instructors: ['Nakhleh, Luay'],
    meetings: [meet('TR', 13, 0, 14, 15)],
    seats: seats(15, 15, 3, 5),
    finalExam: 'noExam',
    detail: {
      courseType: 'Seminar',
      description:
        'A first-year writing-intensive seminar on the purposes of higher education, read through history, economics and memoir.',
      restrictions:
        'Enrollment limited to students with a classification of Freshman.',
    },
  }),
  section({
    crn: '11330',
    raw: 'CHEM 121',
    title: 'General chemistry I',
    credits: 3,
    instructors: ['Tran, Lesa'],
    meetings: [meet('MWF', 10, 0, 10, 50)],
    seats: seats(210, 240),
    attributes: ['GRP3'],
    finalExam: 'scheduledDeptRoom',
    detail: {
      description:
        'Atomic structure, bonding, stoichiometry, gases, thermochemistry, and an introduction to equilibrium.',
    },
  }),
  section({
    crn: '13700',
    raw: 'PHYS 101',
    title: 'Mechanics',
    credits: 3,
    instructors: ['Hafner, Jason'],
    meetings: [meet('MWF', 11, 0, 11, 50)],
    seats: seats(140, 160),
    attributes: ['GRP3'],
    detail: {
      description:
        'Kinematics, Newton’s laws, energy, momentum, rotation, and oscillations, with calculus.',
    },
  }),
  section({
    crn: '12950',
    raw: 'LPAP 170',
    title: 'Yoga',
    credits: 1,
    instructors: ['Sanchez, Maria'],
    meetings: [meet('MW', 8, 0, 8, 50)],
    seats: seats(20, 20),
    finalExam: 'noExam',
    detail: {
      courseType: 'Activity',
      gradeMode: 'Satisfactory/Unsatisfactory',
      description: 'Hatha yoga for beginners: postures, breathing and balance.',
      notes: ['Repeatable for Credit.'],
    },
  }),
  section({
    crn: '12951',
    raw: 'LPAP 170',
    section: '002',
    title: 'Yoga',
    credits: 1,
    instructors: ['Sanchez, Maria'],
    meetings: [meet('TR', 8, 0, 8, 50)],
    seats: seats(14, 20),
    finalExam: 'noExam',
    detail: {
      courseType: 'Activity',
      gradeMode: 'Satisfactory/Unsatisfactory',
      description: 'Hatha yoga for beginners: postures, breathing and balance.',
      notes: ['Repeatable for Credit.'],
    },
  }),
  section({
    crn: '12300',
    raw: 'HIST 117',
    title: 'The world since 1492',
    credits: 3,
    instructors: ['Fett, Rebecca'],
    meetings: [meet('TR', 14, 30, 15, 45)],
    seats: seats(44, 60),
    attributes: ['GRP1', 'AD'],
    detail: {
      description:
        'Global history from the Columbian exchange to the present: empires, trade, revolution, and decolonization.',
    },
  }),
  section({
    crn: '11850',
    raw: 'ENGL 200',
    title: 'Introduction to literary study',
    credits: 3,
    instructors: ['Wolfe, Cary'],
    meetings: [meet('MWF', 13, 0, 13, 50)],
    seats: seats(18, 20),
    attributes: ['GRP1'],
    finalExam: 'takeHome',
    detail: {
      description:
        'Close reading across poetry, drama and fiction, and the craft of the critical essay.',
    },
  }),
  section({
    crn: '13400',
    raw: 'MUSI 117',
    title: 'Fundamentals of music',
    credits: 3,
    instructors: ['Loewen, Peter'],
    meetings: [meet('TR', 10, 50, 12, 5)],
    seats: seats(22, 25),
    attributes: ['GRP2'],
    detail: {
      description:
        'Notation, scales, intervals, chords and rhythm for students with little or no formal training.',
    },
  }),
  section({
    crn: '13455',
    raw: 'MUSI 341',
    title: 'Junior recital',
    credits: 0,
    finalExam: 'noExam',
    detail: {
      courseType: 'Recital',
      gradeMode: 'Satisfactory/Unsatisfactory',
      description:
        'A public recital in the junior year, required of performance majors.',
      restrictions:
        'Enrollment limited to students in the Bachelor of Music program.',
    },
  }),
  section({
    crn: '13520',
    raw: 'MUSI 531',
    title: 'Applied piano',
    credits: {
      kind: 'either',
      value: [creditsFromHours(1), creditsFromHours(3)],
    },
    instructors: ['Chen, Jon'],
    finalExam: 'noExam',
    detail: {
      courseType: 'Private Lesson',
      description: 'Weekly private lessons.',
      notes: ['Repeatable for Credit.', 'Instructor Permission Required.'],
    },
  }),
  section({
    crn: '13521',
    raw: 'MUSI 531',
    section: '002',
    title: 'Applied violin',
    credits: {
      kind: 'either',
      value: [creditsFromHours(1), creditsFromHours(3)],
    },
    instructors: ['Kim, Cho-Liang'],
    finalExam: 'noExam',
  }),
  section({
    crn: '13620',
    raw: 'PHIL 101',
    title: 'Introduction to philosophy',
    credits: 3,
    instructors: ['Grandy, Richard'],
    meetings: [meet('MWF', 14, 0, 14, 50)],
    seats: seats(48, 60),
    attributes: ['GRP1'],
    detail: {
      description:
        'Knowledge, mind, freedom, God and value, read through classic and contemporary texts.',
    },
  }),
  section({
    crn: '12250',
    raw: 'HART 101',
    title: 'Introduction to art history',
    credits: 3,
    instructors: ['Costello, Diane'],
    meetings: [meet('TR', 13, 0, 14, 15)],
    seats: seats(60, 60, 4, 10),
    attributes: ['GRP1'],
    detail: {
      description:
        'How to look at art: painting, sculpture, architecture and photography from antiquity to the present.',
    },
  }),
  section({
    crn: '10410',
    raw: 'ANTH 201',
    title: 'Introduction to cultural anthropology',
    credits: 3,
    instructors: ['Faubion, James'],
    meetings: [meet('MWF', 10, 0, 10, 50)],
    seats: seats(38, 50),
    attributes: ['GRP2', 'AD'],
    detail: {
      description:
        'Culture, kinship, exchange, ritual and power, through ethnographies from around the world.',
    },
  }),
  section({
    crn: '14300',
    raw: 'SOCI 231',
    title: 'Race and ethnic relations',
    credits: 3,
    instructors: ['Emerson, Michael'],
    meetings: [meet('TR', 16, 0, 17, 15)],
    seats: seats(45, 45),
    attributes: ['GRP2', 'AD'],
    detail: {
      description:
        'The social construction of race and ethnicity, inequality, immigration, and identity in the United States.',
    },
  }),
];
