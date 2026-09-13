/**
 * API data source: `skyspace-api` on the same origin (`06-api.md`), with the
 * gap report's method mapping followed route for route. Wire types stop at
 * this file: `../api/endpoints` speaks them, `../api/adapt` translates, and
 * nothing below exports one. A guest's schedules and favorites live in
 * `guestStore` until a claim moves them onto the account.
 */
import {
  bundleFromWire,
  catalogQueryToWire,
  courseInfoFromWire,
  courseViewSections,
  crnToWire,
  planFromWire,
  planToWire,
  programSummaryFromWire,
  scheduleFromWire,
  scheduleToWire,
  sectionFromWire,
  sectionPageFromWire,
} from '../api/adapt';
import {
  account,
  auth,
  collections,
  getCourse,
  getCourses,
  getMeta,
  getReference,
  getSection,
  getSections,
  listPrograms,
  plans,
  postRequirementReport,
  schedules,
} from '../api/endpoints';
import type {Collection} from '../api/generated/Collection';
import type {MetaBody} from '../api/generated/MetaBody';
import type {ReferenceBody} from '../api/generated/ReferenceBody';
import type {Section as WireSection} from '../api/generated/Section';
import {isApiError} from '../api/http';
import {
  DEFAULT_QUERY,
  MAX_LIMIT,
  courseKey,
  formatCourseCode,
  parseCourseCode,
  sameCourse,
  type CatalogQuery,
  type CourseCode,
  type CourseInfo,
  type Crn,
  type Plan,
  type PlanBundle,
  type ProgramSummary,
  type ScheduleId,
  type Section,
  type SectionPage,
  type TermCode,
  type TermSchedule,
} from '../domain';
import {
  clearGuest,
  deleteGuestSchedule,
  guestFavorites,
  guestIsEmpty,
  guestSchedules,
  readJson,
  removeKey,
  saveGuestFavorites,
  saveGuestSchedule,
  setGuestCurrentSchedule,
  writeJson,
} from './guestStore';
import {
  InvalidCodeError,
  RateLimitedError,
  StaleVersionError,
  UnauthenticatedError,
  type DataSource,
  type GuestData,
  type RequirementErrorReport,
  type Session,
  NoPlanError,
} from './types';

/**
 * A signed-in account that holds no plan yet. Not `UnauthenticatedError`:
 * signing in again would not help; the page should route to onboarding.
 */
/** The API has no "current schedule"; the choice stays in this browser, per term. */
const CURRENT_SCHEDULE_KEY = 'skyspace.api.current-schedule.v1.';
const FAVORITES_COLLECTION = 'Favorites';
const META_TTL_MS = 60_000;

// ------------------------------------------------------------------ caches

/** The signed-in probe, memoised until sign-in, sign-out, claim or any 401. */
let sessionCache: Promise<Session | undefined> | undefined;
/** Write versions the store handed out, so a PUT can name the one it saw. */
const scheduleVersions = new Map<string, number>();
const planVersions = new Map<string, number>();
let metaCache: {at: number; value: Promise<MetaBody>} | undefined;
const referenceCache = new Map<TermCode, Promise<ReferenceBody>>();
/** The `Favorites` collection as last loaded, so `saveFavorites` can diff against it. */
let favoritesCache: {id: Collection['id']; courses: CourseCode[]} | undefined;

function forgetSession(): void {
  sessionCache = undefined;
  favoritesCache = undefined;
}

function forgetEverything(): void {
  forgetSession();
  scheduleVersions.clear();
  planVersions.clear();
  metaCache = undefined;
  referenceCache.clear();
}

// ----------------------------------------------------------------- session

/** "jw12@rice.edu" -> "jw12": the API stores no display name. */
function nameFromEmail(email: string): string {
  return email.split('@')[0] ?? email;
}

async function probeSession(): Promise<Session | undefined> {
  try {
    const view = await account.get();
    return {email: view.email, name: nameFromEmail(view.email)};
  } catch (e) {
    if (isApiError(e) && (e.status === 401 || e.status === 404)) {
      return undefined;
    }
    throw e;
  }
}

function session(): Promise<Session | undefined> {
  if (sessionCache === undefined) {
    const probe = probeSession();
    sessionCache = probe;
    // A network failure must not pin "signed out" for the rest of the page.
    probe.catch(() => {
      if (sessionCache === probe) {
        sessionCache = undefined;
      }
    });
  }
  return sessionCache;
}

async function signedIn(): Promise<boolean> {
  return (await session()) !== undefined;
}

/**
 * A session route: `401 unauthenticated` becomes the seam's error and
 * drops the memoised session, since the cookie evidently expired.
 */
async function onSession<T>(what: string, run: () => Promise<T>): Promise<T> {
  try {
    return await run();
  } catch (e) {
    if (isApiError(e, 'unauthenticated')) {
      forgetSession();
      throw new UnauthenticatedError(what);
    }
    throw e;
  }
}

function rateLimited(e: unknown): RateLimitedError | undefined {
  return isApiError(e, 'rate_limited')
    ? new RateLimitedError(e.retryAfterSeconds)
    : undefined;
}

// -------------------------------------------------------------------- meta

function meta(): Promise<MetaBody> {
  const now = Date.now();
  if (metaCache === undefined || now - metaCache.at > META_TTL_MS) {
    const value = getMeta();
    metaCache = {at: now, value};
    value.catch(() => {
      metaCache = undefined;
    });
  }
  return metaCache.value;
}

async function currentTerm(): Promise<{code: TermCode; label: string}> {
  const body = await meta();
  if (body.currentTerm === null) {
    throw new Error('no current term');
  }
  const code = body.currentTerm;
  const label = body.terms.find(t => t.code === code)?.label ?? code;
  return {code, label};
}

function reference(term: TermCode): Promise<ReferenceBody> {
  let cached = referenceCache.get(term);
  if (cached === undefined) {
    cached = getReference(term).then(fresh => fresh.data);
    referenceCache.set(term, cached);
    cached.catch(() => {
      referenceCache.delete(term);
    });
  }
  return cached;
}

// ----------------------------------------------------------------- catalog

function riceAsOf(fresh: {freshness: {riceAsOf: string | null}}) {
  return fresh.freshness.riceAsOf ?? undefined;
}

/** The section alone; `404` is "no such CRN this term". */
async function fetchSection(
  term: TermCode,
  crn: Crn,
): Promise<{section: WireSection; asOf: string | undefined} | undefined> {
  try {
    const fresh = await getSection(term, crnToWire(crn));
    return {section: fresh.data, asOf: riceAsOf(fresh)};
  } catch (e) {
    if (isApiError(e, 'not_found')) {
      return undefined;
    }
    throw e;
  }
}

/**
 * Notes and the mutual-exclusion sentence live on the course record, not
 * the section (gap report §2), so the pane's detail needs a second call.
 * A `404` there just means no enrichment.
 */
async function fetchCourseFor(term: TermCode, code: CourseCode) {
  try {
    const fresh = await getCourse(term, code.subject, code.number);
    return fresh.data.course;
  } catch (e) {
    if (isApiError(e, 'not_found')) {
      return undefined;
    }
    throw e;
  }
}

async function getSectionDetail(
  term: TermCode,
  crn: Crn,
): Promise<Section | undefined> {
  const found = await fetchSection(term, crn);
  if (found === undefined) {
    return undefined;
  }
  const course = await fetchCourseFor(term, found.section.listing.code);
  return sectionFromWire(found.section, course, found.asOf);
}

/** A section page folded by course, for the one caller that reads `code`, `title` and `credits`. */
function foldCourses(page: SectionPage, limit: number): CourseInfo[] {
  const byCourse = new Map<string, CourseInfo>();
  for (const {listing} of page.rows) {
    const key = courseKey(listing.code);
    if (!byCourse.has(key)) {
      byCourse.set(key, {
        code: listing.code,
        title: listing.title,
        credits: listing.credits,
        attributes: [],
        repeatable: false,
        seasonsOffered: [],
        termsObserved: 0,
        offeredNow: true,
      });
    }
  }
  return [...byCourse.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([, info]) => info)
    .slice(0, limit);
}

async function searchCourses(
  query: string,
  limit: number,
): Promise<CourseInfo[]> {
  const q = query.trim();
  if (q === '') {
    return [];
  }
  const {code: term} = await currentTerm();
  const code = parseCourseCode(q);
  if (code !== null) {
    const fresh = await getCourses(term, [formatCourseCode(code)]);
    return fresh.data.map(v => courseInfoFromWire(v.course)).slice(0, limit);
  }
  // There is no course search: `/sections?q=` searches sections, so ask
  // for several per course and fold (gap report §3 #6).
  const wire: CatalogQuery = {
    ...DEFAULT_QUERY,
    q,
    scheduledOnly: false,
    limit: Math.min(limit * 4, MAX_LIMIT),
  };
  const fresh = await getSections(catalogQueryToWire(term, wire));
  return foldCourses(sectionPageFromWire(fresh.data, riceAsOf(fresh)), limit);
}

// --------------------------------------------------------------- schedules

const isScheduleId = (v: unknown): v is ScheduleId => typeof v === 'string';

function readCurrentSchedule(term: TermCode): ScheduleId | undefined {
  return readJson(CURRENT_SCHEDULE_KEY + term, isScheduleId);
}

function forgetCurrentSchedules(): void {
  try {
    const keys = Array.from({length: localStorage.length}, (_, i) =>
      localStorage.key(i),
    );
    for (const key of keys) {
      if (key !== null && key.startsWith(CURRENT_SCHEDULE_KEY)) {
        removeKey(key);
      }
    }
  } catch {
    // Blocked storage: nothing was kept.
  }
}

async function loadAccountSchedules(
  term: TermCode,
): Promise<{schedules: TermSchedule[]; current?: ScheduleId}> {
  const summaries = await schedules.list(term);
  const envelopes = await Promise.all(summaries.map(s => schedules.get(s.id)));
  const docs = envelopes.map(env => {
    scheduleVersions.set(env.id, env.version);
    return scheduleFromWire(env.schedule);
  });
  const current = readCurrentSchedule(term);
  return docs.some(s => s.id === current)
    ? {schedules: docs, current}
    : {schedules: docs};
}

async function saveAccountSchedule(
  schedule: TermSchedule,
): Promise<TermSchedule> {
  const version = scheduleVersions.get(schedule.id);
  const wire = scheduleToWire(schedule);
  let envelope;
  if (version === undefined) {
    envelope = await schedules.create(wire);
  } else {
    try {
      envelope = await schedules.save(schedule.id, version, wire);
    } catch (e) {
      if (isApiError(e, 'stale_version')) {
        throw new StaleVersionError('This schedule');
      }
      throw e;
    }
  }
  scheduleVersions.set(envelope.id, envelope.version);
  // The store mints the id on create and writes it into the document.
  return scheduleFromWire(envelope.schedule);
}

async function deleteAccountSchedule(id: ScheduleId): Promise<void> {
  scheduleVersions.delete(id);
  try {
    await schedules.remove(id);
  } catch (e) {
    if (!isApiError(e, 'not_found')) {
      throw e;
    }
    // Already gone, which is what was asked for.
  }
}

// ---------------------------------------------------------------- favorites

/** The one collection favorites live in, created on first use. */
async function favoritesCollection(): Promise<Collection> {
  const find = (list: Collection[]) =>
    list.find(c => c.name.toLowerCase() === FAVORITES_COLLECTION.toLowerCase());
  const existing = find(await collections.list());
  if (existing !== undefined) {
    return existing;
  }
  try {
    return await collections.create(FAVORITES_COLLECTION);
  } catch (e) {
    if (!isApiError(e, 'duplicate_name')) {
      throw e;
    }
  }
  // Another tab created it between the list and the create.
  const raced = find(await collections.list());
  if (raced === undefined) {
    throw new Error('Favorites collection exists but was not listed');
  }
  return raced;
}

async function loadAccountFavorites(): Promise<CourseCode[]> {
  const collection = await favoritesCollection();
  favoritesCache = {id: collection.id, courses: collection.courses};
  return collection.courses;
}

/** Whole list in, per-course routes out: diff against the last load. */
async function saveAccountFavorites(favorites: CourseCode[]): Promise<void> {
  if (favoritesCache === undefined) {
    await loadAccountFavorites();
  }
  const before = favoritesCache;
  if (before === undefined) {
    throw new Error('Favorites collection did not load');
  }
  const added = favorites.filter(
    code => !before.courses.some(kept => sameCourse(kept, code)),
  );
  const removed = before.courses.filter(
    code => !favorites.some(kept => sameCourse(kept, code)),
  );
  await Promise.all([
    ...added.map(c => collections.addCourse(before.id, c.subject, c.number)),
    ...removed.map(c =>
      collections.removeCourse(before.id, c.subject, c.number),
    ),
  ]);
  favoritesCache = {id: before.id, courses: favorites};
}

// -------------------------------------------------------------------- plans

async function loadBundle(): Promise<PlanBundle> {
  if (!(await signedIn())) {
    throw new UnauthenticatedError('see your plan');
  }
  return onSession('see your plan', async () => {
    const summaries = await plans.list();
    const active = summaries.find(p => p.isActive) ?? summaries[0];
    if (active === undefined) {
      throw new NoPlanError();
    }
    planVersions.set(active.id, active.version);
    return bundleFromWire(await plans.bundle(active.id));
  });
}

/** The version a PUT must name; fetched when this tab never loaded the plan. */
async function planVersion(id: string): Promise<number> {
  const cached = planVersions.get(id);
  if (cached !== undefined) {
    return cached;
  }
  const envelope = await plans.get(id);
  planVersions.set(id, envelope.version);
  return envelope.version;
}

async function savePlan(plan: Plan): Promise<void> {
  await onSession('save your plan', async () => {
    const version = await planVersion(plan.id);
    try {
      const envelope = await plans.save(plan.id, version, planToWire(plan));
      planVersions.set(plan.id, envelope.version);
    } catch (e) {
      if (isApiError(e, 'stale_version')) {
        throw new StaleVersionError('Your plan');
      }
      throw e;
    }
  });
}

async function createPlan(plan: Plan): Promise<Plan> {
  return onSession('create a plan', async () => {
    const envelope = await plans.create(planToWire(plan));
    const created = planFromWire(envelope.plan);
    planVersions.set(created.id, envelope.version);
    return created;
  });
}

// ---------------------------------------------------------------- account

async function verifySignInCode(email: string, code: string): Promise<Session> {
  try {
    const view = await auth.verifyCode(email, code);
    const next = {email: view.email, name: nameFromEmail(view.email)};
    forgetSession();
    sessionCache = Promise.resolve(next);
    return next;
  } catch (e) {
    if (isApiError(e, 'invalid_request') && e.status === 400) {
      throw new InvalidCodeError();
    }
    throw rateLimited(e) ?? e;
  }
}

async function claimGuestData(guest: GuestData): Promise<void> {
  const nothing =
    guest.schedules.length === 0 && guest.collections.length === 0;
  if (nothing && guestIsEmpty()) {
    clearGuest();
    return;
  }
  await onSession('keep what you built', async () => {
    const result = await account.claim({
      schedules: guest.schedules.map(schedule => ({
        clientId: schedule.id,
        schedule: scheduleToWire(schedule),
      })),
      collections: guest.collections.map(c => ({
        clientId: crypto.randomUUID(),
        name: c.name,
        courses: c.courses,
      })),
    });
    for (const skip of result.skipped) {
      console.warn(`claim skipped ${skip.clientId}: ${skip.reason}`);
    }
    // Only a 200 reaches here; a thrown claim keeps the store for a retry.
    clearGuest();
    favoritesCache = undefined;
  });
}

// ------------------------------------------------------------------ source

export const apiDataSource: DataSource = {
  kind: 'api',

  loadBundle,
  savePlan,
  createPlan,

  async resetPlan(): Promise<void> {
    throw new Error('resetPlan is a demo-only operation');
  },

  async loadFavorites(): Promise<CourseCode[]> {
    if (!(await signedIn())) {
      return guestFavorites();
    }
    return onSession('see your favorites', loadAccountFavorites);
  },

  async saveFavorites(favorites: CourseCode[]): Promise<void> {
    if (!(await signedIn())) {
      saveGuestFavorites(favorites);
      return;
    }
    await onSession('save your favorites', () =>
      saveAccountFavorites(favorites),
    );
  },

  searchCourses,

  /** No endpoint takes a requirement's filter yet, so the strip offers nothing (gap report §3 #7). */
  async catalogCandidates(): Promise<CourseCode[]> {
    return [];
  },

  currentTerm,

  async searchSections(
    term: TermCode,
    query: CatalogQuery,
  ): Promise<SectionPage> {
    const fresh = await getSections(catalogQueryToWire(term, query));
    return sectionPageFromWire(fresh.data, riceAsOf(fresh));
  },

  getSection: getSectionDetail,

  /** `/sections` takes no CRN filter, so this is one call per CRN, without the course enrichment. */
  async getSections(term: TermCode, crns: Crn[]): Promise<Section[]> {
    const found = await Promise.all(crns.map(crn => fetchSection(term, crn)));
    return found
      .filter(f => f !== undefined)
      .map(f => sectionFromWire(f.section, undefined, f.asOf));
  },

  async courseSections(term: TermCode, code: CourseCode): Promise<Section[]> {
    try {
      const fresh = await getCourse(term, code.subject, code.number);
      return courseViewSections(fresh.data, riceAsOf(fresh));
    } catch (e) {
      if (!isApiError(e, 'not_found')) {
        throw e;
      }
    }
    // No course record yet (the catalog pull runs monthly and after the
    // listings), but the term's sections may still be held: ask by code.
    const fresh = await getSections(
      catalogQueryToWire(term, {
        ...DEFAULT_QUERY,
        q: `${code.subject} ${code.number}`,
        scheduledOnly: false,
        limit: 100,
      }),
    );
    return sectionPageFromWire(fresh.data, riceAsOf(fresh)).rows.filter(s =>
      sameCourse(s.listing.code, code),
    );
  },

  async listSubjects(term: TermCode): Promise<string[]> {
    return (await reference(term)).subjects.map(e => e.code);
  },

  async listPartsOfTerm(term: TermCode) {
    return (await reference(term)).partsOfTerm.map(({code, label}) => ({
      code,
      label,
    }));
  },

  async loadSchedules(term: TermCode) {
    if (!(await signedIn())) {
      return guestSchedules(term);
    }
    return onSession('see your schedules', () => loadAccountSchedules(term));
  },

  async saveSchedule(schedule: TermSchedule): Promise<TermSchedule> {
    if (!(await signedIn())) {
      saveGuestSchedule(schedule);
      return schedule;
    }
    return onSession('save your schedule', () => saveAccountSchedule(schedule));
  },

  async deleteSchedule(id: ScheduleId): Promise<void> {
    if (!(await signedIn())) {
      deleteGuestSchedule(id);
      return;
    }
    await onSession('delete your schedule', () => deleteAccountSchedule(id));
  },

  async setCurrentSchedule(term: TermCode, id: ScheduleId): Promise<void> {
    if (!(await signedIn())) {
      setGuestCurrentSchedule(term, id);
      return;
    }
    writeJson(CURRENT_SCHEDULE_KEY + term, id);
  },

  async reportRequirement(report: RequirementErrorReport): Promise<void> {
    try {
      // All four keys are required on the wire; a missing one is `null`.
      await postRequirementReport({
        requirement: report.requirement ?? null,
        program: report.program ?? null,
        catalogYear: report.catalogYear ?? null,
        message: report.message,
      });
    } catch (e) {
      throw rateLimited(e) ?? e;
    }
  },

  async listPrograms(): Promise<ProgramSummary[]> {
    return (await listPrograms()).map(programSummaryFromWire);
  },

  session,

  async requestSignInCode(email: string): Promise<void> {
    try {
      await auth.requestCode(email);
    } catch (e) {
      throw rateLimited(e) ?? e;
    }
  },

  verifySignInCode,

  async signOut(): Promise<void> {
    try {
      await auth.logout();
    } catch (e) {
      if (!isApiError(e, 'unauthenticated')) {
        throw e;
      }
      // Already signed out, which is what was asked for.
    }
    forgetSession();
  },

  claimGuestData,

  async deleteAccount(): Promise<void> {
    await onSession('delete your account', () => account.remove());
    forgetEverything();
    clearGuest();
    forgetCurrentSchedules();
  },

  async freshness(): Promise<{stale: boolean; staleSince?: string}> {
    const stale = (await meta()).jobs.filter(j => j.stale);
    const lastOk = stale
      .map(j => j.lastOk)
      .filter(at => at !== null)
      .sort()
      .at(-1);
    return lastOk === undefined
      ? {stale: stale.length > 0}
      : {stale: true, staleSince: lastOk};
  },
};
