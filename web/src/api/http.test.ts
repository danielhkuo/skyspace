import {afterEach, describe, expect, it, vi} from 'vitest';

import type {ErrorBody} from './generated/ErrorBody';
import {
  ApiError,
  apiFetch,
  isApiError,
  toSearchParams,
  unwrapFresh,
} from './http';

type Call = {url: string; init: RequestInit};

/** Stubs `fetch` with one canned answer and records what was asked. */
function stubFetch(
  status: number,
  body: string | null,
  headers: Record<string, string> = {},
): Call[] {
  const calls: Call[] = [];
  vi.stubGlobal('fetch', (url: string, init: RequestInit) => {
    calls.push({url, init});
    return Promise.resolve(new Response(body, {status, headers}));
  });
  return calls;
}

function headerOf(call: Call | undefined, name: string): string | null {
  return new Headers(call?.init.headers).get(name);
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('toSearchParams', () => {
  it('repeats a key per array item and never joins with commas', () => {
    const params = toSearchParams({
      subject: ['COMP', 'MATH'],
      crn: [12345, 12346],
    });
    expect(params.toString()).toBe(
      'subject=COMP&subject=MATH&crn=12345&crn=12346',
    );
  });

  it('skips undefined and empty arrays, and spells booleans out', () => {
    const params = toSearchParams({
      q: undefined,
      days: [],
      scheduledOnly: false,
      openSeatsOnly: true,
      offset: 0,
    });
    expect(params.toString()).toBe(
      'scheduledOnly=false&openSeatsOnly=true&offset=0',
    );
  });
});

describe('apiFetch', () => {
  it('prefixes /api/v1, sends Accept and the cookie, and parses a 200', async () => {
    const calls = stubFetch(200, JSON.stringify({ok: true}), {
      'Content-Type': 'application/json',
    });
    const body = await apiFetch<{ok: boolean}>('/meta');
    expect(body).toEqual({ok: true});
    expect(calls[0]?.url).toBe('/api/v1/meta');
    expect(calls[0]?.init.method).toBe('GET');
    expect(calls[0]?.init.credentials).toBe('same-origin');
    expect(headerOf(calls[0], 'Accept')).toBe('application/json');
    expect(headerOf(calls[0], 'Content-Type')).toBeNull();
    expect(headerOf(calls[0], 'If-None-Match')).toBeNull();
  });

  it('leaves /health outside the base path and encodes the query', async () => {
    const calls = stubFetch(200, '{}');
    await apiFetch('/health');
    await apiFetch('/sections', {query: {term: '202710', subject: ['COMP']}});
    expect(calls[0]?.url).toBe('/health');
    expect(calls[1]?.url).toBe('/api/v1/sections?term=202710&subject=COMP');
  });

  it('resolves undefined for a 204', async () => {
    stubFetch(204, null);
    await expect(apiFetch<void>('/plans/x', {method: 'DELETE'})).resolves.toBe(
      undefined,
    );
  });

  it('resolves undefined for a 202', async () => {
    stubFetch(202, null);
    await expect(
      apiFetch<void>('/reports/rule', {method: 'POST', body: {message: 'x'}}),
    ).resolves.toBe(undefined);
  });

  it('sends Content-Type: application/json on a bodyless DELETE', async () => {
    const calls = stubFetch(204, null);
    await apiFetch('/schedules/abc', {method: 'DELETE'});
    expect(headerOf(calls[0], 'Content-Type')).toBe('application/json');
    expect(calls[0]?.init.body).toBeUndefined();
  });

  it('JSON-encodes a body with Content-Type on a POST', async () => {
    const calls = stubFetch(200, '{}');
    await apiFetch('/plans', {method: 'POST', body: {plan: {id: 'p'}}});
    expect(calls[0]?.init.body).toBe('{"plan":{"id":"p"}}');
    expect(headerOf(calls[0], 'Content-Type')).toBe('application/json');
  });

  it('throws an ApiError built from the ErrorBody on a 409', async () => {
    const wire: ErrorBody = {
      code: 'stale_version',
      message: 'version',
      requestId: 'req-1',
      retryAfterSeconds: null,
    };
    stubFetch(409, JSON.stringify(wire), {
      'Content-Type': 'application/json',
    });
    const err = await apiFetch('/plans/p', {
      method: 'PUT',
      body: {},
    }).catch((e: unknown) => e);
    expect(isApiError(err)).toBe(true);
    expect(isApiError(err, 'stale_version')).toBe(true);
    expect(isApiError(err, 'not_found')).toBe(false);
    const apiErr = err as ApiError;
    expect(apiErr.status).toBe(409);
    expect(apiErr.message).toBe('version');
    expect(apiErr.requestId).toBe('req-1');
    expect(apiErr.retryAfterSeconds).toBeUndefined();
  });

  it('falls back to code internal for a non-JSON 502', async () => {
    stubFetch(502, '<html>Bad Gateway</html>', {'Content-Type': 'text/html'});
    const err = await apiFetch('/meta').catch((e: unknown) => e);
    expect(isApiError(err, 'internal')).toBe(true);
    expect((err as ApiError).status).toBe(502);
  });

  it('reads Retry-After on a 429', async () => {
    const wire: ErrorBody = {
      code: 'rate_limited',
      message: 'slow down',
      requestId: 'req-2',
      retryAfterSeconds: 3600,
    };
    stubFetch(429, JSON.stringify(wire), {'Retry-After': '3600'});
    const err = await apiFetch('/auth/email/request', {
      method: 'POST',
      body: {email: 'a@rice.edu'},
    }).catch((e: unknown) => e);
    expect((err as ApiError).retryAfterSeconds).toBe(3600);
  });

  it('is not fooled by a non-ApiError', () => {
    expect(isApiError(new Error('x'))).toBe(false);
    expect(isApiError(undefined)).toBe(false);
  });
});

describe('unwrapFresh', () => {
  it('returns the data and drops the freshness', () => {
    expect(
      unwrapFresh({
        data: [1, 2],
        freshness: {
          source: 'seats',
          riceAsOf: null,
          pulledAt: '2026-09-11T00:00:00Z',
          stale: false,
        },
      }),
    ).toEqual([1, 2]);
  });
});
