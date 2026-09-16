/**
 * The path and method table of `skyspace-api`, one function per route the
 * browser uses (`06-api.md`, gap report §1). Thin: generated wire types in,
 * generated wire types out. `adapt/` turns them into domain types.
 */
import {apiFetch, type Query} from './http';
import type {AccountView} from './generated/AccountView';
import type {AuthMethods} from './generated/AuthMethods';
import type {CatalogYear} from './generated/CatalogYear';
import type {ClaimRequest} from './generated/ClaimRequest';
import type {ClaimResult} from './generated/ClaimResult';
import type {Collection} from './generated/Collection';
import type {CollectionId} from './generated/CollectionId';
import type {CollectionWrite} from './generated/CollectionWrite';
import type {CourseView} from './generated/CourseView';
import type {Crn} from './generated/Crn';
import type {EmailRequest} from './generated/EmailRequest';
import type {EmailVerify} from './generated/EmailVerify';
import type {EventBody} from './generated/EventBody';
import type {Fresh} from './generated/Fresh';
import type {HealthBody} from './generated/HealthBody';
import type {MetaBody} from './generated/MetaBody';
import type {Plan} from './generated/Plan';
import type {PlanBundle} from './generated/PlanBundle';
import type {PlanCreate} from './generated/PlanCreate';
import type {PlanEnvelope} from './generated/PlanEnvelope';
import type {PlanId} from './generated/PlanId';
import type {PlanSummary} from './generated/PlanSummary';
import type {PlanWrite} from './generated/PlanWrite';
import type {Program} from './generated/Program';
import type {ProgramId} from './generated/ProgramId';
import type {ProgramSummary} from './generated/ProgramSummary';
import type {ReferenceBody} from './generated/ReferenceBody';
import type {Report} from './generated/Report';
import type {RequirementErrorReport} from './generated/RequirementErrorReport';
import type {ScheduleEnvelope} from './generated/ScheduleEnvelope';
import type {ScheduleId} from './generated/ScheduleId';
import type {ScheduleSummary} from './generated/ScheduleSummary';
import type {ScheduleWrite} from './generated/ScheduleWrite';
import type {SeatRow} from './generated/SeatRow';
import type {Section} from './generated/Section';
import type {SectionPage} from './generated/SectionPage';
import type {TermCode} from './generated/TermCode';
import type {TermSchedule} from './generated/TermSchedule';

const seg = encodeURIComponent;

// ------------------------------------------------------------ meta / health

export function getHealth(signal?: AbortSignal): Promise<HealthBody> {
  return apiFetch<HealthBody>('/health', {signal});
}

/** Bare, not `Fresh`: `currentTerm` is `null` until a person sets one. */
export function getMeta(signal?: AbortSignal): Promise<MetaBody> {
  return apiFetch<MetaBody>('/meta', {signal});
}

// ----------------------------------------------------------------- catalog

export function getReference(
  term: TermCode,
  signal?: AbortSignal,
): Promise<Fresh<ReferenceBody>> {
  return apiFetch<Fresh<ReferenceBody>>('/reference', {
    query: {term},
    signal,
  });
}

/** `query` is the encoded `CatalogQuery` (`adapt/section.ts` `catalogQueryToWire`), `term` included. */
export function getSections(
  query: Query,
  signal?: AbortSignal,
): Promise<Fresh<SectionPage>> {
  return apiFetch<Fresh<SectionPage>>('/sections', {query, signal});
}

/** The path segment is the number: a non-numeric one is a `400` before the handler runs. */
export function getSection(
  term: TermCode,
  crn: Crn,
  signal?: AbortSignal,
): Promise<Fresh<Section>> {
  return apiFetch<Fresh<Section>>(`/sections/${seg(String(crn))}`, {
    query: {term},
    signal,
  });
}

export function getCourse(
  term: TermCode,
  subject: string,
  number: string,
  signal?: AbortSignal,
): Promise<Fresh<CourseView>> {
  return apiFetch<Fresh<CourseView>>(
    `/courses/${seg(subject)}/${seg(number)}`,
    {query: {term}, signal},
  );
}

/** 1 to 50 codes (`ELEC 303`, `ELEC+303` or `ELEC303`); unknown codes are absent, and the order is the server's. */
export function getCourses(
  term: TermCode,
  codes: readonly string[],
  signal?: AbortSignal,
): Promise<Fresh<CourseView[]>> {
  return apiFetch<Fresh<CourseView[]>>('/courses', {
    query: {term, code: codes},
    signal,
  });
}

/** Up to 200 CRNs; zero is legal and returns `[]`. `seats: null` means outside the poll set. */
export function getSeats(
  term: TermCode,
  crns: readonly Crn[],
  signal?: AbortSignal,
): Promise<Fresh<SeatRow[]>> {
  return apiFetch<Fresh<SeatRow[]>>('/seats', {
    query: {term, crn: crns},
    signal,
  });
}

// ------------------------------------------------------- programs / reports

export type ListProgramsParams = {
  /** Defaults to the current term's academic year server-side. */
  catalogYear?: CatalogYear;
  credential?: string;
  q?: string;
};

/** Bare `ProgramSummary[]`, not `Fresh`. */
export function listPrograms(
  params: ListProgramsParams = {},
  signal?: AbortSignal,
): Promise<ProgramSummary[]> {
  return apiFetch<ProgramSummary[]>('/programs', {
    query: {
      catalogYear: params.catalogYear,
      credential: params.credential,
      q: params.q,
    },
    signal,
  });
}

export function getProgram(
  id: ProgramId,
  catalogYear?: CatalogYear,
  signal?: AbortSignal,
): Promise<Fresh<Program>> {
  return apiFetch<Fresh<Program>>(`/programs/${seg(id)}`, {
    query: {catalogYear},
    signal,
  });
}

/** `202` with no body. All four keys are required on the wire: send explicit `null`s. */
export function postRequirementReport(
  body: RequirementErrorReport,
  signal?: AbortSignal,
): Promise<void> {
  return apiFetch<void>('/reports/rule', {method: 'POST', body, signal});
}

// --------------------------------------------------------------- schedules

export const schedules = {
  /** Summaries only: the documents take one `get` each. */
  list(term: TermCode, signal?: AbortSignal): Promise<ScheduleSummary[]> {
    return apiFetch<ScheduleSummary[]>('/schedules', {query: {term}, signal});
  },
  /** The store mints a new id and returns it in the envelope; the one sent is discarded. */
  create(
    schedule: TermSchedule,
    signal?: AbortSignal,
  ): Promise<ScheduleEnvelope> {
    const body: ScheduleWrite = {version: null, schedule};
    return apiFetch<ScheduleEnvelope>('/schedules', {
      method: 'POST',
      body,
      signal,
    });
  },
  get(id: ScheduleId, signal?: AbortSignal): Promise<ScheduleEnvelope> {
    return apiFetch<ScheduleEnvelope>(`/schedules/${seg(id)}`, {signal});
  },
  /** `409 stale_version` when `version` is behind the store. */
  save(
    id: ScheduleId,
    version: number,
    schedule: TermSchedule,
    signal?: AbortSignal,
  ): Promise<ScheduleEnvelope> {
    const body: ScheduleWrite = {version, schedule};
    return apiFetch<ScheduleEnvelope>(`/schedules/${seg(id)}`, {
      method: 'PUT',
      body,
      signal,
    });
  },
  remove(id: ScheduleId, signal?: AbortSignal): Promise<void> {
    return apiFetch<void>(`/schedules/${seg(id)}`, {
      method: 'DELETE',
      signal,
    });
  },
};

// ------------------------------------------------------------------- plans

export const plans = {
  /** Active plan first. */
  list(signal?: AbortSignal): Promise<PlanSummary[]> {
    return apiFetch<PlanSummary[]>('/plans', {signal});
  },
  /** `plan.id` is discarded; the envelope carries the minted one. The first plan becomes active. */
  create(plan: Plan, signal?: AbortSignal): Promise<PlanEnvelope> {
    const body: PlanCreate = {plan};
    return apiFetch<PlanEnvelope>('/plans', {method: 'POST', body, signal});
  },
  get(id: PlanId, signal?: AbortSignal): Promise<PlanEnvelope> {
    return apiFetch<PlanEnvelope>(`/plans/${seg(id)}`, {signal});
  },
  /** `409 stale_version` when `version` is behind the store. */
  save(
    id: PlanId,
    version: number,
    plan: Plan,
    signal?: AbortSignal,
  ): Promise<PlanEnvelope> {
    const body: PlanWrite = {version, plan};
    return apiFetch<PlanEnvelope>(`/plans/${seg(id)}`, {
      method: 'PUT',
      body,
      signal,
    });
  },
  /** `409 last_plan` when it is the only one. */
  remove(id: PlanId, signal?: AbortSignal): Promise<void> {
    return apiFetch<void>(`/plans/${seg(id)}`, {method: 'DELETE', signal});
  },
  /** Make it the plan the board opens on; only an account's first plan is active by itself. */
  activate(id: PlanId, signal?: AbortSignal): Promise<void> {
    return apiFetch<void>(`/plans/${seg(id)}/activate`, {
      method: 'POST',
      signal,
    });
  },
  /** Bare, not `Fresh`, and carries no version. */
  bundle(id: PlanId, signal?: AbortSignal): Promise<PlanBundle> {
    return apiFetch<PlanBundle>(`/plans/${seg(id)}/bundle`, {signal});
  },
  report(id: PlanId, signal?: AbortSignal): Promise<Report> {
    return apiFetch<Report>(`/plans/${seg(id)}/report`, {signal});
  },
};

// ------------------------------------------------------------- collections

export const collections = {
  list(signal?: AbortSignal): Promise<Collection[]> {
    return apiFetch<Collection[]>('/collections', {signal});
  },
  /** `409 duplicate_name`. */
  create(name: string, signal?: AbortSignal): Promise<Collection> {
    const body: CollectionWrite = {name};
    return apiFetch<Collection>('/collections', {
      method: 'POST',
      body,
      signal,
    });
  },
  rename(
    id: CollectionId,
    name: string,
    signal?: AbortSignal,
  ): Promise<Collection> {
    const body: CollectionWrite = {name};
    return apiFetch<Collection>(`/collections/${seg(id)}`, {
      method: 'PATCH',
      body,
      signal,
    });
  },
  remove(id: CollectionId, signal?: AbortSignal): Promise<void> {
    return apiFetch<void>(`/collections/${seg(id)}`, {
      method: 'DELETE',
      signal,
    });
  },
  /** Idempotent; no body. */
  addCourse(
    id: CollectionId,
    subject: string,
    number: string,
    signal?: AbortSignal,
  ): Promise<void> {
    return apiFetch<void>(
      `/collections/${seg(id)}/courses/${seg(subject)}/${seg(number)}`,
      {method: 'PUT', signal},
    );
  },
  removeCourse(
    id: CollectionId,
    subject: string,
    number: string,
    signal?: AbortSignal,
  ): Promise<void> {
    return apiFetch<void>(
      `/collections/${seg(id)}/courses/${seg(subject)}/${seg(number)}`,
      {method: 'DELETE', signal},
    );
  },
};

// -------------------------------------------------------------------- auth

export const auth = {
  methods(signal?: AbortSignal): Promise<AuthMethods> {
    return apiFetch<AuthMethods>('/auth/methods', {signal});
  },
  /** Always `202`, even for a disallowed domain; `429 rate_limited` is the only refusal. */
  requestCode(email: string, signal?: AbortSignal): Promise<void> {
    const body: EmailRequest = {email};
    return apiFetch<void>('/auth/email/request', {
      method: 'POST',
      body,
      signal,
    });
  },
  /** Sets the session cookie. `400 invalid_request "code"` for a wrong, expired or used code. */
  verifyCode(
    email: string,
    code: string,
    signal?: AbortSignal,
  ): Promise<AccountView> {
    const body: EmailVerify = {email, code};
    return apiFetch<AccountView>('/auth/email/verify', {
      method: 'POST',
      body,
      signal,
    });
  },
  /** `401` when already signed out. */
  logout(signal?: AbortSignal): Promise<void> {
    return apiFetch<void>('/auth/logout', {method: 'POST', signal});
  },
};

// ----------------------------------------------------------------- account

export const account = {
  /** `401` is the signed-out probe: the cookie is `HttpOnly`, so this is how the browser asks. */
  get(signal?: AbortSignal): Promise<AccountView> {
    return apiFetch<AccountView>('/account', {signal});
  },
  /** 1 MiB body limit; over-limit items are skipped, not rejected. */
  claim(body: ClaimRequest, signal?: AbortSignal): Promise<ClaimResult> {
    return apiFetch<ClaimResult>('/account/claim', {
      method: 'POST',
      body,
      signal,
    });
  },
  /** Cascades plans, schedules, collections and sessions. */
  remove(signal?: AbortSignal): Promise<void> {
    return apiFetch<void>('/account', {method: 'DELETE', signal});
  },
};

// ------------------------------------------------------------------ events

/** All three keys are required on the wire: `term` and `empty` take explicit `null`. */
export function postEvent(
  body: EventBody,
  signal?: AbortSignal,
): Promise<void> {
  return apiFetch<void>('/events', {method: 'POST', body, signal});
}
