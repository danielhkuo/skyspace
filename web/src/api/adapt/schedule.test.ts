import {describe, expect, it} from 'vitest';

import type {TermSchedule as WireTermSchedule} from '../generated/TermSchedule';
import {
  meetingOwnerFromWire,
  meetingOwnerToWire,
  scheduleFromWire,
  scheduleToWire,
} from './schedule';

const wireSchedule: WireTermSchedule = {
  id: 'sched-1',
  name: 'Plan A',
  term: '202710',
  candidates: [
    {
      course: {subject: 'COMP', number: '140'},
      sections: [12345, 123],
      visible: true,
      colour: 3,
    },
    {
      course: {subject: 'MATH', number: '212'},
      sections: [],
      visible: false,
      colour: 0,
    },
  ],
  busy: [
    {
      id: 'busy-1',
      label: 'Work',
      meeting: {
        pattern: {kind: 'timed', value: {days: 'MWF', start: 540, end: 600}},
      },
    },
    {
      id: 'busy-2',
      label: 'Practice',
      meeting: {
        pattern: {kind: 'timed', value: {days: 'TR', start: 1020, end: 1080}},
        dates: {
          start: {year: 2026, month: 9, day: 1},
          end: {year: 2026, month: 11, day: 30},
        },
      },
    },
  ],
};

describe('schedule round trip', () => {
  it('scheduleToWire(scheduleFromWire(w)) reproduces the wire document', () => {
    expect(scheduleToWire(scheduleFromWire(wireSchedule))).toStrictEqual(
      wireSchedule,
    );
  });

  it('pads CRNs and splits busy-block days inbound', () => {
    const schedule = scheduleFromWire(wireSchedule);
    expect(schedule.candidates[0]?.sections).toEqual(['12345', '00123']);
    expect(schedule.busy[0]?.meeting.pattern).toEqual({
      kind: 'timed',
      value: {days: ['M', 'W', 'F'], start: 540, end: 600},
    });
    expect(schedule.busy[0]?.meeting).not.toHaveProperty('dates');
  });
});

describe('meeting owner', () => {
  it('converts the section CRN and passes the busy id through', () => {
    expect(meetingOwnerFromWire({kind: 'section', value: 123})).toEqual({
      kind: 'section',
      value: '00123',
    });
    expect(
      meetingOwnerToWire(meetingOwnerFromWire({kind: 'busy', value: 'b'})),
    ).toEqual({
      kind: 'busy',
      value: 'b',
    });
    expect(
      meetingOwnerToWire(meetingOwnerFromWire({kind: 'section', value: 123})),
    ).toEqual({
      kind: 'section',
      value: 123,
    });
  });

  it('throws on an unknown kind', () => {
    expect(() =>
      meetingOwnerFromWire({kind: 'bogus', value: 1} as unknown as {
        kind: 'section';
        value: number;
      }),
    ).toThrow(/unknown wire value for meetingOwner/);
  });
});
