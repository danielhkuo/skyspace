import {describe, expect, it} from 'vitest';

import {fallSections} from '../fixtures/fallSections';
import {
  activeFilterCount,
  applyQuery,
  DEFAULT_QUERY,
  parseCatalogUrl,
  parseTimeText,
  serializeCatalogUrl,
} from './query';

const crns = (rows: {listing: {crn: string}}[]): string[] =>
  rows.map(r => r.listing.crn);
const run = (patch: Partial<typeof DEFAULT_QUERY>) =>
  applyQuery(fallSections, {...DEFAULT_QUERY, ...patch});

describe('search', () => {
  it('matches a code however it is spelled', () => {
    for (const q of ['comp140', 'COMP 140', 'comp 140', 'Comp-140']) {
      expect(crns(run({q}).rows)).toEqual(['12422', '12423']);
    }
  });

  it('ranks an exact code above a prefix, then titles, then instructors', () => {
    expect(crns(run({q: 'comp 1'}).rows).slice(0, 3)).toEqual([
      '12422',
      '12423',
      '12430',
    ]);
    expect(crns(run({q: 'warren'}).rows)).toEqual(['12422', '12423']);
    const byTitle = crns(run({q: 'probability'}).rows);
    expect(byTitle).toContain('14210');
    expect(byTitle).toContain('11720');
  });
});

describe('filters', () => {
  it('hides unscheduled rows by default, counts them, and never hides MUSI', () => {
    const page = run({limit: 100});
    expect(page.unscheduledHidden).toBe(2);
    expect(crns(page.rows)).toContain('13455');
    expect(crns(page.rows)).not.toContain('12501');
    const shown = run({scheduledOnly: false, limit: 100});
    expect(shown.unscheduledHidden).toBe(0);
    expect(crns(shown.rows)).toContain('12501');
  });

  it('treats not-polled as excluded by "open seats only"', () => {
    const open = crns(
      run({openSeatsOnly: true, scheduledOnly: false, limit: 100}).rows,
    );
    expect(open).not.toContain('12501');
    expect(open).not.toContain('12423');
    expect(open).toContain('12422');
  });

  it('filters by attribute, level, days and time window', () => {
    expect(crns(run({attr: ['AD']}).rows).sort()).toEqual([
      '10410',
      '12300',
      '14300',
    ]);
    expect(crns(run({subject: ['COMP'], level: [400]}).rows)).toEqual([
      '12475',
      '12481',
      '12488',
      '12490',
    ]);
    expect(crns(run({subject: ['COMP'], days: ['F']}).rows)).toEqual([
      '12430',
      '12441',
      '12455',
      '12481',
    ]);
    expect(crns(run({subject: ['COMP'], endsBefore: 12 * 60}).rows)).toEqual([
      '12481',
      '12488',
    ]);
  });

  it('matches a credit window against ranges and either-or', () => {
    expect(
      crns(run({creditsMin: 200, creditsMax: 200, scheduledOnly: false}).rows),
    ).toEqual(['12501', '12502']);
    expect(crns(run({creditsMax: 100}).rows)).toEqual([
      '12950',
      '12951',
      '13455',
      '13520',
      '13521',
    ]);
  });

  it('sorts by open seats with not-polled last', () => {
    const rows = run({
      subject: ['COMP'],
      sort: 'openSeats',
      scheduledOnly: false,
    }).rows;
    expect(rows[0]?.listing.crn).toBe('12488');
    expect(crns(rows).slice(-2)).toEqual(['12501', '12502']);
  });

  it('pages with total, hasMore and the whole-match course count', () => {
    const first = run({limit: 10});
    expect(first.rows.length).toBe(10);
    expect(first.hasMore).toBe(true);
    expect(first.total).toBe(40);
    expect(first.courseCount).toBe(36);
    const last = run({limit: 10, offset: 30});
    expect(last.rows.length).toBe(10);
    expect(last.hasMore).toBe(false);
    expect(run({limit: 1000}).limit).toBe(100);
  });
});

describe('URL round trip', () => {
  it('writes only what differs from the default and reads it back', () => {
    expect(serializeCatalogUrl({query: DEFAULT_QUERY}).toString()).toBe('');
    const state = {
      query: {
        ...DEFAULT_QUERY,
        q: 'comp 140',
        subject: ['COMP'],
        attr: ['GRP3' as const],
        level: [100, 200],
        days: ['T' as const, 'R' as const],
        startsAfter: 540,
        creditsMin: 150,
        creditsMax: 300,
        partOfTerm: ['Full Term'],
        openSeatsOnly: true,
        scheduledOnly: false,
        sort: 'openSeats' as const,
      },
      term: '202710',
      crn: '12422',
    };
    const params = serializeCatalogUrl(state);
    expect(params.toString()).toBe(
      'term=202710&q=comp+140&subj=COMP&attr=GRP3&level=100%2C200&days=TR&after=540&cmin=1.5&cmax=3&pot=Full+Term&open=1&unsched=1&sort=openSeats&crn=12422',
    );
    expect(parseCatalogUrl(params)).toEqual(state);
    expect(activeFilterCount(state.query)).toBe(9);
  });

  it('never carries paging, and drops junk from a hand-edited URL', () => {
    expect(
      serializeCatalogUrl({
        query: {...DEFAULT_QUERY, offset: 50, limit: 10},
      }).toString(),
    ).toBe('');
    const {query, crn} = parseCatalogUrl(
      new URLSearchParams(
        'attr=GRP9,ad&level=150,300&days=xMz&after=9999&cmin=x&crn=12',
      ),
    );
    expect(query.attr).toEqual(['AD']);
    expect(query.level).toEqual([300]);
    expect(query.days).toEqual(['M']);
    expect(query.startsAfter).toBeUndefined();
    expect(query.creditsMin).toBeUndefined();
    expect(crn).toBeUndefined();
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
