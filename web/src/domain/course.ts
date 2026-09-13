/**
 * Course codes and credit hours, mirroring `skyspace-core::code` and `::term`.
 *
 * Credits are hundredths of a credit hour (`02-domain.md`): Rice prints `.75`
 * and `1.5`, sums must be exact, and the wire value is an integer.
 */

export type CourseCode = {
  subject: string;
  number: string;
};

/** Map key and equality for a course code. `COMP 140` and `comp140` are the same course. */
export function courseKey(code: CourseCode): string {
  return `${code.subject.toUpperCase()} ${code.number.toUpperCase()}`;
}

export function formatCourseCode(code: CourseCode): string {
  return `${code.subject} ${code.number}`;
}

export function sameCourse(a: CourseCode, b: CourseCode): boolean {
  return courseKey(a) === courseKey(b);
}

/** `"COMP 140"`, `"comp140"`, `"COMP-140"` all parse. Returns `null` for anything else. */
export function parseCourseCode(raw: string): CourseCode | null {
  const match = /^\s*([A-Za-z]{2,8})[\s-]*(\d{1,4}[A-Za-z]{0,2})\s*$/.exec(raw);
  if (match === null) {
    return null;
  }
  const subject = match[1];
  const number = match[2];
  if (subject === undefined || number === undefined) {
    return null;
  }
  return {subject: subject.toUpperCase(), number: number.toUpperCase()};
}

/** Leading digits of the course number; `level` is that rounded down to the hundred. */
export function courseLevel(code: CourseCode): number {
  const digits = /^\d+/.exec(code.number)?.[0] ?? '0';
  return Math.floor(Number(digits) / 100) * 100;
}

/** Hundredths of a credit hour. `300` is three credit hours. */
export type Credits = number;

export const ZERO_CREDITS: Credits = 0;

export function creditsFromHours(hours: number): Credits {
  return Math.round(hours * 100);
}

/** `300` -> `"3"`, `75` -> `".75"`, `150` -> `"1.5"`. Rice's own spellings. */
export function formatCredits(credits: Credits): string {
  const whole = Math.floor(credits / 100);
  const cents = credits % 100;
  if (cents === 0) {
    return String(whole);
  }
  const fraction = (cents / 100).toFixed(2).replace(/0$/, '').slice(1);
  return whole === 0 ? fraction : `${whole}${fraction}`;
}

export type CreditRange =
  | {kind: 'fixed'; value: Credits}
  | {kind: 'range'; value: {min: Credits; max: Credits}}
  | {kind: 'either'; value: [Credits, Credits]};

export function creditRangeMin(range: CreditRange): Credits {
  switch (range.kind) {
    case 'fixed':
      return range.value;
    case 'range':
      return range.value.min;
    case 'either':
      return Math.min(range.value[0], range.value[1]);
  }
}

export function creditRangeMax(range: CreditRange): Credits {
  switch (range.kind) {
    case 'fixed':
      return range.value;
    case 'range':
      return range.value.max;
    case 'either':
      return Math.max(range.value[0], range.value[1]);
  }
}

/** `3`, `1 TO 4`, `1 OR 3`; GA's `3-4` and `1 or 3` too. */
export function formatCreditRange(range: CreditRange): string {
  switch (range.kind) {
    case 'fixed':
      return formatCredits(range.value);
    case 'range':
      return `${formatCredits(range.value.min)} TO ${formatCredits(range.value.max)}`;
    case 'either':
      return `${formatCredits(range.value[0])} OR ${formatCredits(range.value[1])}`;
  }
}
