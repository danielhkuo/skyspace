import {MoreMenu} from '@astryxdesign/core/MoreMenu';

import {
  isAwayTerm,
  shortTermLabel,
  type EntryId,
  type ManualCourseCard,
  type Plan,
  type TermId,
} from '../domain';
import type {PlanAction} from './usePlan';

type ManualCardMenuProps = {
  card: ManualCourseCard;
  /** Where the card is now; `'incoming'` for the incoming-credit island. */
  where: TermId | 'incoming';
  plan: Plan;
  dispatch: (action: PlanAction) => void;
  onChooseRule: () => void;
};

/**
 * The ⋯ menu on an away or incoming card. These cards have no drag: they
 * move only between away terms and incoming credit, so a menu covers it.
 */
export function ManualCardMenu({
  card,
  where,
  plan,
  dispatch,
  onChooseRule,
}: ManualCardMenuProps) {
  const entry: EntryId = card.id;
  const targets = [
    ...plan.terms
      .filter(t => isAwayTerm(t.kind) && t.id !== where)
      .map(t => ({
        id: `move-${t.id}`,
        label: shortTermLabel(t.position),
        description: t.label,
        onClick: () => dispatch({type: 'moveManualCard', entry, to: t.id}),
      })),
    ...(where === 'incoming'
      ? []
      : [
          {
            id: 'move-incoming',
            label: 'Incoming credit',
            onClick: () =>
              dispatch({type: 'moveManualCard', entry, to: 'incoming'}),
          },
        ]),
  ];
  return (
    <MoreMenu
      label={`Options for ${card.code}`}
      size="sm"
      alignment="end"
      items={[
        ...(targets.length === 0
          ? []
          : [
              {
                type: 'section' as const,
                title: 'Move to…',
                id: 'move',
                items: targets,
              },
              {type: 'divider' as const},
            ]),
        {
          id: 'fill',
          label: 'Which rule it fills…',
          description: 'Pin it to a rule and say why it should count',
          onClick: onChooseRule,
        },
        {
          id: 'remove',
          label: 'Remove',
          description: 'Takes it off the plan',
          onClick: () => dispatch({type: 'removeEntry', entry}),
        },
      ]}
    />
  );
}
