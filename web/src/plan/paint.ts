/**
 * Paint Astryx has no prop for: the subject stripe, the dashed drop slot,
 * the lifted card, violet self-check ink. All values are tokens; structural
 * widths are the only raw px (`web/AGENTS.md`).
 */
import type {CSSProperties} from 'react';

/** Astryx ships ten hue families; a subject hashes onto one so COMP is the same colour everywhere. */
const HUES = [
  'blue',
  'cyan',
  'green',
  'orange',
  'pink',
  'purple',
  'red',
  'teal',
  'yellow',
  'gray',
] as const;

export type Hue = (typeof HUES)[number];

/** Hand-assigned for the subjects a launch plan sees most; the rest hash. */
const PINNED: Record<string, Hue> = {
  COMP: 'cyan',
  MATH: 'red',
  STAT: 'pink',
  FWIS: 'teal',
  LPAP: 'orange',
  CHEM: 'gray',
  PHYS: 'pink',
  HIST: 'yellow',
  ENGL: 'green',
  MUSI: 'blue',
  PHIL: 'purple',
  HART: 'teal',
  ECON: 'green',
};

export function subjectHue(subject: string): Hue {
  const pinned = PINNED[subject];
  if (pinned !== undefined) {
    return pinned;
  }
  let hash = 0;
  for (const ch of subject) {
    hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  }
  return HUES[hash % HUES.length] ?? 'gray';
}

const LINE = 'var(--border-width) solid var(--color-border)';

export const stripe = (hue: Hue): string =>
  `inset 4px 0 0 0 var(--color-border-${hue})`;

/** A course row: top requirement plus the subject stripe. */
export function rowStyle(hue: Hue): CSSProperties {
  return {borderBlockStart: LINE, boxShadow: stripe(hue)};
}

export const rowDivider: CSSProperties = {borderBlockStart: LINE};
export const islandHead: CSSProperties = {borderBlockEnd: LINE};
export const islandNow: CSSProperties = {
  boxShadow: 'inset 0 0 0 2px var(--color-accent)',
};
export const islandHover: CSSProperties = {
  boxShadow: 'inset 0 0 0 2px var(--color-accent)',
  background: 'var(--color-accent-muted)',
};
export const islandDimmed: CSSProperties = {opacity: 0.6};

/** A row carrying a warning: the yellow wash, so the problem is visible from across the board. */
export const warningRow: CSSProperties = {
  background: 'var(--color-background-yellow)',
};

export const selfCheckRow: CSSProperties = {
  borderBlockStart: LINE,
  boxShadow: 'inset 3px 0 0 0 var(--color-border-purple)',
};
export const violetInk: CSSProperties = {color: 'var(--color-text-purple)'};
export const amberInk: CSSProperties = {color: 'var(--color-text-yellow)'};
export const greenInk: CSSProperties = {color: 'var(--color-text-green)'};

export const slotRow: CSSProperties = {
  border: '2px dashed var(--color-accent)',
  background: 'var(--color-accent-muted)',
  borderRadius: 'var(--radius-inner)',
};
export const ghostRow: CSSProperties = {
  borderBlockStart: LINE,
  opacity: 0.4,
};
export function liftedCard(hue: Hue): CSSProperties {
  return {
    position: 'fixed',
    zIndex: 20,
    pointerEvents: 'none',
    boxShadow: `0 12px 28px var(--color-shadow), inset 0 0 0 2px var(--color-accent), ${stripe(hue)}`,
    background: 'var(--color-background-card)',
    borderRadius: 'var(--radius-inner)',
    transform: 'scale(0.96)',
  };
}
/** A requirement row is a drop target, so it reads as a slot, not a tag. */
export const requirementRow: CSSProperties = {
  border: 'var(--border-width) solid var(--color-border)',
  borderRadius: 'var(--radius-inner)',
  background: 'var(--color-background-card)',
  minHeight: 40,
};
export const requirementRowRaised: CSSProperties = {
  ...requirementRow,
  borderColor: 'var(--color-accent)',
  background: 'var(--color-accent-muted)',
};
export const requirementHit: CSSProperties = {
  background: 'var(--color-accent-muted)',
  borderRadius: 'var(--radius-inner)',
};
export function chipStyle(hue: Hue): CSSProperties {
  return {
    boxShadow: `${stripe(hue)}, inset 0 0 0 1px var(--color-border)`,
    borderRadius: 'var(--radius-inner)',
    background: 'var(--color-background-card)',
    cursor: 'grab',
  };
}
export const grabbable: CSSProperties = {cursor: 'grab', touchAction: 'none'};
