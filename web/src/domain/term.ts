/**
 * Terms, mirroring `skyspace-core::term`. `TermCode` is Rice's data key
 * (`202710`); `TermPosition` orders the board and exists for terms Rice has
 * not published yet.
 */

/** Variant order is load-bearing: it is the board order inside an academic year. */
export type Season = 'fall' | 'spring' | 'summer';

const SEASON_ORDER: Record<Season, number> = {fall: 0, spring: 1, summer: 2};

export type TermPosition = {
  academicYear: number;
  season: Season;
};

export function compareTermPosition(a: TermPosition, b: TermPosition): number {
  if (a.academicYear !== b.academicYear) {
    return a.academicYear - b.academicYear;
  }
  return SEASON_ORDER[a.season] - SEASON_ORDER[b.season];
}

/** Fall 2026 is academic year 2027; spring and summer 2027 are too. */
export function calendarYear(position: TermPosition): number {
  return position.season === 'fall'
    ? position.academicYear - 1
    : position.academicYear;
}

const SEASON_LABEL: Record<Season, string> = {
  fall: 'Fall',
  spring: 'Spring',
  summer: 'Summer',
};

/** Rice's own label shape: "Fall Semester 2026". */
export function termLabel(position: TermPosition): string {
  return `${SEASON_LABEL[position.season]} Semester ${calendarYear(position)}`;
}

/** The short form the board headers and warnings use: "Fall 2026". */
export function shortTermLabel(position: TermPosition): string {
  return `${SEASON_LABEL[position.season]} ${calendarYear(position)}`;
}

/** Six digits: academic year then season code (`10` fall, `20` spring, `30` summer). */
export type TermCode = string;

/** `CatalogYear(2026)` is the 2026-27 General Announcements. */
export type CatalogYear = number;

export function formatCatalogYear(year: CatalogYear): string {
  return `${year}-${String(year + 1).slice(2)}`;
}
