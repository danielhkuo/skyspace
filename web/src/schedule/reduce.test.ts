import {describe, expect, it} from 'vitest';

import {parseCourseCode, visibleCredits, type CourseCode} from '../domain';
import {fallSchedule} from '../fixtures/fallSchedule';
import {fallSections} from '../fixtures/fallSections';
import {nextScheduleName} from './labels';
import {freeColour, reduceSchedule} from './reduce';

const code = (raw: string): CourseCode => {
  const c = parseCourseCode(raw);
  if (c === null) {
    throw new Error(raw);
  }
  return c;
};
const byCrn = new Map(fallSections.map(s => [s.listing.crn, s]));

describe('reduceSchedule', () => {
  it('adds a course with the first unused colour and never twice', () => {
    const once = reduceSchedule(fallSchedule, {
      type: 'addCandidate',
      course: code('MATH 212'),
      crn: '13030',
    });
    expect(once.candidates).toHaveLength(7);
    expect(once.candidates[6]?.colour).toBe(freeColour(fallSchedule));
    expect(new Set(once.candidates.map(c => c.colour)).size).toBe(7);
    const twice = reduceSchedule(once, {
      type: 'addCandidate',
      course: code('MATH 212'),
    });
    expect(twice.candidates).toHaveLength(7);
  });

  it('adding a hidden course again shows it and re-picks the section', () => {
    const next = reduceSchedule(fallSchedule, {
      type: 'addCandidate',
      course: code('FWIS 149'),
      crn: '11990',
    });
    const fwis = next.candidates.find(c => c.course.subject === 'FWIS');
    expect(fwis?.visible).toBe(true);
    expect(fwis?.sections).toEqual(['11990']);
  });

  it('the visible total counts picked sections of shown candidates only', () => {
    // COMP 140 (4) + COMP 222 (4) + STAT 310 (3) + MUSI 117 (3) + LPAP 170 (1); FWIS hidden.
    expect(visibleCredits(fallSchedule, byCrn)).toBe(1500);
    const hidden = reduceSchedule(fallSchedule, {
      type: 'toggleVisible',
      course: code('COMP 140'),
    });
    expect(visibleCredits(hidden, byCrn)).toBe(1100);
    const unpicked = reduceSchedule(fallSchedule, {
      type: 'removeCandidate',
      course: code('LPAP 170'),
    });
    expect(visibleCredits(unpicked, byCrn)).toBe(1400);
  });

  it('names the next schedule past the letters taken', () => {
    expect(nextScheduleName(['Schedule A'])).toBe('Schedule B');
    expect(nextScheduleName(['Schedule B'])).toBe('Schedule A');
  });
});
