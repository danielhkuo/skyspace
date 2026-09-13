/**
 * Programs: wire to domain. The wire tags requirement bodies, course
 * selectors and non-course kinds in snake_case; the domain in camelCase
 * (gap report §2). Read-only: the browser never writes a program.
 */
import type {ProgramId, RequirementId} from '../../domain/ids';
import type {
  CourseFilter,
  CourseSelector,
  NonCourseKind,
  Program,
  ProgramSummary,
  Requirement,
  RequirementBody,
  SourceRef,
} from '../../domain/program';
import type {CourseFilter as WireCourseFilter} from '../generated/CourseFilter';
import type {CourseSelector as WireCourseSelector} from '../generated/CourseSelector';
import type {NonCourseKind as WireNonCourseKind} from '../generated/NonCourseKind';
import type {Program as WireProgram} from '../generated/Program';
import type {ProgramSummary as WireProgramSummary} from '../generated/ProgramSummary';
import type {Requirement as WireRequirement} from '../generated/Requirement';
import type {RequirementBody as WireRequirementBody} from '../generated/RequirementBody';
import type {SourceRef as WireSourceRef} from '../generated/SourceRef';
import {lookup, opt, unknownWire} from './util';

const NON_COURSE_KIND: Record<WireNonCourseKind, NonCourseKind> = {
  proficiency_exam: 'proficiencyExam',
  portfolio: 'portfolio',
  other: 'other',
};

export function nonCourseKindFromWire(w: WireNonCourseKind): NonCourseKind {
  return lookup(NON_COURSE_KIND, w, 'nonCourseKind');
}

export function courseSelectorFromWire(w: WireCourseSelector): CourseSelector {
  switch (w.kind) {
    case 'code':
      return {kind: 'code', code: w.code};
    case 'subject':
      return {kind: 'subject', subject: w.subject};
    case 'number_range':
      return {
        kind: 'numberRange',
        ...opt('subject', w.subject),
        low: w.low,
        high: w.high,
      };
    case 'attribute':
      return {kind: 'attribute', attribute: w.attribute};
    default:
      return unknownWire('courseSelector', w);
  }
}

export function courseFilterFromWire(w: WireCourseFilter): CourseFilter {
  return {
    include: w.include.map(courseSelectorFromWire),
    exclude: w.exclude.map(courseSelectorFromWire),
  };
}

function sourceRefFromWire(w: WireSourceRef): SourceRef {
  return {url: w.url, ...opt('anchor', w.anchor)};
}

function requirementBodyFromWire(w: WireRequirementBody): RequirementBody {
  switch (w.kind) {
    case 'all':
      return {kind: 'all', of: w.of.map(requirementFromWire)};
    case 'select':
      return {
        kind: 'select',
        count: w.count,
        of: w.of.map(requirementFromWire),
      };
    case 'course':
      return {
        kind: 'course',
        filter: courseFilterFromWire(w.filter),
        semesters: w.semesters,
      };
    case 'credits':
      return {
        kind: 'credits',
        minimum: w.minimum,
        scope: w.scope,
        from: courseFilterFromWire(w.from),
      };
    case 'non_course':
      return {
        kind: 'nonCourse',
        nonCourseKind: nonCourseKindFromWire(w.nonCourseKind),
        description: w.description,
      };
    case 'unverifiable':
      return {kind: 'unverifiable', text: w.text};
    case 'distinct_departments':
      return {kind: 'distinctDepartments', minimum: w.minimum, text: w.text};
    default:
      return unknownWire('requirementBody', w);
  }
}

export function requirementFromWire(w: WireRequirement): Requirement {
  return {
    id: w.id as RequirementId,
    label: w.label,
    ...opt('hours', w.hours),
    source: sourceRefFromWire(w.source),
    body: requirementBodyFromWire(w.body),
  };
}

/** `review` has no domain field and is dropped. */
export function programFromWire(w: WireProgram): Program {
  return {
    id: w.id as ProgramId,
    catalogYear: w.catalogYear,
    slug: w.slug,
    kind: w.kind,
    name: w.name,
    credential: w.credential,
    ...opt('totalCredits', w.totalCredits),
    source: sourceRefFromWire(w.source),
    root: requirementFromWire(w.root),
    retiredRequirements: w.retiredRequirements.map(id => id as RequirementId),
  };
}

/** `totalCredits: null` (a real null, `dto.rs`) becomes an absent key. */
export function programSummaryFromWire(w: WireProgramSummary): ProgramSummary {
  return {
    id: w.id as ProgramId,
    slug: w.slug,
    kind: w.kind,
    name: w.name,
    credential: w.credential,
    catalogYears: w.catalogYears,
    ...opt('totalCredits', w.totalCredits),
  };
}
