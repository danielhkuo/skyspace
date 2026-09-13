import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';

import type {Plan, PlanId, ScheduleId, TermSchedule} from '../domain';
import {DEFAULT_QUERY} from '../domain';
import {GUEST_KEY} from './guestStore';

type Recorded = {method: string; url: string; body: unknown};

type Route = {
  method: string;
  path: string;
  status: number;
  body?: unknown;
};

/** vitest runs in node: a tiny in-memory `localStorage` stands in for the browser's. */
function memoryStorage(): Storage {
  const map = new Map<string, string>();
  return {
    get length() {
      return map.size;
    },
    key: (i: number) => [...map.keys()][i] ?? null,
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, String(v)),
    removeItem: (k: string) => void map.delete(k),
    clear: () => map.clear(),
  };
}

const requests: Recorded[] = [];
let routes: Route[] = [];

/** First matching route wins; anything else is a 500 the test will notice. */
function stubFetch(): void {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: string, init?: RequestInit) => {
      const method = init?.method ?? 'GET';
      const path = input.split('?')[0] ?? input;
      const body =
        typeof init?.body === 'string' ? JSON.parse(init.body) : undefined;
      requests.push({method, url: input, body});
      const route = routes.find(r => r.method === method && r.path === path);
      if (route === undefined) {
        return new Response(
          JSON.stringify({code: 'internal', message: `unrouted ${input}`}),
          {status: 500},
        );
      }
      return route.body === undefined
        ? new Response(null, {status: route.status})
        : new Response(JSON.stringify(route.body), {
            status: route.status,
            headers: {'Content-Type': 'application/json'},
          });
    }),
  );
}

const unauthenticated = (path: string): Route => ({
  method: 'GET',
  path,
  status: 401,
  body: {code: 'unauthenticated', message: 'Sign in', requestId: 'r'},
});

const ACCOUNT = {id: 'a1', email: 'jw12@rice.edu', createdAt: '2026-01-01'};

const signedIn = (): Route => ({
  method: 'GET',
  path: '/api/v1/account',
  status: 200,
  body: ACCOUNT,
});

const of = (method: string, path: string) =>
  requests.filter(r => r.method === method && r.url.startsWith(path));

/** Module-level caches live in `api.ts`, so every test loads a fresh copy. */
async function load() {
  vi.resetModules();
  return (await import('./api')).apiDataSource;
}

const schedule: TermSchedule = {
  id: 'guest-1' as ScheduleId,
  name: 'Fall draft',
  term: '202710',
  candidates: [
    {
      course: {subject: 'COMP', number: '140'},
      sections: ['12345'],
      visible: true,
      colour: 3,
    },
  ],
  busy: [],
};

const plan: Plan = {
  id: 'p1' as PlanId,
  name: 'My plan',
  catalogYear: 2026,
  matriculation: {academicYear: 2027, season: 'fall'},
  programs: [],
  incomingCredit: [],
  terms: [],
  selfChecks: [],
};

beforeEach(() => {
  requests.length = 0;
  routes = [];
  vi.stubGlobal('localStorage', memoryStorage());
  stubFetch();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('sign-in', () => {
  it('requests a code, verifies it, and then reports the session without probing again', async () => {
    routes = [
      unauthenticated('/api/v1/account'),
      {method: 'POST', path: '/api/v1/auth/email/request', status: 202},
      {
        method: 'POST',
        path: '/api/v1/auth/email/verify',
        status: 200,
        body: ACCOUNT,
      },
    ];
    const source = await load();

    expect(await source.session()).toBeUndefined();
    await source.requestSignInCode('jw12@rice.edu');
    expect(of('POST', '/api/v1/auth/email/request')[0]?.body).toEqual({
      email: 'jw12@rice.edu',
    });

    const session = await source.verifySignInCode('jw12@rice.edu', '123456');
    expect(session).toEqual({email: 'jw12@rice.edu', name: 'jw12'});
    expect(of('POST', '/api/v1/auth/email/verify')[0]?.body).toEqual({
      email: 'jw12@rice.edu',
      code: '123456',
    });

    const probes = of('GET', '/api/v1/account').length;
    expect(await source.session()).toEqual(session);
    expect(of('GET', '/api/v1/account')).toHaveLength(probes);
  });
});

describe('guest schedules', () => {
  it('round-trips through the guest store and clears it on a claim', async () => {
    routes = [
      unauthenticated('/api/v1/account'),
      {
        method: 'POST',
        path: '/api/v1/auth/email/verify',
        status: 200,
        body: ACCOUNT,
      },
      {
        method: 'POST',
        path: '/api/v1/account/claim',
        status: 200,
        body: {
          schedules: [{clientId: 'guest-1', id: 's-9', renamedTo: null}],
          collections: [],
          skipped: [],
        },
      },
    ];
    const source = await load();

    expect(await source.saveSchedule(schedule)).toEqual(schedule);
    await source.setCurrentSchedule('202710', schedule.id);
    expect(await source.loadSchedules('202710')).toEqual({
      schedules: [schedule],
      current: schedule.id,
    });
    expect(await source.loadSchedules('202720')).toEqual({schedules: []});
    expect(of('GET', '/api/v1/schedules')).toHaveLength(0);
    expect(localStorage.getItem(GUEST_KEY)).not.toBeNull();

    await source.verifySignInCode('jw12@rice.edu', '123456');
    await source.claimGuestData({
      schedules: [schedule],
      collections: [
        {name: 'Favorites', courses: [{subject: 'COMP', number: '140'}]},
      ],
    });

    const claim = of('POST', '/api/v1/account/claim')[0]?.body as {
      schedules: {clientId: string; schedule: {id: string}}[];
      collections: {clientId: string; name: string}[];
    };
    expect(claim.schedules).toHaveLength(1);
    expect(claim.schedules[0]?.clientId).toBe('guest-1');
    expect(claim.schedules[0]?.schedule.id).toBe('guest-1');
    expect(claim.collections[0]?.name).toBe('Favorites');
    expect(claim.collections[0]?.clientId).toMatch(/[0-9a-f-]{36}/);
    expect(localStorage.getItem(GUEST_KEY)).toBeNull();
  });
});

describe('signed-in schedules', () => {
  it('creates with POST, keeps the minted id, and PUTs with the version it was given', async () => {
    const minted = {...schedule, id: 's-9'};
    routes = [
      signedIn(),
      {
        method: 'POST',
        path: '/api/v1/schedules',
        status: 200,
        body: {id: 's-9', version: 1, updatedAt: 'now', schedule: minted},
      },
      {
        method: 'PUT',
        path: '/api/v1/schedules/s-9',
        status: 200,
        body: {id: 's-9', version: 2, updatedAt: 'now', schedule: minted},
      },
    ];
    const source = await load();

    const saved = await source.saveSchedule(schedule);
    expect(saved.id).toBe('s-9');
    expect(of('POST', '/api/v1/schedules')).toHaveLength(1);
    expect(localStorage.getItem(GUEST_KEY)).toBeNull();

    await source.saveSchedule({...saved, name: 'Renamed'});
    const put = of('PUT', '/api/v1/schedules/s-9');
    expect(put).toHaveLength(1);
    expect(put[0]?.body).toMatchObject({
      version: 1,
      schedule: {id: 's-9', name: 'Renamed'},
    });
  });
});

describe('plans', () => {
  it('turns a 409 on save into StaleVersionError', async () => {
    routes = [
      signedIn(),
      {
        method: 'GET',
        path: '/api/v1/plans/p1',
        status: 200,
        body: {version: 3, updatedAt: 'now', plan: {}},
      },
      {
        method: 'PUT',
        path: '/api/v1/plans/p1',
        status: 409,
        body: {code: 'stale_version', message: 'stale', requestId: 'r'},
      },
    ];
    const source = await load();

    // `load()` resets modules, so the class identity differs; match by name.
    await expect(source.savePlan(plan)).rejects.toMatchObject({
      name: 'StaleVersionError',
    });
    expect(of('PUT', '/api/v1/plans/p1')[0]?.body).toMatchObject({
      version: 3,
      plan: {id: 'p1', name: 'My plan'},
    });
  });
});

describe('favorites', () => {
  it('diffs against the last load: one PUT and one DELETE', async () => {
    routes = [
      signedIn(),
      {
        method: 'GET',
        path: '/api/v1/collections',
        status: 200,
        body: [
          {
            id: 'c1',
            name: 'Favorites',
            courses: [
              {subject: 'COMP', number: '140'},
              {subject: 'MATH', number: '101'},
            ],
          },
        ],
      },
      {
        method: 'PUT',
        path: '/api/v1/collections/c1/courses/STAT/310',
        status: 204,
      },
      {
        method: 'DELETE',
        path: '/api/v1/collections/c1/courses/MATH/101',
        status: 204,
      },
    ];
    const source = await load();

    expect(await source.loadFavorites()).toEqual([
      {subject: 'COMP', number: '140'},
      {subject: 'MATH', number: '101'},
    ]);
    await source.saveFavorites([
      {subject: 'COMP', number: '140'},
      {subject: 'STAT', number: '310'},
    ]);

    expect(of('PUT', '/api/v1/collections')).toHaveLength(1);
    expect(of('DELETE', '/api/v1/collections')).toHaveLength(1);
    expect(of('POST', '/api/v1/collections')).toHaveLength(0);
  });
});

describe('catalog', () => {
  it('always sends scheduledOnly and repeats days', async () => {
    routes = [
      {
        method: 'GET',
        path: '/api/v1/sections',
        status: 200,
        body: {
          data: {
            rows: [],
            total: 0,
            offset: 0,
            limit: 25,
            hasMore: false,
            unscheduledHidden: 0,
            courseCount: 0,
            applied: {},
            suggestions: [],
          },
          freshness: {
            source: 'listing',
            riceAsOf: null,
            pulledAt: 'now',
            stale: false,
          },
        },
      },
    ];
    const source = await load();

    const page = await source.searchSections('202710', {
      ...DEFAULT_QUERY,
      days: ['M', 'W'],
    });
    expect(page.rows).toEqual([]);
    const url = of('GET', '/api/v1/sections')[0]?.url ?? '';
    const params = new URLSearchParams(url.split('?')[1]);
    expect(params.get('term')).toBe('202710');
    expect(params.get('scheduledOnly')).toBe('true');
    expect(params.getAll('days')).toEqual(['mon', 'wed']);
  });
});
