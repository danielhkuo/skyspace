/**
 * Local persistence for the plan document until the API `PUT` exists
 * (`08-board-interaction.md` "Saving"). Same seam, same debounce; only the
 * destination changes later.
 */
import type {Plan, PlanId} from '../domain';

const KEY_PREFIX = 'skyspace.plan.v1.';
export const SAVE_DEBOUNCE_MS = 500;

export function loadPlan(id: PlanId): Plan | undefined {
  try {
    const raw = localStorage.getItem(KEY_PREFIX + id);
    if (raw === null) {
      return undefined;
    }
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== 'object' || parsed === null || !('terms' in parsed)) {
      return undefined;
    }
    return parsed as Plan;
  } catch {
    // Private mode or blocked storage: start from the fixture.
    return undefined;
  }
}

export function savePlan(plan: Plan): void {
  try {
    localStorage.setItem(KEY_PREFIX + plan.id, JSON.stringify(plan));
  } catch {
    // Storage full or blocked; the in-memory plan stays authoritative.
  }
}

export function clearPlan(id: PlanId): void {
  try {
    localStorage.removeItem(KEY_PREFIX + id);
  } catch {
    // Nothing to clear.
  }
}
