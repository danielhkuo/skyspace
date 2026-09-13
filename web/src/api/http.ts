/**
 * The one `fetch` wrapper every endpoint goes through (`06-api.md`). It knows
 * the base path, the session cookie, the CSRF guard's demands, and the one
 * error shape; it knows nothing about any route.
 */
import type {ErrorBody} from './generated/ErrorBody';
import type {ErrorCode} from './generated/ErrorCode';
import type {Fresh} from './generated/Fresh';
import type {Freshness} from './generated/Freshness';

export type {Fresh, Freshness};

const BASE_PATH = '/api/v1';

/** A `>= 400` answer, keyed on `code`: the message is prose for people, never for switches. */
export class ApiError extends Error {
  readonly status: number;
  readonly code: ErrorCode;
  readonly requestId?: string;
  readonly retryAfterSeconds?: number;

  constructor(
    status: number,
    code: ErrorCode,
    message: string,
    details: {requestId?: string; retryAfterSeconds?: number} = {},
  ) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
    if (details.requestId !== undefined) {
      this.requestId = details.requestId;
    }
    if (details.retryAfterSeconds !== undefined) {
      this.retryAfterSeconds = details.retryAfterSeconds;
    }
  }
}

export function isApiError(e: unknown, code?: ErrorCode): e is ApiError {
  return e instanceof ApiError && (code === undefined || e.code === code);
}

export type Query = Record<
  string,
  string | number | boolean | undefined | readonly (string | number)[]
>;

/**
 * Repeated keys for arrays, never comma lists: the server parses
 * `subject=COMP&subject=MATH` and rejects `subject=COMP,MATH` (gap report §0).
 */
export function toSearchParams(q: Query): URLSearchParams {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(q)) {
    if (value === undefined) {
      continue;
    }
    if (Array.isArray(value)) {
      for (const item of value as readonly (string | number)[]) {
        params.append(key, String(item));
      }
      continue;
    }
    params.append(key, String(value));
  }
  return params;
}

export type Method = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';

export type FetchInit = {
  method?: Method;
  query?: Query;
  body?: unknown;
  signal?: AbortSignal;
};

/** `/health` is the only route outside `/api/v1`. */
function resolvePath(path: string, query: Query | undefined): string {
  const prefixed = path.startsWith('/health') ? path : `${BASE_PATH}${path}`;
  if (query === undefined) {
    return prefixed;
  }
  const search = toSearchParams(query).toString();
  return search === '' ? prefixed : `${prefixed}?${search}`;
}

function buildHeaders(method: Method): Headers {
  const headers = new Headers({Accept: 'application/json'});
  // The CSRF guard wants `application/json` on every mutation, even a
  // bodyless DELETE (gap report §0), so the header does not follow the body.
  if (method !== 'GET') {
    headers.set('Content-Type', 'application/json');
  }
  return headers;
}

function parseRetryAfter(res: Response): number | undefined {
  const header = res.headers.get('Retry-After');
  if (header === null) {
    return undefined;
  }
  const seconds = Number(header);
  return Number.isFinite(seconds) ? seconds : undefined;
}

function isErrorBody(value: unknown): value is ErrorBody {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as {code?: unknown}).code === 'string' &&
    typeof (value as {message?: unknown}).message === 'string'
  );
}

/** A non-JSON answer (a proxy's 502 page, say) still becomes an `ApiError`, coded `internal`. */
async function toApiError(res: Response): Promise<ApiError> {
  const retryAfterSeconds = parseRetryAfter(res);
  let body: unknown;
  try {
    body = await res.json();
  } catch {
    body = undefined;
  }
  if (!isErrorBody(body)) {
    return new ApiError(res.status, 'internal', `HTTP ${res.status}`, {
      retryAfterSeconds,
    });
  }
  return new ApiError(res.status, body.code, body.message, {
    requestId: body.requestId,
    retryAfterSeconds: body.retryAfterSeconds ?? retryAfterSeconds,
  });
}

/**
 * `credentials: 'same-origin'` because the session is a cookie and there is
 * no CORS layer. Never sends `If-None-Match`: a `304` has no body, and the
 * browser cache honours `Cache-Control` on its own.
 */
export async function apiFetch<T>(
  path: string,
  init: FetchInit = {},
): Promise<T> {
  const method = init.method ?? 'GET';
  const request: RequestInit = {
    method,
    headers: buildHeaders(method),
    credentials: 'same-origin',
  };
  if (init.body !== undefined) {
    request.body = JSON.stringify(init.body);
  }
  if (init.signal !== undefined) {
    request.signal = init.signal;
  }
  const res = await fetch(resolvePath(path, init.query), request);
  if (res.status >= 400) {
    throw await toApiError(res);
  }
  if (res.status === 204 || res.status === 202) {
    return undefined as T;
  }
  return (await res.json()) as T;
}

export function unwrapFresh<T>(fresh: Fresh<T>): T {
  return fresh.data;
}
