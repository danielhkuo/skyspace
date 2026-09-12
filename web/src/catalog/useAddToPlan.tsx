import {useCallback, useMemo, useState, type ReactNode} from 'react';

import {dataSource} from '../datasource';
import {
  isRiceTerm,
  newEntryId,
  originForLabel,
  type CourseCode,
  type Credits,
  type Plan,
  type PlanBundle,
  type TermId,
} from '../domain';
import {engine} from '../engine';
import {courseInfo} from '../engine/interim/filter';
import {TermPickerDialog} from '../plan/TermPickerDialog';
import {reducePlan} from '../plan/usePlan';

export type AddToPlan = {
  /** Opens the term picker for a course; a no-op until the plan has loaded. */
  open: ((course: CourseCode, credits: Credits) => void) | undefined;
  dialog: ReactNode;
};

/**
 * "Add to plan" from outside the board: the same term picker the board's
 * tray uses, then the same reducer the board runs, then one save. The
 * caller keeps the returned plan so the page reflects the placement.
 */
export function useAddToPlan(
  bundle: PlanBundle | undefined,
  onSaved: (plan: Plan) => void,
): AddToPlan {
  const [picking, setPicking] = useState<
    {course: CourseCode; credits: Credits} | undefined
  >(undefined);
  const report = useMemo(
    () => (bundle === undefined ? undefined : engine.evaluate(bundle)),
    [bundle],
  );

  const open = useCallback((course: CourseCode, credits: Credits) => {
    setPicking({course, credits});
  }, []);

  const pick = (termId: TermId): void => {
    if (bundle === undefined || picking === undefined) {
      return;
    }
    const term = bundle.plan.terms.find(t => t.id === termId);
    if (term === undefined) {
      return;
    }
    const next = isRiceTerm(term.kind)
      ? reducePlan(bundle.plan, {
          type: 'addCourse',
          to: termId,
          index: term.kind.rice.courses.length,
          course: {
            id: newEntryId(),
            course: picking.course,
            credits: picking.credits,
            fills: [],
          },
        })
      : reducePlan(bundle.plan, {
          type: 'addManualCard',
          to: termId,
          card: {
            id: newEntryId(),
            origin: originForLabel(term.label),
            code: `${picking.course.subject} ${picking.course.number}`,
            title: courseInfo(bundle.facts, picking.course)?.title ?? '',
            credits: picking.credits,
            riceEquivalent: picking.course,
            fills: [],
          },
        });
    void dataSource.savePlan(next);
    onSaved(next);
  };

  const dialog =
    bundle === undefined || report === undefined ? null : (
      <TermPickerDialog
        isOpen={picking !== undefined}
        course={picking?.course}
        credits={picking?.credits ?? 0}
        bundle={bundle}
        report={report}
        onClose={() => setPicking(undefined)}
        onPick={pick}
      />
    );

  return {open: bundle === undefined ? undefined : open, dialog};
}
