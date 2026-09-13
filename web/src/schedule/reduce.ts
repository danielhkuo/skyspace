/**
 * Edits to one schedule document, pure so the page and the catalog's "Add to
 * schedule" apply the same rules. Every action returns a new document; the
 * caller saves it.
 */
import {
  courseKey,
  newScheduleId,
  sameCourse,
  type CourseCode,
  type Crn,
  type TermCode,
  type TermSchedule,
} from '../domain';
import {COLOURS} from './labels';

export type ScheduleAction =
  /** Adds the course; an existing candidate is shown again and, given a CRN, re-picked. */
  | {type: 'addCandidate'; course: CourseCode; crn?: Crn}
  | {type: 'pickSection'; course: CourseCode; crn: Crn}
  | {type: 'toggleVisible'; course: CourseCode}
  | {type: 'removeCandidate'; course: CourseCode}
  | {type: 'setColour'; course: CourseCode; colour: number}
  | {type: 'rename'; name: string};

export function newSchedule(term: TermCode, name: string): TermSchedule {
  return {id: newScheduleId(), name, term, candidates: [], busy: []};
}

/** The lowest hue no other candidate uses, so ten courses get ten colours before any repeat. */
export function freeColour(schedule: TermSchedule): number {
  const used = new Set(schedule.candidates.map(c => c.colour % COLOURS.length));
  for (let i = 0; i < COLOURS.length; i += 1) {
    if (!used.has(i)) {
      return i;
    }
  }
  return schedule.candidates.length % COLOURS.length;
}

export function reduceSchedule(
  schedule: TermSchedule,
  action: ScheduleAction,
): TermSchedule {
  switch (action.type) {
    case 'addCandidate': {
      const existing = schedule.candidates.find(c =>
        sameCourse(c.course, action.course),
      );
      if (existing !== undefined) {
        return {
          ...schedule,
          candidates: schedule.candidates.map(c =>
            c === existing
              ? {
                  ...c,
                  visible: true,
                  sections:
                    action.crn === undefined ? c.sections : [action.crn],
                }
              : c,
          ),
        };
      }
      return {
        ...schedule,
        candidates: [
          ...schedule.candidates,
          {
            course: action.course,
            sections: action.crn === undefined ? [] : [action.crn],
            visible: true,
            colour: freeColour(schedule),
          },
        ],
      };
    }
    case 'pickSection':
      return {
        ...schedule,
        candidates: schedule.candidates.map(c =>
          sameCourse(c.course, action.course)
            ? {...c, sections: [action.crn]}
            : c,
        ),
      };
    case 'toggleVisible':
      return {
        ...schedule,
        candidates: schedule.candidates.map(c =>
          sameCourse(c.course, action.course) ? {...c, visible: !c.visible} : c,
        ),
      };
    case 'removeCandidate':
      return {
        ...schedule,
        candidates: schedule.candidates.filter(
          c => courseKey(c.course) !== courseKey(action.course),
        ),
      };
    case 'setColour':
      return {
        ...schedule,
        candidates: schedule.candidates.map(c =>
          sameCourse(c.course, action.course)
            ? {...c, colour: action.colour}
            : c,
        ),
      };
    case 'rename':
      return {...schedule, name: action.name};
  }
}
