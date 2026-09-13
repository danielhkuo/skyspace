/**
 * `TakenIndex`: the earliest appearance of each course on the board.
 * Manual cards enter only through `riceEquivalent`. Codes are canonicalised.
 */
import {
  compareTermPosition,
  courseKey,
  isAwayTerm,
  isRiceTerm,
  type CourseCode,
  type CourseFacts,
  type EntryId,
  type Placed,
  type Plan,
} from '../../domain';
import {canonical} from './filter';

export type TakenIndex = Map<string, Placed>;

function placedBefore(a: Placed, b: Placed): boolean {
  if (a.when.kind === 'incoming') {
    return true;
  }
  if (b.when.kind === 'incoming') {
    return false;
  }
  return compareTermPosition(a.when.value, b.when.value) < 0;
}

/** Build once per check. `skip` leaves one card out, for the drag preview. */
export function buildTakenIndex(
  plan: Plan,
  facts: CourseFacts,
  skip?: EntryId,
): TakenIndex {
  const index: TakenIndex = new Map();
  const record = (code: CourseCode, placed: Placed): void => {
    const key = courseKey(canonical(facts, code));
    const existing = index.get(key);
    if (existing === undefined || placedBefore(placed, existing)) {
      index.set(key, placed);
    }
  };

  for (const card of plan.incomingCredit) {
    if (card.riceEquivalent !== undefined && card.id !== skip) {
      record(card.riceEquivalent, {when: {kind: 'incoming'}, entry: card.id});
    }
  }
  for (const term of plan.terms) {
    const when = {kind: 'term', value: term.position} as const;
    if (isRiceTerm(term.kind)) {
      for (const course of term.kind.rice.courses) {
        if (course.id !== skip) {
          record(course.course, {when, term: term.id, entry: course.id});
        }
      }
    } else if (isAwayTerm(term.kind)) {
      for (const card of term.kind.away.cards) {
        if (card.riceEquivalent !== undefined && card.id !== skip) {
          record(card.riceEquivalent, {when, term: term.id, entry: card.id});
        }
      }
    }
  }
  return index;
}

export function earliest(
  index: TakenIndex,
  facts: CourseFacts,
  code: CourseCode,
): Placed | undefined {
  return index.get(courseKey(canonical(facts, code)));
}
