import {describe, expect, it} from 'vitest';

import {fallSections} from '../fixtures/fallSections';
import {
  formatAsOf,
  formatMeetingTime,
  formatMeetings,
  isScheduled,
  seatStatus,
  seatsOpen,
} from './section';

describe('seats', () => {
  it('grades open, nearly full, waitlist and full from the same numbers', () => {
    const base = {waitlistCount: 0, waitlistCapacity: 0, asOf: ''};
    expect(seatStatus({...base, enrolled: 60, capacity: 72})).toBe('open');
    expect(seatStatus({...base, enrolled: 67, capacity: 72})).toBe(
      'nearlyFull',
    );
    expect(seatStatus({...base, enrolled: 72, capacity: 72})).toBe('full');
    expect(
      seatStatus({...base, enrolled: 72, capacity: 72, waitlistCapacity: 20}),
    ).toBe('waitlistOpen');
  });

  it('never goes negative for an over-enrolled section', () => {
    expect(
      seatsOpen({
        enrolled: 75,
        capacity: 72,
        waitlistCount: 0,
        waitlistCapacity: 0,
        asOf: '',
      }),
    ).toBe(0);
  });
});

describe('meeting labels', () => {
  it('prints the meridiem once when both ends share it', () => {
    expect(formatMeetingTime({days: ['T', 'R'], start: 870, end: 945})).toBe(
      '2:30–3:45 PM TR',
    );
    expect(formatMeetingTime({days: ['T', 'R'], start: 650, end: 725})).toBe(
      '10:50 AM–12:05 PM TR',
    );
    expect(
      formatMeetingTime({days: ['M', 'W', 'F'], start: 780, end: 830}),
    ).toBe('1–1:50 PM MWF');
  });

  it('joins a lecture and its lab with a middle dot', () => {
    const comp222 = fallSections.find(s => s.listing.crn === '12455');
    expect(comp222 && formatMeetings(comp222.listing)).toBe(
      '3–3:50 PM MWF · 4–5:15 PM R',
    );
  });

  it("reads Rice's timestamp in its own offset", () => {
    expect(formatAsOf('2026-09-11T19:09:12-05:00')).toBe('7:09 PM');
  });
});

describe('fixture', () => {
  it('has unique CRNs and unscheduled rows to hide', () => {
    const crns = fallSections.map(s => s.listing.crn);
    expect(new Set(crns).size).toBe(crns.length);
    expect(
      fallSections.filter(s => !isScheduled(s.listing)).length,
    ).toBeGreaterThan(3);
  });
});
