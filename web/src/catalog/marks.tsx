/**
 * The bookmark star. Astryx's icon registry has no star or bookmark glyph,
 * so it is handed to `Icon` as an SVG component, the documented escape hatch.
 */
import type {SVGProps} from 'react';

const PATH =
  'M12 3.2l2.7 5.5 6 .9-4.3 4.2 1 6-5.4-2.8-5.4 2.8 1-6L3.3 9.6l6-.9z';

export function StarMark(props: SVGProps<SVGSVGElement>) {
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
      <path d={PATH} />
    </svg>
  );
}

export function StarFilledMark(props: SVGProps<SVGSVGElement>) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="currentColor"
      stroke="currentColor"
      strokeWidth={1.6}
      strokeLinejoin="round"
      {...props}
    >
      <path d={PATH} />
    </svg>
  );
}
