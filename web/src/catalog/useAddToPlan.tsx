import {useCallback, useMemo, useState, type ReactNode} from 'react';

import {dataSource} from '../datasource';
import {flushPlanSave, queuePlanSave} from '../datasource/planSaver';
import {
  courseInfo,
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
    const chosen = picking;
    // Never write from the snapshot this page loaded: the board may have
    // saved since. Flush its queue, reload, then reduce on that plan.
    void (async () => {
      await flushPlanSave();
      const fresh = await dataSource.loadBundle();
      const term = fresh.plan.terms.find(t => t.id === termId);
      if (term === undefined) {
        return;
      }
      const next = applyPick(fresh, term, chosen);
      queuePlanSave(next, 0);
      onSaved(next);
    })();
  };

  const applyPick = (
    fresh: PlanBundle,
    term: PlanBundle['plan']['terms'][number],
    chosen: {course: CourseCode; credits: Credits},
  ): Plan =>
    isRiceTerm(term.kind)
      ? reducePlan(fresh.plan, {
          type: 'addCourse',
          to: term.id,
          index: term.kind.rice.courses.length,
          course: {
            id: newEntryId(),
            course: chosen.course,
            credits: chosen.credits,
            fills: [],
          },
        })
      : reducePlan(fresh.plan, {
          type: 'addManualCard',
          to: term.id,
          card: {
            id: newEntryId(),
            origin: originForLabel(term.label),
            code: `${chosen.course.subject} ${chosen.course.number}`,
            title: courseInfo(fresh.facts, chosen.course)?.title ?? '',
            credits: chosen.credits,
            riceEquivalent: chosen.course,
            fills: [],
          },
        });

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
