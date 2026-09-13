/**
 * A page with unsaved work (onboarding) registers a message here; the shell
 * asks before an in-app navigation, and the browser asks before a reload or
 * close. One guard at a time; clearing it is the page's job.
 */
let message: string | undefined;
const listeners = new Set<() => void>();

export function setLeaveGuard(next: string | undefined): void {
  message = next;
  listeners.forEach(l => l());
}

export function getLeaveGuard(): string | undefined {
  return message;
}

export function subscribeLeaveGuard(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

if (typeof window !== 'undefined') {
  window.addEventListener('beforeunload', event => {
    if (message !== undefined) {
      event.preventDefault();
    }
  });
}
