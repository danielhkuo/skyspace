/**
 * The status marks the design vocabulary needs and Astryx's registry lacks:
 * a hollow circle (unmet), a half circle (partial), the grip, and a plus.
 * Handed to `Icon` as SVG components, the documented escape hatch.
 */
import type {SVGProps} from 'react';

const base = {
  viewBox: '0 0 24 24',
  fill: 'none',
  stroke: 'currentColor',
  strokeWidth: 1.8,
  strokeLinecap: 'round',
} as const;

export function HollowMark(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...base} {...props}>
      <circle cx={12} cy={12} r={8} />
    </svg>
  );
}

export function HalfMark(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...base} {...props}>
      <circle cx={12} cy={12} r={8} />
      <path d="M12 4a8 8 0 0 1 0 16z" fill="currentColor" stroke="none" />
    </svg>
  );
}

export function GripMark(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...base} {...props}>
      <path
        strokeWidth={2.6}
        d="M9 6.5h.01M9 12h.01M9 17.5h.01M15 6.5h.01M15 12h.01M15 17.5h.01"
      />
    </svg>
  );
}

export function PlusMark(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...base} {...props}>
      <path d="M12 5v14M5 12h14" />
    </svg>
  );
}
