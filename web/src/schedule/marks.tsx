/** The registry has `eyeSlash` but no open eye, so the show/hide toggle brings its own. */
import type {SVGProps} from 'react';

export function EyeMark(props: SVGProps<SVGSVGElement>) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.6}
      strokeLinecap="round"
      strokeLinejoin="round"
      {...props}
    >
      <path d="M2 12s3.6-6.5 10-6.5S22 12 22 12s-3.6 6.5-10 6.5S2 12 2 12z" />
      <circle cx={12} cy={12} r={3} />
    </svg>
  );
}
