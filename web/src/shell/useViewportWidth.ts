import {useSyncExternalStore} from 'react';

const subscribe = (onChange: () => void): (() => void) => {
  window.addEventListener('resize', onChange);
  return () => window.removeEventListener('resize', onChange);
};

const read = (): number => window.innerWidth;

/** Live viewport width. The phone gate and tablet layouts key off this, not the user agent. */
export function useViewportWidth(): number {
  return useSyncExternalStore(subscribe, read, read);
}

/** Below this the app does not render; it shows a notice instead (features/platform.md). */
export const MIN_SUPPORTED_WIDTH = 768;
/** Below this, two-pane layouts stack to one column. */
export const DESKTOP_WIDTH = 1024;
