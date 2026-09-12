/** Copy and hrefs the catalog and class pages share. Kept out of component files so fast refresh stays whole-file. */
import {
  formatCourseCode,
  seatStatus,
  seatsOpen,
  type CourseCode,
  type Seats,
  type Section,
} from '../domain';

/** Search the catalog for a code; a course has many sections, so a search is the honest target. */
export function catalogSearchHref(code: CourseCode): string {
  return `/catalog?q=${encodeURIComponent(formatCourseCode(code))}`;
}

/** "COMP 140 · 001 · CRN 12422". */
export function sectionIdentity(section: Section): string {
  const {code, section: number, crn} = section.listing;
  return `${code.subject} ${code.number} · ${number} · CRN ${crn}`;
}

/** The class page's headline: "67 enrolled of 72 · 5 open · waitlist 0 of 0". */
export function seatsHeadline(seats: Seats): string {
  return `${seats.enrolled} enrolled of ${seats.capacity} · ${seatsOpen(seats)} open · waitlist ${seats.waitlistCount} of ${seats.waitlistCapacity}`;
}

export const SEAT_DOT = {
  open: {variant: 'success', label: 'Open'},
  nearlyFull: {variant: 'warning', label: 'Nearly full'},
  waitlistOpen: {variant: 'warning', label: 'Waitlist open'},
  full: {variant: 'error', label: 'Full'},
} as const;

export function seatsDot(
  seats: Seats,
): (typeof SEAT_DOT)[keyof typeof SEAT_DOT] {
  return SEAT_DOT[seatStatus(seats)];
}
