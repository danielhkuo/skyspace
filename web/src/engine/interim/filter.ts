/**
 * `CourseFilter::matches` and `CourseFacts::canonical`, in TypeScript.
 *
 * Interim: `skyspace-core` owns this logic and the wasm build replaces it.
 * Kept small and literal so the two cannot drift far.
 */
import {
  canonical,
  courseInfo,
  courseLevel,
  sameCourse,
  type CourseCode,
  type CourseFacts,
  type CourseFilter,
  type CourseInfo,
  type CourseSelector,
} from '../../domain';

export {canonical, courseInfo};

export type FilterMatch = 'yes' | 'no' | 'unknown';

/** Unknown code comes back unchanged. Every code comparison calls this first. */
function selectorMatches(
  selector: CourseSelector,
  code: CourseCode,
  info: CourseInfo | undefined,
): FilterMatch {
  switch (selector.kind) {
    case 'code':
      return sameCourse(selector.code, code) ? 'yes' : 'no';
    case 'subject':
      return selector.subject === code.subject ? 'yes' : 'no';
    case 'numberRange': {
      if (selector.subject !== undefined && selector.subject !== code.subject) {
        return 'no';
      }
      const n = Number(/^\d+/.exec(code.number)?.[0] ?? 'NaN');
      return n >= selector.low && n <= selector.high ? 'yes' : 'no';
    }
    case 'attribute':
      if (info === undefined) {
        return 'unknown';
      }
      return info.attributes.includes(selector.attribute) ? 'yes' : 'no';
  }
}

/** `code` must already be canonical. Three values: a missing fact is not a non-match. */
export function filterMatches(
  filter: CourseFilter,
  code: CourseCode,
  facts: CourseFacts,
): FilterMatch {
  const info = courseInfo(facts, code);
  for (const ex of filter.exclude) {
    if (selectorMatches(ex, code, info) === 'yes') {
      return 'no';
    }
  }
  if (filter.include.length === 0) {
    return 'yes';
  }
  let sawUnknown = false;
  for (const inc of filter.include) {
    const m = selectorMatches(inc, code, info);
    if (m === 'yes') {
      return 'yes';
    }
    if (m === 'unknown') {
      sawUnknown = true;
    }
  }
  return sawUnknown ? 'unknown' : 'no';
}

export {courseLevel};
