/**
 * Demo data source: the CS + Stats fixture, with edits kept in localStorage
 * so a reload keeps the board and "Reset" throws them away. No network.
 */
import {
  courseKey,
  type CourseInfo,
  type Plan,
  type PlanBundle,
  type PlanId,
} from '../domain';
import {
  bundle as fixtureBundle,
  catalogCandidates as fixtureCandidates,
  savedCollections as fixtureCollections,
} from '../fixtures/csStats';
import type {DataSource, SavedCollection} from './types';

const PLAN_KEY = 'skyspace.demo.plan.v1.';
const COLLECTIONS_KEY = 'skyspace.demo.collections.v1';

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

const isPlan = (v: unknown): v is Plan =>
  typeof v === 'object' && v !== null && 'terms' in v && 'programs' in v;

const isCollections = (v: unknown): v is SavedCollection[] =>
  Array.isArray(v) &&
  v.every(c => typeof c === 'object' && c !== null && 'courses' in c);

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
    remove(COLLECTIONS_KEY);
    // The key an earlier build used; harmless to clear.
    remove(`skyspace.plan.v1.${id}`);
  },

  async loadCollections(): Promise<SavedCollection[]> {
    return read(COLLECTIONS_KEY, isCollections) ?? fixtureCollections;
  },

  async saveCollections(collections: SavedCollection[]): Promise<void> {
    write(COLLECTIONS_KEY, collections);
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
};
