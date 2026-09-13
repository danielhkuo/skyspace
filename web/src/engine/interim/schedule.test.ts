import {describe, expect, it} from 'vitest';

import type {Day, Meeting, ScheduledMeeting} from '../../domain';
import {findConflictsInterim} from './schedule';

const timed = (
  days: string,
  start: number,
  end: number,
  dates?: Meeting['dates'],
): Meeting => ({
  pattern: {
    kind: 'timed',
    value: {days: days.split('') as Day[], start, end},
  },
  ...(dates === undefined ? {} : {dates}),
});

const at = (
  crn: string,
  meeting: Meeting,
  partOfTerm?: string,
): ScheduledMeeting => ({
  owner: {kind: 'section', value: crn},
  meeting,
  ...(partOfTerm === undefined ? {} : {partOfTerm}),
});

describe('findConflicts', () => {
  it('names the shared days of an overlap, each pair once', () => {
    const found = findConflictsInterim([
      at('1', timed('TR', 16 * 60, 17 * 60 + 15)),
      at('2', timed('R', 16 * 60, 17 * 60 + 15)),
      at('3', timed('MWF', 15 * 60, 15 * 60 + 50)),
    ]);
    expect(found).toEqual([
      {
        a: {kind: 'section', value: '1'},
        b: {kind: 'section', value: '2'},
        days: ['R'],
      },
    ]);
  });

  it('touching times do not conflict', () => {
    expect(
      findConflictsInterim([
        at('1', timed('TR', 9 * 60 + 25, 10 * 60 + 40)),
        at('2', timed('TR', 10 * 60 + 40, 11 * 60 + 55)),
      ]),
    ).toEqual([]);
  });

  it('summer sessions do not conflict: disjoint dates, or different parts of term with no dates', () => {
    const first = {
      start: {year: 2027, month: 5, day: 24},
      end: {year: 2027, month: 6, day: 25},
    };
    const second = {
      start: {year: 2027, month: 7, day: 5},
      end: {year: 2027, month: 8, day: 6},
    };
    expect(
      findConflictsInterim([
        at('1', timed('MW', 600, 660, first)),
        at('2', timed('MW', 600, 660, second)),
      ]),
    ).toEqual([]);
    expect(
      findConflictsInterim([
        at('1', timed('MW', 600, 660), '1st Summer Session'),
        at('2', timed('MW', 600, 660), '2nd Summer Session'),
      ]),
    ).toEqual([]);
    expect(
      findConflictsInterim([
        at('1', timed('MW', 600, 660), 'Full Term'),
        at('2', timed('MW', 600, 660), 'Full Term'),
      ]),
    ).toHaveLength(1);
  });

  it('a section never conflicts with its own second meeting; unparsed patterns are ignored', () => {
    expect(
      findConflictsInterim([
        at('1', timed('R', 600, 660)),
        at('1', timed('R', 600, 660)),
        at('2', {pattern: {kind: 'unparsed', value: 'TBA'}}),
      ]),
    ).toEqual([]);
  });
});
