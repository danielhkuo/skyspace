/**
 * Courses: the wire's `Course` record and `CourseView` to the domain's
 * `CourseInfo` and `Section[]`. Best effort: the course page knows nothing
 * about which seasons a course runs in, so those fields are the zero value.
 */
import type {CourseFacts, CourseInfo} from '../../domain/facts';
import type {Section} from '../../domain/section';
import type {Course as WireCourse} from '../generated/Course';
import type {CourseFacts as WireCourseFacts} from '../generated/CourseFacts';
import type {CourseInfo as WireCourseInfo} from '../generated/CourseInfo';
import type {CourseView as WireCourseView} from '../generated/CourseView';
import {sectionFromWire} from './section';
import {opt} from './util';

/**
 * From a `GET /courses/...` record. `seasonsOffered`, `termsObserved` and
 * `offeredNow` come from the plan bundle's facts, not from the course page,
 * so they are empty here; `courseFactsFromWire` carries the real ones.
 */
export function courseInfoFromWire(c: WireCourse): CourseInfo {
  return {
    code: c.code,
    title: c.title,
    credits: c.credits,
    attributes: c.attributes,
    department: c.department,
    repeatable: c.flags.repeatable,
    seasonsOffered: [],
    termsObserved: 0,
    offeredNow: false,
  };
}

/** The bundle's `CourseInfo`: identical but for the nullable `department`. */
export function courseInfoFactFromWire(w: WireCourseInfo): CourseInfo {
  return {
    code: w.code,
    title: w.title,
    credits: w.credits,
    attributes: w.attributes,
    ...opt('department', w.department),
    repeatable: w.repeatable,
    seasonsOffered: w.seasonsOffered,
    termsObserved: w.termsObserved,
    offeredNow: w.offeredNow,
  };
}

export function courseFactsFromWire(w: WireCourseFacts): CourseFacts {
  return {
    courses: w.courses.map(courseInfoFactFromWire),
    aliases: w.aliases.map(([alias, target]) => [alias, target]),
  };
}

/** The view's sections, with the course record filling `notes` and `mutuallyExclusive`. */
export function courseViewSections(
  v: WireCourseView,
  riceAsOf?: string,
): Section[] {
  return v.sections.map(s => sectionFromWire(s, v.course, riceAsOf));
}
