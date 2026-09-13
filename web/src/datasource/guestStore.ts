/**
 * What a signed-out browser keeps (`06-api.md` "Guest mode"): schedules,
 * favorites and the schedule each term opens, all under one localStorage
 * key so a claim can hand the lot to the account and clear it in one go.
 * The generic JSON helpers are exported because the API source keeps two
 * more keys of its own the same way.
 */
import {
  sameCourse,
  type CourseCode,
  type ScheduleId,
  type TermCode,
  type TermSchedule,
} from '../domain';

export const GUEST_KEY = 'skyspace.guest.v1';

export type GuestStore = {
  schedules: TermSchedule[];
  favorites: CourseCode[];
  currentSchedule: Record<TermCode, ScheduleId>;
};

const EMPTY: GuestStore = {schedules: [], favorites: [], currentSchedule: {}};

export function readJson<T>(
  key: string,
  isValid: (v: unknown) => v is T,
): T | undefined {
  try {
    const raw = localStorage.getItem(key);
    if (raw === null) {
      return undefined;
    }
    const parsed: unknown = JSON.parse(raw);
    return isValid(parsed) ? parsed : undefined;
  } catch {
    // Private mode or blocked storage: behave as if nothing was saved.
    return undefined;
  }
}

export function writeJson(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Storage full or blocked; in-memory state stays authoritative.
  }
}

export function removeKey(key: string): void {
  try {
    localStorage.removeItem(key);
  } catch {
    // Nothing to remove.
  }
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null;

const isCourseCode = (v: unknown): v is CourseCode =>
  isRecord(v) &&
  typeof v['subject'] === 'string' &&
  typeof v['number'] === 'string';

const isCandidate = (v: unknown): boolean =>
  isRecord(v) &&
  isCourseCode(v['course']) &&
  Array.isArray(v['sections']) &&
  typeof v['visible'] === 'boolean' &&
  typeof v['colour'] === 'number';

const isSchedule = (v: unknown): v is TermSchedule =>
  isRecord(v) &&
  typeof v['id'] === 'string' &&
  typeof v['name'] === 'string' &&
  typeof v['term'] === 'string' &&
  Array.isArray(v['candidates']) &&
  v['candidates'].every(isCandidate) &&
  Array.isArray(v['busy']);

/** A hand-edited or stale document falls back to an empty store rather than a crash. */
const isGuestStore = (v: unknown): v is GuestStore =>
  isRecord(v) &&
  Array.isArray(v['schedules']) &&
  v['schedules'].every(isSchedule) &&
  Array.isArray(v['favorites']) &&
  v['favorites'].every(isCourseCode) &&
  isRecord(v['currentSchedule']) &&
  Object.values(v['currentSchedule']).every(id => typeof id === 'string');

export function readGuest(): GuestStore {
  return readJson(GUEST_KEY, isGuestStore) ?? EMPTY;
}

function writeGuest(store: GuestStore): void {
  writeJson(GUEST_KEY, store);
}

export function clearGuest(): void {
  removeKey(GUEST_KEY);
}

/** Whether the store holds anything worth claiming. */
export function guestIsEmpty(store: GuestStore = readGuest()): boolean {
  return store.schedules.length === 0 && store.favorites.length === 0;
}

export function guestSchedules(term: TermCode): {
  schedules: TermSchedule[];
  current?: ScheduleId;
} {
  const store = readGuest();
  const schedules = store.schedules.filter(s => s.term === term);
  const current = store.currentSchedule[term];
  return schedules.some(s => s.id === current)
    ? {schedules, current}
    : {schedules};
}

export function saveGuestSchedule(schedule: TermSchedule): void {
  const store = readGuest();
  const known = store.schedules.some(s => s.id === schedule.id);
  writeGuest({
    ...store,
    schedules: known
      ? store.schedules.map(s => (s.id === schedule.id ? schedule : s))
      : [...store.schedules, schedule],
  });
}

export function deleteGuestSchedule(id: ScheduleId): void {
  const store = readGuest();
  const currentSchedule = Object.fromEntries(
    Object.entries(store.currentSchedule).filter(([, cur]) => cur !== id),
  );
  writeGuest({
    ...store,
    schedules: store.schedules.filter(s => s.id !== id),
    currentSchedule,
  });
}

export function setGuestCurrentSchedule(term: TermCode, id: ScheduleId): void {
  const store = readGuest();
  writeGuest({
    ...store,
    currentSchedule: {...store.currentSchedule, [term]: id},
  });
}

export function guestFavorites(): CourseCode[] {
  return readGuest().favorites;
}

export function saveGuestFavorites(favorites: CourseCode[]): void {
  const store = readGuest();
  const deduped = favorites.filter(
    (code, i) => favorites.findIndex(other => sameCourse(other, code)) === i,
  );
  writeGuest({...store, favorites: deduped});
}
