/**
 * The one writer of the plan document. Every edit path (the board, the
 * catalog's "Add to plan") queues through here, so two writers cannot
 * overwrite each other from stale snapshots, a burst of drops is one write,
 * and leaving the page flushes instead of losing the last 500 ms.
 */
import type {Plan} from '../domain';
import {dataSource} from './index';

export type SaveStatus = 'saved' | 'pending' | 'saving';

const DEBOUNCE_MS = 500;

let pending: Plan | undefined;
let timer: ReturnType<typeof setTimeout> | undefined;
let inFlight: Promise<void> | undefined;
let status: SaveStatus = 'saved';
const listeners = new Set<() => void>();

function setStatus(next: SaveStatus): void {
  if (status !== next) {
    status = next;
    listeners.forEach(l => l());
  }
}

function writeNow(): Promise<void> {
  const plan = pending;
  pending = undefined;
  if (timer !== undefined) {
    clearTimeout(timer);
    timer = undefined;
  }
  if (plan === undefined) {
    return inFlight ?? Promise.resolve();
  }
  setStatus('saving');
  const previous = inFlight ?? Promise.resolve();
  // One PUT at a time, in order: a later plan never lands before an earlier one.
  const run = previous
    .then(() => dataSource.savePlan(plan))
    .finally(() => {
      if (inFlight === run) {
        inFlight = undefined;
        setStatus(pending === undefined ? 'saved' : 'pending');
      }
    });
  inFlight = run;
  return run;
}

/** Save soon. The newest plan wins; earlier queued ones are never written. */
export function queuePlanSave(plan: Plan, delay = DEBOUNCE_MS): void {
  pending = plan;
  setStatus('pending');
  if (timer !== undefined) {
    clearTimeout(timer);
  }
  if (delay === 0) {
    void writeNow();
    return;
  }
  timer = setTimeout(() => void writeNow(), delay);
}

/** Write whatever is pending now and wait for every write in flight. */
export function flushPlanSave(): Promise<void> {
  return writeNow();
}

export function getSaveStatus(): SaveStatus {
  return status;
}

export function subscribeSaveStatus(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

// The tab is going away: the pending plan must not go with it. localStorage
// writes synchronously before the first await, so this is enough for the demo.
if (typeof window !== 'undefined') {
  window.addEventListener('pagehide', () => void flushPlanSave());
}
