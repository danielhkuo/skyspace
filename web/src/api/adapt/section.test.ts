import {describe, expect, it} from 'vitest';

import type {CatalogQuery} from '../../domain/catalog';
import type {Course as WireCourse} from '../generated/Course';
import type {Section as WireSection} from '../generated/Section';
import type {SectionPage as WireSectionPage} from '../generated/SectionPage';
import {toSearchParams} from '../http';
import {
  asOfFromWire,
  catalogQueryToWire,
  crnFromWire,
  crnToWire,
  daysFromWire,
  daysToWire,
  dayToQuery,
  finalExamFromWire,
  meetingFromWire,
  meetingToWire,
  sectionFromWire,
  sectionPageFromWire,
} from './section';

const wireSection: WireSection = {
  listing: {
    crn: 12345,
    term: '202710',
    code: {subject: 'COMP', number: '140'},
    section: '001',
    title: 'COMPUTATIONAL THINKING',
    credits: {kind: 'fixed', value: 300},
    partOfTerm: '1',
    instructors: [{name: 'A. Person', netId: 'ap1'}, {name: 'B. Person'}],
    meetings: [
      {
        pattern: {kind: 'timed', value: {days: 'TR', start: 870, end: 945}},
        dates: {
          start: {year: 2026, month: 8, day: 24},
          end: {year: 2026, month: 12, day: 4},
        },
      },
      {pattern: {kind: 'unparsed', value: 'TBA'}},
    ],
    finalExam: 'scheduled_dept_room',
  },
  detail: {
    longTitle: 'Computational Thinking',
    description: 'An introduction.',
    department: 'Computer Science',
    attributes: ['GRP3'],
    prerequisitesText: null,
    restrictions: {
      raw: 'Must be enrolled in Undergraduate',
      clauses: [
        {effect: 'must_be', dimension: {kind: 'level'}, values: ['UG']},
      ],
    },
    gradeMode: 'Standard Letter',
    methodOfInstruction: 'Face to Face',
    courseType: null,
    language: null,
    reserved: [{label: 'Matriculants', capacity: 60, available: 2}],
    fees: [{label: 'Lab', amountCents: 2500, raw: '$25'}],
    hasSyllabus: true,
    fetchedAt: 1_757_600_000,
  },
  seats: null,
};

const wireCourse: WireCourse = {
  catalogYear: 2026,
  code: {subject: 'COMP', number: '140'},
  title: 'Computational Thinking',
  credits: {kind: 'fixed', value: 300},
  department: 'Computer Science',
  attributes: ['GRP3'],
  description: 'An introduction.',
  flags: {repeatable: true, instructorPermission: true, secondHalf: false},
  mutualExclusions: [
    {
      with: [{subject: 'COMP', number: '182'}],
      raw: 'Cannot register for COMP 140 if student has credit for COMP 182.',
    },
  ],
  crossList: [],
  equivalents: [],
};

describe('crn', () => {
  it('zero-pads to five digits and parses back', () => {
    expect(crnFromWire(123)).toBe('00123');
    expect(crnFromWire(12345)).toBe('12345');
    expect(crnToWire('00123')).toBe(123);
    expect(crnToWire(crnFromWire(98765))).toBe(98765);
  });

  it('rejects text that is not a number', () => {
    expect(() => crnToWire('abc')).toThrow(/unknown wire value/);
  });
});

describe('days', () => {
  it('splits Rice letters and joins them back', () => {
    expect(daysFromWire('MWF')).toEqual(['M', 'W', 'F']);
    expect(daysFromWire('')).toEqual([]);
    expect(daysToWire(['T', 'R'])).toBe('TR');
  });

  it('rejects an unknown letter', () => {
    expect(() => daysFromWire('MX')).toThrow(/unknown wire value/);
  });

  it('maps M to mon for the query string', () => {
    expect(dayToQuery('M')).toBe('mon');
    expect(dayToQuery('R')).toBe('thu');
    expect(dayToQuery('U')).toBe('sun');
  });
});

describe('finalExamFromWire', () => {
  it('renames the snake_case kinds', () => {
    expect(finalExamFromWire('scheduled_dept_room')).toBe('scheduledDeptRoom');
    expect(finalExamFromWire('no_exam')).toBe('noExam');
    expect(finalExamFromWire('unknown')).toBe('unknown');
  });
});

describe('asOfFromWire', () => {
  it('uses the offset carried by riceAsOf', () => {
    // 2026-09-12T00:09:12Z
    const seconds = Date.UTC(2026, 8, 12, 0, 9, 12) / 1000;
    expect(asOfFromWire(seconds, '2026-09-11T19:00:00-05:00')).toBe(
      '2026-09-11T19:09:12-05:00',
    );
  });

  it('falls back to Central daylight time in September', () => {
    const seconds = Date.UTC(2026, 8, 12, 0, 9, 12) / 1000;
    expect(asOfFromWire(seconds)).toBe('2026-09-11T19:09:12-05:00');
  });

  it('falls back to Central standard time in January', () => {
    const seconds = Date.UTC(2027, 0, 15, 12, 0, 0) / 1000;
    expect(asOfFromWire(seconds)).toBe('2027-01-15T06:00:00-06:00');
  });

  it('treats a Z stamp as carrying no campus offset', () => {
    const seconds = Date.UTC(2027, 0, 15, 12, 0, 0) / 1000;
    expect(asOfFromWire(seconds, '2027-01-15T12:00:00Z')).toBe(
      '2027-01-15T06:00:00-06:00',
    );
  });

  it('switches on the second Sunday of March and the first of November', () => {
    // 2026-03-08 is the second Sunday; 2:00 CST = 08:00 UTC.
    const before = Date.UTC(2026, 2, 8, 7, 59, 59) / 1000;
    const after = Date.UTC(2026, 2, 8, 8, 0, 0) / 1000;
    expect(asOfFromWire(before)).toMatch(/-06:00$/);
    expect(asOfFromWire(after)).toMatch(/-05:00$/);
    // 2026-11-01 is the first Sunday; 2:00 CDT = 07:00 UTC.
    const beforeEnd = Date.UTC(2026, 10, 1, 6, 59, 59) / 1000;
    const afterEnd = Date.UTC(2026, 10, 1, 7, 0, 0) / 1000;
    expect(asOfFromWire(beforeEnd)).toMatch(/-05:00$/);
    expect(asOfFromWire(afterEnd)).toMatch(/-06:00$/);
  });
});

describe('meetings', () => {
  it('round-trips a timed meeting with dates and an unparsed one without', () => {
    for (const wire of wireSection.listing.meetings) {
      expect(meetingToWire(meetingFromWire(wire))).toEqual(wire);
    }
  });
});

describe('sectionFromWire', () => {
  it('converts the listing, keeps both meetings and leaves null seats absent', () => {
    const section = sectionFromWire(wireSection);
    expect(section.listing.crn).toBe('12345');
    expect(section.listing.partOfTerm).toBe('1');
    expect(section.listing.finalExam).toBe('scheduledDeptRoom');
    expect(section.listing.instructors).toEqual([
      {name: 'A. Person', netId: 'ap1'},
      {name: 'B. Person'},
    ]);
    expect(section.listing.meetings).toHaveLength(2);
    expect(section.listing.meetings[0]?.pattern).toEqual({
      kind: 'timed',
      value: {days: ['T', 'R'], start: 870, end: 945},
    });
    expect(section.listing.meetings[1]).toEqual({
      pattern: {kind: 'unparsed', value: 'TBA'},
    });
    expect('seats' in section).toBe(false);
  });

  it('flattens restrictions to raw and drops fees and fetchedAt', () => {
    const detail = sectionFromWire(wireSection).detail;
    expect(detail?.restrictions).toBe('Must be enrolled in Undergraduate');
    expect(detail?.gradeMode).toBe('Standard Letter');
    expect(detail).not.toHaveProperty('fees');
    expect(detail).not.toHaveProperty('fetchedAt');
    expect(detail).not.toHaveProperty('prerequisitesText');
    expect(detail?.notes).toEqual([]);
    expect(detail?.mutuallyExclusive).toBeUndefined();
  });

  it('synthesises notes and mutuallyExclusive from the course', () => {
    const detail = sectionFromWire(wireSection, wireCourse).detail;
    expect(detail?.notes).toEqual([
      'Repeatable for Credit.',
      'Instructor Permission Required.',
    ]);
    expect(detail?.mutuallyExclusive).toBe(
      'Cannot register for COMP 140 if student has credit for COMP 182.',
    );
  });

  it('converts seats asOf into an ISO stamp in the riceAsOf offset', () => {
    const seconds = Date.UTC(2026, 8, 12, 0, 9, 12) / 1000;
    const section = sectionFromWire(
      {
        ...wireSection,
        seats: {
          enrolled: 60,
          capacity: 72,
          waitlistCount: 0,
          waitlistCapacity: 10,
          asOf: seconds,
        },
      },
      undefined,
      '2026-09-11T19:00:00-05:00',
    );
    expect(section.seats?.asOf).toBe('2026-09-11T19:09:12-05:00');
    expect(section.seats?.enrolled).toBe(60);
  });
});

describe('sectionPageFromWire', () => {
  it('drops applied and suggestions', () => {
    const page: WireSectionPage = {
      rows: [wireSection],
      total: 1,
      offset: 0,
      limit: 25,
      hasMore: false,
      unscheduledHidden: 3,
      courseCount: 1,
      applied: {
        term: '202710',
        q: null,
        subject: [],
        department: [],
        school: [],
        attr: [],
        level: [],
        levelMin: null,
        levelMax: null,
        creditsMin: null,
        creditsMax: null,
        days: [],
        startsAfter: null,
        endsBefore: null,
        partOfTerm: [],
        openSeatsOnly: false,
        scheduledOnly: true,
        sort: 'relevance',
        offset: 0,
        limit: 25,
      },
      suggestions: [{field: 'days', wouldMatch: 4}],
    };
    const out = sectionPageFromWire(page);
    expect(out).toEqual({
      rows: [sectionFromWire(wireSection)],
      total: 1,
      offset: 0,
      limit: 25,
      hasMore: false,
      unscheduledHidden: 3,
      courseCount: 1,
    });
    expect(out).not.toHaveProperty('applied');
    expect(out).not.toHaveProperty('suggestions');
  });
});

describe('catalogQueryToWire', () => {
  const base: CatalogQuery = {
    q: '',
    subject: [],
    attr: [],
    level: [],
    days: [],
    partOfTerm: [],
    openSeatsOnly: false,
    scheduledOnly: true,
    sort: 'relevance',
    offset: 0,
    limit: 25,
  };

  it('always emits scheduledOnly and drops empty text and lists', () => {
    const params = toSearchParams(catalogQueryToWire('202710', base));
    expect(params.toString()).toBe(
      'term=202710&openSeatsOnly=false&scheduledOnly=true&sort=relevance&offset=0&limit=25',
    );
  });

  it('repeats keys and maps days to the query vocabulary', () => {
    const params = toSearchParams(
      catalogQueryToWire('202710', {
        ...base,
        q: '  linear  ',
        subject: ['COMP', 'MATH'],
        attr: ['GRP3'],
        level: [100, 200],
        days: ['M', 'W', 'F'],
        creditsMin: 300,
        startsAfter: 600,
        endsBefore: 1020,
        partOfTerm: ['1'],
        scheduledOnly: false,
        sort: 'courseNumber',
        offset: 25,
        limit: 40,
      }),
    );
    expect(params.getAll('subject')).toEqual(['COMP', 'MATH']);
    expect(params.getAll('days')).toEqual(['mon', 'wed', 'fri']);
    expect(params.getAll('level')).toEqual(['100', '200']);
    expect(params.get('q')).toBe('linear');
    expect(params.get('scheduledOnly')).toBe('false');
    expect(params.get('creditsMin')).toBe('300');
    expect(params.get('creditsMax')).toBeNull();
    expect(params.get('startsAfter')).toBe('600');
    expect(params.get('endsBefore')).toBe('1020');
    expect(params.get('partOfTerm')).toBe('1');
    expect(params.get('sort')).toBe('courseNumber');
    expect(params.get('offset')).toBe('25');
    expect(params.get('limit')).toBe('40');
    expect(params.toString()).not.toContain(',');
  });
});
