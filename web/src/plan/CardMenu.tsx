import {MoreMenu} from '@astryxdesign/core/MoreMenu';
import {useState} from 'react';

import {
  isOffTerm,
  isRiceTerm,
  shortTermLabel,
  type CourseCode,
  type Credits,
  type EntryId,
  type PlacementPreview,
  type PlanBundle,
  type Report,
  type TermId,
} from '../domain';
import {engine} from '../engine';
import {previewLine} from './labels';
import type {DragPayload, DropTarget} from './useBoardDrag';
import type {PlanAction} from './usePlan';

type CardMenuProps = {
  entry: EntryId;
  course: CourseCode;
  credits: Credits;
  currentTerm: TermId;
  bundle: PlanBundle;
  report: Report;
  dispatch: (action: PlanAction) => void;
  dropOn: (payload: DragPayload, target: DropTarget) => void;
  onPark: (course: CourseCode) => void;
  onChooseRule: () => void;
};

/**
 * The non-drag path (`design-prompt-dnd.md` 5d): every drop target a card can
 * reach by pointer is reachable here, with the same preview lines.
 */
export function CardMenu({
  entry,
  course,
  credits,
  currentTerm,
  bundle,
  report,
  dispatch,
  dropOn,
  onPark,
  onChooseRule,
}: CardMenuProps) {
  const [previews, setPreviews] = useState<Map<TermId, PlacementPreview>>(
    new Map(),
  );
  const payload: DragPayload = {kind: 'card', entry, course, credits};

  const onOpenChange = (open: boolean): void => {
    if (!open) {
      return;
    }
    const rows = engine.previewPlacement(
      bundle,
      report,
      course,
      entry,
      credits,
    );
    setPreviews(new Map(rows.map(r => [r.term, r])));
  };

  const moveItems = bundle.plan.terms
    .filter(t => t.id !== currentTerm && !isOffTerm(t.kind))
    .map(term => {
      const preview = previews.get(term.id);
      const line =
        preview === undefined
          ? undefined
          : isRiceTerm(term.kind)
            ? previewLine(preview, bundle.plan).text
            : 'becomes a manual card';
      const slot = isRiceTerm(term.kind) ? term.kind.rice.courses.length : 0;
      return {
        id: `move-${term.id}`,
        label: shortTermLabel(term.position),
        description: line,
        onClick: () => dropOn(payload, {kind: 'term', term: term.id, slot}),
      };
    });

  return (
    <MoreMenu
      label={`Options for ${course.subject} ${course.number}`}
      size="sm"
      alignment="end"
      onOpenChange={onOpenChange}
      items={[
        {type: 'section', title: 'Move to…', id: 'move', items: moveItems},
        {type: 'divider'},
        {
          id: 'fill',
          label: 'Which rule it fills…',
          description: 'Pin it to a rule, or let Skyspace decide',
          onClick: onChooseRule,
        },
        {
          id: 'park',
          label: 'Park in Saved',
          description: 'Removes it from the plan and bookmarks it',
          onClick: () => dropOn(payload, {kind: 'tray'}),
        },
        {
          id: 'remove',
          label: 'Remove from plan',
          variant: 'destructive',
          onClick: () => {
            void onPark;
            dispatch({type: 'removeEntry', entry});
          },
        },
      ]}
    />
  );
}
