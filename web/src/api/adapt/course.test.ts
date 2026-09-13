import {describe, expect, it} from 'vitest';

import type {CourseView as WireCourseView} from '../generated/CourseView';
import {
  courseFactsFromWire,
  courseInfoFromWire,
  courseViewSections,
} from './course';

const view: WireCourseView = {
  course: {
    catalogYear: 2026,
    code: {subject: 'COMP', number: '140'},
    title: 'Computational Thinking',
    credits: {kind: 'fixed', value: 300},
    department: 'Computer Science',
    attributes: ['GRP3'],
    description: 'An introduction.',
    flags: {repeatable: false, instructorPermission: true, secondHalf: false},
    mutualExclusions: [
      {with: [{subject: 'COMP', number: '182'}], raw: 'No COMP 182.'},
    ],
    crossList: [],
    equivalents: [],
  },
  sections: [
    {
      listing: {
        crn: 12345,
        term: '202710',
        code: {subject: 'COMP', number: '140'},
        section: '001',
        title: 'COMPUTATIONAL THINKING',
        credits: {kind: 'fixed', value: 300},
        instructors: [],
        meetings: [],
        finalExam: 'unknown',
      },
      detail: {
        longTitle: 'Computational Thinking',
        description: 'An introduction.',
        department: 'Computer Science',
        attributes: ['GRP3'],
        reserved: [],
        fees: [],
        hasSyllabus: false,
        fetchedAt: 1,
      },
      seats: {
        enrolled: 1,
        capacity: 2,
        waitlistCount: 0,
        waitlistCapacity: 0,
        asOf: Date.UTC(2026, 8, 12, 0, 0, 0) / 1000,
      },
    },
  ],
  links: {
    riceCoursePage: 'https://x',
    estherSyllabus: null,
    estherEvaluations: null,
  },
  offered: true,
};

describe('courseInfoFromWire', () => {
  it('takes what the course record knows and zeroes the rest', () => {
    expect(courseInfoFromWire(view.course)).toEqual({
      code: {subject: 'COMP', number: '140'},
      title: 'Computational Thinking',
      credits: {kind: 'fixed', value: 300},
      attributes: ['GRP3'],
      department: 'Computer Science',
      repeatable: false,
      seasonsOffered: [],
      termsObserved: 0,
      offeredNow: false,
    });
  });
});

describe('courseFactsFromWire', () => {
  it('drops a null department', () => {
    const facts = courseFactsFromWire({
      courses: [
        {
          code: {subject: 'COMP', number: '140'},
          title: 'T',
          credits: {kind: 'fixed', value: 300},
          attributes: [],
          department: null,
          repeatable: true,
          seasonsOffered: ['fall'],
          termsObserved: 2,
          offeredNow: false,
        },
      ],
      aliases: [],
    });
    expect(facts.courses[0]).not.toHaveProperty('department');
    expect(facts.courses[0]?.repeatable).toBe(true);
  });
});

describe('courseViewSections', () => {
  it('fills notes and mutuallyExclusive from the course and converts seats', () => {
    const sections = courseViewSections(view, '2026-09-11T19:00:00-05:00');
    expect(sections).toHaveLength(1);
    expect(sections[0]?.listing.crn).toBe('12345');
    expect(sections[0]?.detail?.notes).toEqual([
      'Instructor Permission Required.',
    ]);
    expect(sections[0]?.detail?.mutuallyExclusive).toBe('No COMP 182.');
    expect(sections[0]?.seats?.asOf).toBe('2026-09-11T19:00:00-05:00');
  });
});
