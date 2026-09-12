/**
 * Demo data source: the CS + Stats fixture, with edits kept in localStorage
 * so a reload keeps the board and "Reset" throws them away. No network.
 */
import {applyQuery, partsOfTermIn, subjectsIn} from '../catalog/query';
import {
  courseKey,
  sameCourse,
  type CatalogQuery,
  type CourseCode,
  type CourseInfo,
  type Crn,
  type Plan,
  type PlanBundle,
  type PlanId,
  type Section,
  type SectionPage,
  type TermCode,
} from '../domain';
import {
  bundle as fixtureBundle,
  catalogCandidates as fixtureCandidates,
  favorites as fixtureFavorites,
} from '../fixtures/csStats';
import {
  FALL_2026,
  FALL_2026_LABEL,
  fallSections,
} from '../fixtures/fallSections';
import type {DataSource} from './types';

const PLAN_KEY = 'skyspace.demo.plan.v1.';
const FAVORITES_KEY = 'skyspace.demo.favorites.v1';

function read<T>(key: string, isValid: (v: unknown) => v is T): T | undefined {
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

function write(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Storage full or blocked; in-memory state stays authoritative.
  }
}

function remove(key: string): void {
  try {
    localStorage.removeItem(key);
  } catch {
    // Nothing to remove.
  }
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null;

const isTermKind = (v: unknown): boolean =>
  v === 'off' ||
  (isRecord(v) &&
    (('rice' in v &&
      isRecord(v['rice']) &&
      Array.isArray(v['rice']['courses'])) ||
      ('away' in v &&
        isRecord(v['away']) &&
        Array.isArray(v['away']['cards']))));

const isTerm = (v: unknown): boolean =>
  isRecord(v) &&
  typeof v['id'] === 'string' &&
  isRecord(v['position']) &&
  typeof v['position']['academicYear'] === 'number' &&
  ['fall', 'spring', 'summer'].includes(String(v['position']['season'])) &&
  isTermKind(v['kind']) &&
  Array.isArray(v['nonCourse']);

/**
 * Shape check on what localStorage hands back. A stale or hand-edited
 * document falls back to the fixture rather than blanking the board.
 */
const isPlan = (v: unknown): v is Plan =>
  isRecord(v) &&
  typeof v['id'] === 'string' &&
  typeof v['name'] === 'string' &&
  Array.isArray(v['programs']) &&
  Array.isArray(v['incomingCredit']) &&
  Array.isArray(v['selfChecks']) &&
  Array.isArray(v['terms']) &&
  v['terms'].every(isTerm);

const isCourseCodes = (v: unknown): v is CourseCode[] =>
  Array.isArray(v) &&
  v.every(c => typeof c === 'object' && c !== null && 'subject' in c);

export const demoDataSource: DataSource = {
  kind: 'demo',

  async loadBundle(): Promise<PlanBundle> {
    const saved = read(PLAN_KEY + fixtureBundle.plan.id, isPlan);
    return {...fixtureBundle, plan: saved ?? fixtureBundle.plan};
  },

  async savePlan(plan: Plan): Promise<void> {
    write(PLAN_KEY + plan.id, plan);
  },

  async resetPlan(id: PlanId): Promise<void> {
    remove(PLAN_KEY + id);
    remove(FAVORITES_KEY);
    // The key an earlier build used; harmless to clear.
    remove(`skyspace.plan.v1.${id}`);
  },

  async loadFavorites(): Promise<CourseCode[]> {
    return read(FAVORITES_KEY, isCourseCodes) ?? fixtureFavorites;
  },

  async saveFavorites(favorites: CourseCode[]): Promise<void> {
    write(FAVORITES_KEY, favorites);
  },

  async searchCourses(query: string, limit: number): Promise<CourseInfo[]> {
    const q = query.trim().toLowerCase().replace(/\s+/g, ' ');
    if (q === '') {
      return [];
    }
    const compact = q.replace(/ /g, '');
    return fixtureBundle.facts.courses
      .filter(info => {
        const code = courseKey(info.code).toLowerCase();
        return (
          code.replace(/ /g, '').startsWith(compact) ||
          code.includes(q) ||
          info.title.toLowerCase().includes(q)
        );
      })
      .sort((a, b) => courseKey(a.code).localeCompare(courseKey(b.code)))
      .slice(0, limit);
  },

  async catalogCandidates() {
    return fixtureCandidates;
  },

  async currentTerm() {
    return {code: FALL_2026, label: FALL_2026_LABEL};
  },

  async searchSections(
    term: TermCode,
    query: CatalogQuery,
  ): Promise<SectionPage> {
    return applyQuery(term === FALL_2026 ? fallSections : [], query);
  },

  async getSection(term: TermCode, crn: Crn): Promise<Section | undefined> {
    return term === FALL_2026
      ? fallSections.find(s => s.listing.crn === crn)
      : undefined;
  },

  async courseSections(term: TermCode, code: CourseCode): Promise<Section[]> {
    return term === FALL_2026
      ? fallSections.filter(s => sameCourse(s.listing.code, code))
      : [];
  },

  async listSubjects(term: TermCode): Promise<string[]> {
    return term === FALL_2026 ? subjectsIn(fallSections) : [];
  },

  async listPartsOfTerm(term: TermCode): Promise<string[]> {
    return term === FALL_2026 ? partsOfTermIn(fallSections) : [];
  },
};
