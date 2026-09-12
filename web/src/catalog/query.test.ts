import {describe, expect, it} from 'vitest';

import {fallSections} from '../fixtures/fallSections';
import {
  activeFilterCount,
  applyQuery,
  EMPTY_QUERY,
  parseQuery,
  parseTimeText,
  serializeQuery,
} from './query';

const crns = (rows: {listing: {crn: string}}[]): string[] =>
  rows.map(r => r.listing.crn);

describe('search', () => {
  it('matches a code however it is spelled', () => {
    for (const q of ['comp140', 'COMP 140', 'comp 140', 'Comp-140']) {
      expect(crns(applyQuery(fallSections, {...EMPTY_QUERY, q}).rows)).toEqual([
        '12422',
        '12423',
      ]);
    }
  });

  it('ranks an exact code above a prefix, then titles, then instructors', () => {
    const rows = applyQuery(fallSections, {...EMPTY_QUERY, q: 'comp 1'}).rows;
    expect(crns(rows).slice(0, 3)).toEqual(['12422', '12423', '12430']);
    const byName = applyQuery(fallSections, {...EMPTY_QUERY, q: 'warren'}).rows;
    expect(crns(byName)).toEqual(['12422', '12423']);
    const byTitle = applyQuery(fallSections, {
      ...EMPTY_QUERY,
      q: 'probability',
    }).rows;
    expect(crns(byTitle)).toContain('14210');
    expect(crns(byTitle)).toContain('11720');
  });
});

describe('filters', () => {
  it('hides unscheduled rows by default, counts them, and never hides MUSI', () => {
    const all = applyQuery(fallSections, EMPTY_QUERY);
    expect(all.hiddenUnscheduled).toBe(2);
    expect(crns(all.rows)).toContain('13455');
    expect(crns(all.rows)).not.toContain('12501');
    const shown = applyQuery(fallSections, {
      ...EMPTY_QUERY,
      showUnscheduled: true,
    });
    expect(shown.hiddenUnscheduled).toBe(0);
    expect(crns(shown.rows)).toContain('12501');
  });

  it('treats not-polled as excluded by "open seats only"', () => {
    const open = applyQuery(fallSections, {
      ...EMPTY_QUERY,
      openOnly: true,
      showUnscheduled: true,
    });
    expect(crns(open.rows)).not.toContain('12501');
    expect(crns(open.rows)).not.toContain('12423');
    expect(crns(open.rows)).toContain('12422');
  });

  it('filters by distribution, level, days and time window', () => {
    const dist = applyQuery(fallSections, {
      ...EMPTY_QUERY,
      distribution: ['AD'],
    });
    expect(crns(dist.rows).sort()).toEqual(['10410', '12300', '14300']);
    const level = applyQuery(fallSections, {
      ...EMPTY_QUERY,
      subjects: ['COMP'],
      levels: [400],
    });
    expect(crns(level.rows)).toEqual(['12475', '12481', '12488', '12490']);
    const friday = applyQuery(fallSections, {
      ...EMPTY_QUERY,
      subjects: ['COMP'],
      days: ['F'],
    });
    expect(crns(friday.rows)).toEqual(['12430', '12441', '12455', '12481']);
    const morning = applyQuery(fallSections, {
      ...EMPTY_QUERY,
      subjects: ['COMP'],
      endsBefore: 12 * 60,
    });
    expect(crns(morning.rows)).toEqual(['12481', '12488']);
  });

  it('matches credit hours inside a range and either-or', () => {
    const two = applyQuery(fallSections, {
      ...EMPTY_QUERY,
      creditHours: 2,
      showUnscheduled: true,
    });
    expect(crns(two.rows)).toEqual(['12501', '12502']);
    const one = applyQuery(fallSections, {...EMPTY_QUERY, creditHours: 1});
    expect(crns(one.rows)).toEqual(['12950', '12951', '13520', '13521']);
  });

  it('sorts by open seats with not-polled last', () => {
    const rows = applyQuery(fallSections, {
      ...EMPTY_QUERY,
      subjects: ['COMP'],
      sort: 'openSeats',
      showUnscheduled: true,
    }).rows;
    expect(rows[0]?.listing.crn).toBe('12488');
    expect(crns(rows).slice(-2)).toEqual(['12501', '12502']);
  });
});

describe('URL round trip', () => {
  it('writes only what differs from the default and reads it back', () => {
    expect(serializeQuery(EMPTY_QUERY).toString()).toBe('');
    const q = {
      ...EMPTY_QUERY,
      q: 'comp 140',
      subjects: ['COMP'],
      distribution: ['GRP3' as const],
      levels: [100, 200],
      days: ['T' as const, 'R' as const],
      startsAfter: 540,
      openOnly: true,
      sort: 'openSeats' as const,
      crn: '12422',
    };
    const params = serializeQuery(q);
    expect(params.toString()).toBe(
      'q=comp+140&subj=COMP&dist=GRP3&level=100%2C200&days=TR&after=540&open=1&sort=openSeats&crn=12422',
    );
    expect(parseQuery(params)).toEqual(q);
    expect(activeFilterCount(q)).toBe(7);
  });

  it('drops junk from a hand-edited URL', () => {
    const q = parseQuery(
      new URLSearchParams(
        'dist=GRP9,ad&level=150,300&days=xMz&after=9999&crn=12',
      ),
    );
    expect(q.distribution).toEqual(['AD']);
    expect(q.levels).toEqual([300]);
    expect(q.days).toEqual(['M']);
    expect(q.startsAfter).toBeUndefined();
    expect(q.crn).toBeUndefined();
  });
});

describe('time text', () => {
  it('reads the ways people type a time', () => {
    expect(parseTimeText('9:00 AM')).toBe(540);
    expect(parseTimeText('9am')).toBe(540);
    expect(parseTimeText('3:30pm')).toBe(930);
    expect(parseTimeText('15:00')).toBe(900);
    expect(parseTimeText('12 pm')).toBe(720);
    expect(parseTimeText('noon')).toBeUndefined();
    expect(parseTimeText('13 pm')).toBeUndefined();
  });
});
