/**
 * Pointer-event drag for the plan page (`08-board-interaction.md`): HTML5 drag
 * has no touch support for custom previews and cannot auto-scroll, so this
 * tracks the pointer itself. A drag lifts after 4 px of travel, previews every
 * term on lift, hit-tests registered targets on move, and mutates the plan on
 * drop. Targets are term islands, the saved tray, and rule rows.
 */
import {useCallback, useEffect, useMemo, useRef, useState} from 'react';
import type {PointerEvent as ReactPointerEvent} from 'react';

import {
  isAwayTerm,
  isOffTerm,
  isRiceTerm,
  newEntryId,
  originForLabel,
  type CourseCode,
  type Credits,
  type EntryId,
  type PlanBundle,
  type Program,
  type ProgramId,
  type Report,
  type RuleId,
  type TermId,
} from '../domain';
import {engine} from '../engine';
import {courseInfo} from '../engine/interim/filter';
import {previewLine} from './labels';
import type {IslandPreview} from './TermIsland';
import {locateEntry, type PlanAction} from './usePlan';

const LIFT_THRESHOLD_PX = 4;
/** A touch must hold this long before it lifts, so a scroll is not a drag. */
const LONG_PRESS_MS = 300;

/** What is being dragged: a board card, or a course from the tray or a chip. */
export type DragPayload =
  | {kind: 'card'; entry: EntryId; course: CourseCode; credits: Credits}
  | {kind: 'course'; course: CourseCode; credits: Credits; fills?: RuleId[]};

/** Where the pointer is. Keys are strings so a DOM lookup can produce them. */
export type DropTarget =
  | {kind: 'term'; term: TermId; slot: number}
  | {kind: 'tray'}
  | {kind: 'rule'; program: ProgramId; rule: RuleId};

export function termTargetKey(term: TermId): string {
  return `term:${term}`;
}
export const TRAY_TARGET_KEY = 'tray';
export function ruleTargetKey(program: ProgramId, rule: RuleId): string {
  return `rule:${program}:${rule}`;
}

function parseTargetKey(key: string, slot: number): DropTarget | undefined {
  if (key === TRAY_TARGET_KEY) {
    return {kind: 'tray'};
  }
  if (key.startsWith('term:')) {
    return {kind: 'term', term: key.slice(5) as TermId, slot};
  }
  if (key.startsWith('rule:')) {
    const [, program, rule] = key.split(':');
    if (program !== undefined && rule !== undefined) {
      return {
        kind: 'rule',
        program: program as ProgramId,
        rule: rule as RuleId,
      };
    }
  }
  return undefined;
}

export type DragState = {
  payload: DragPayload;
  x: number;
  y: number;
  hovered?: DropTarget;
  previews: Map<TermId, IslandPreview>;
  raisedRules: Set<RuleId>;
  /** Lifted with the keyboard: arrows move the slot, Enter drops, Escape restores. */
  keyboard: boolean;
};

type Pending = {
  payload: DragPayload;
  startX: number;
  startY: number;
  /** Touch: lift only after the long-press timer fires. */
  armed: boolean;
};

export type BoardDrag = {
  state: DragState | undefined;
  /** Attach to a board row; starts a card drag. */
  onRowPointerDown: (
    entry: EntryId,
    event: ReactPointerEvent<HTMLElement>,
  ) => void;
  /** Attach to a tray item or suggestion chip; starts a course drag. */
  onCoursePointerDown: (
    course: CourseCode,
    credits: Credits,
    fills: RuleId[] | undefined,
    event: ReactPointerEvent<HTMLElement>,
  ) => void;
  registerTarget: (key: string, element: HTMLElement | null) => void;
  /** Drop programmatically, for the keyboard and menu paths. */
  dropOn: (payload: DragPayload, target: DropTarget) => void;
  /** Space on a focused row: lift it in place and let the arrow keys move it. */
  liftByKeyboard: (entry: EntryId) => void;
  cancel: () => void;
};

type DragCallbacks = {
  /** A board card dropped on the tray: bookmark it. Removal is done here. */
  onPark: (course: CourseCode) => void;
};

function slotIndexIn(island: HTMLElement, y: number, skip?: EntryId): number {
  const rows = [...island.querySelectorAll<HTMLElement>('[data-entry]')].filter(
    row => row.dataset['entry'] !== skip,
  );
  let index = 0;
  for (const row of rows) {
    const rect = row.getBoundingClientRect();
    if (y > rect.top + rect.height / 2) {
      index += 1;
    }
  }
  return index;
}

export function useBoardDrag(
  bundle: PlanBundle,
  report: Report,
  dispatch: (action: PlanAction) => void,
  callbacks: DragCallbacks,
): BoardDrag {
  const [state, setState] = useState<DragState | undefined>(undefined);
  const pending = useRef<Pending | undefined>(undefined);
  const targets = useRef(new Map<string, HTMLElement>());
  const stateRef = useRef(state);
  useEffect(() => {
    stateRef.current = state;
  }, [state]);

  const registerTarget = useCallback(
    (key: string, element: HTMLElement | null) => {
      if (element === null) {
        targets.current.delete(key);
      } else {
        targets.current.set(key, element);
      }
    },
    [],
  );

  const lift = useCallback(
    (
      payload: DragPayload,
      x: number,
      y: number,
      keyboard = false,
      hovered?: DropTarget,
    ) => {
      const moving = payload.kind === 'card' ? payload.entry : undefined;
      const rows = engine.previewPlacement(
        bundle,
        report,
        payload.course,
        moving,
        payload.credits,
      );
      const previews = new Map<TermId, IslandPreview>();
      const raised = new Set<RuleId>();
      for (const row of rows) {
        const line = previewLine(row, bundle.plan);
        previews.set(row.term, {...line, creditsAfter: row.creditsAfter});
        for (const [, rule] of row.fills) {
          raised.add(rule);
        }
      }
      // Off terms get a preview entry too, so the island can dim itself.
      for (const term of bundle.plan.terms) {
        if (isOffTerm(term.kind)) {
          previews.set(term.id, {text: '', tone: 'neutral'});
        }
      }
      setState({
        payload,
        x,
        y,
        hovered,
        previews,
        raisedRules: raised,
        keyboard,
      });
    },
    [bundle, report],
  );

  const hitTest = useCallback(
    (x: number, y: number, skip?: EntryId): DropTarget | undefined => {
      // Rule rows sit inside the sidebar and are smaller than islands; test them first.
      const entries = [...targets.current.entries()].sort(([a], [b]) => {
        const ra = a.startsWith('rule:');
        const rb = b.startsWith('rule:');
        return ra === rb ? 0 : ra ? -1 : 1;
      });
      for (const [key, element] of entries) {
        const rect = element.getBoundingClientRect();
        if (
          x >= rect.left &&
          x <= rect.right &&
          y >= rect.top &&
          y <= rect.bottom
        ) {
          return parseTargetKey(key, slotIndexIn(element, y, skip));
        }
      }
      return undefined;
    },
    [],
  );

  const dropOn = useCallback(
    (payload: DragPayload, target: DropTarget) => {
      if (target.kind === 'tray') {
        if (payload.kind === 'card') {
          dispatch({type: 'removeEntry', entry: payload.entry});
          callbacks.onPark(payload.course);
        }
        return;
      }
      if (target.kind === 'rule') {
        if (payload.kind !== 'card') {
          return;
        }
        const program: Program | undefined = bundle.programs.find(
          p => p.id === target.program,
        );
        if (program !== undefined) {
          dispatch({
            type: 'setFills',
            entry: payload.entry,
            program,
            rule: target.rule,
          });
        }
        return;
      }
      const term = bundle.plan.terms.find(t => t.id === target.term);
      if (term === undefined || isOffTerm(term.kind)) {
        return;
      }
      if (isRiceTerm(term.kind)) {
        if (payload.kind === 'card') {
          dispatch({
            type: 'moveCourse',
            entry: payload.entry,
            to: target.term,
            index: target.slot,
          });
        } else {
          dispatch({
            type: 'addCourse',
            to: target.term,
            index: target.slot,
            course: {
              id: newEntryId(),
              course: payload.course,
              credits: payload.credits,
              fills: payload.fills ?? [],
            },
          });
        }
        return;
      }
      if (isAwayTerm(term.kind)) {
        const title = courseInfo(bundle.facts, payload.course)?.title ?? '';
        let fills: RuleId[] =
          payload.kind === 'course' ? (payload.fills ?? []) : [];
        if (payload.kind === 'card') {
          const located = locateEntry(bundle.plan, payload.entry);
          if (located?.where === 'rice') {
            fills = located.course.fills;
          }
          dispatch({type: 'removeEntry', entry: payload.entry});
        }
        dispatch({
          type: 'addManualCard',
          to: target.term,
          card: {
            id: newEntryId(),
            origin: originForLabel(term.label),
            code: `${payload.course.subject} ${payload.course.number}`,
            title,
            credits: payload.credits,
            riceEquivalent: payload.course,
            fills,
          },
        });
      }
    },
    [bundle, dispatch, callbacks],
  );

  const cancel = useCallback(() => {
    pending.current = undefined;
    setState(undefined);
  }, []);

  useEffect(() => {
    const onMove = (event: PointerEvent): void => {
      const p = pending.current;
      const current = stateRef.current;
      if (p !== undefined && current === undefined) {
        const dx = event.clientX - p.startX;
        const dy = event.clientY - p.startY;
        if (Math.hypot(dx, dy) >= LIFT_THRESHOLD_PX) {
          if (p.armed) {
            lift(p.payload, event.clientX, event.clientY);
          } else {
            // Moved before the long press: it was a scroll.
            pending.current = undefined;
          }
        }
        return;
      }
      if (current === undefined || current.keyboard) {
        return;
      }
      const skip =
        current.payload.kind === 'card' ? current.payload.entry : undefined;
      setState({
        ...current,
        x: event.clientX,
        y: event.clientY,
        hovered: hitTest(event.clientX, event.clientY, skip),
      });
    };
    const onUp = (): void => {
      const current = stateRef.current;
      pending.current = undefined;
      if (current !== undefined && !current.keyboard) {
        if (current.hovered !== undefined) {
          dropOn(current.payload, current.hovered);
        }
        setState(undefined);
      }
    };
    const onKey = (event: KeyboardEvent): void => {
      const current = stateRef.current;
      if (current === undefined) {
        return;
      }
      if (event.key === 'Escape') {
        cancel();
        return;
      }
      if (!current.keyboard) {
        return;
      }
      const hovered = current.hovered;
      if (hovered === undefined || hovered.kind !== 'term') {
        return;
      }
      const terms = bundle.plan.terms.filter(t => !isOffTerm(t.kind));
      const at = terms.findIndex(t => t.id === hovered.term);
      const skip =
        current.payload.kind === 'card' ? current.payload.entry : undefined;
      const slotsIn = (term: TermId): number => {
        const el = targets.current.get(termTargetKey(term));
        return el === undefined
          ? 0
          : el.querySelectorAll('[data-entry]').length -
              (skip !== undefined && el.querySelector(`[data-entry="${skip}"]`)
                ? 1
                : 0);
      };
      let next: DropTarget | undefined;
      if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
        const step = event.key === 'ArrowRight' ? 1 : -1;
        const target = terms[at + step];
        if (target !== undefined) {
          next = {kind: 'term', term: target.id, slot: slotsIn(target.id)};
        }
      } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
        const step = event.key === 'ArrowDown' ? 1 : -1;
        const max = slotsIn(hovered.term);
        next = {
          kind: 'term',
          term: hovered.term,
          slot: Math.max(0, Math.min(max, hovered.slot + step)),
        };
      } else if (event.key === 'Enter' || event.key === ' ') {
        event.preventDefault();
        dropOn(current.payload, hovered);
        setState(undefined);
        return;
      }
      if (next !== undefined) {
        event.preventDefault();
        setState({...current, hovered: next});
      }
    };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    window.addEventListener('pointercancel', onUp);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      window.removeEventListener('pointercancel', onUp);
      window.removeEventListener('keydown', onKey);
    };
  }, [lift, hitTest, dropOn, cancel, bundle.plan.terms]);

  const begin = useCallback(
    (payload: DragPayload, event: ReactPointerEvent<HTMLElement>) => {
      const touch = event.pointerType === 'touch';
      pending.current = {
        payload,
        startX: event.clientX,
        startY: event.clientY,
        armed: !touch,
      };
      if (touch) {
        const started = pending.current;
        setTimeout(() => {
          if (pending.current === started && stateRef.current === undefined) {
            started.armed = true;
            lift(started.payload, started.startX, started.startY);
          }
        }, LONG_PRESS_MS);
      } else {
        event.preventDefault();
      }
    },
    [lift],
  );

  const onRowPointerDown = useCallback(
    (entry: EntryId, event: ReactPointerEvent<HTMLElement>) => {
      if (event.button !== 0) {
        return;
      }
      const located = locateEntry(bundle.plan, entry);
      if (located === undefined || located.where !== 'rice') {
        return;
      }
      begin(
        {
          kind: 'card',
          entry,
          course: located.course.course,
          credits: located.course.credits,
        },
        event,
      );
    },
    [bundle.plan, begin],
  );

  const onCoursePointerDown = useCallback(
    (
      course: CourseCode,
      credits: Credits,
      fills: RuleId[] | undefined,
      event: ReactPointerEvent<HTMLElement>,
    ) => {
      if (event.button !== 0) {
        return;
      }
      begin({kind: 'course', course, credits, fills}, event);
    },
    [begin],
  );

  const liftByKeyboard = useCallback(
    (entry: EntryId) => {
      const located = locateEntry(bundle.plan, entry);
      if (located === undefined || located.where !== 'rice') {
        return;
      }
      const term = bundle.plan.terms.find(t => t.id === located.term);
      const slot =
        term !== undefined && isRiceTerm(term.kind)
          ? term.kind.rice.courses.findIndex(c => c.id === entry)
          : 0;
      const el = targets.current.get(termTargetKey(located.term));
      const rect = el?.getBoundingClientRect();
      lift(
        {
          kind: 'card',
          entry,
          course: located.course.course,
          credits: located.course.credits,
        },
        rect === undefined ? 0 : rect.left + 24,
        rect === undefined ? 0 : rect.top + 24,
        true,
        {kind: 'term', term: located.term, slot: Math.max(0, slot)},
      );
    },
    [bundle.plan, lift],
  );

  return useMemo(
    () => ({
      state,
      onRowPointerDown,
      onCoursePointerDown,
      registerTarget,
      dropOn,
      liftByKeyboard,
      cancel,
    }),
    [
      state,
      onRowPointerDown,
      onCoursePointerDown,
      registerTarget,
      dropOn,
      liftByKeyboard,
      cancel,
    ],
  );
}
