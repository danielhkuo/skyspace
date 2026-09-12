import {Button} from '@astryxdesign/core/Button';
import {Card} from '@astryxdesign/core/Card';
import {Icon} from '@astryxdesign/core/Icon';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {Token} from '@astryxdesign/core/Token';
import type {CSSProperties, ReactNode} from 'react';

import {
  compareTermPosition,
  formatCredits,
  isAwayTerm,
  isOffTerm,
  isRiceTerm,
  plannedCredits,
  termLabel,
  type CourseFacts,
  type EntryId,
  type ManualCourseCard,
  type PlanTerm,
  type TermId,
  type TermPosition,
  type Warning,
} from '../domain';
import {CourseRow} from './CourseRow';
import {PlusMark} from './marks';
import {
  amberInk,
  greenInk,
  islandDimmed,
  islandHead,
  islandHover,
  islandNow,
  rowDivider,
  slotRow,
} from './paint';
import type {FillsIndex} from './usePlan';

export type IslandPreview = {
  text: string;
  tone: 'amber' | 'green' | 'neutral';
  creditsAfter?: number;
};

type TermIslandProps = {
  term: PlanTerm;
  today: TermPosition;
  facts: CourseFacts;
  fillsIndex: FillsIndex;
  warningsByEntry: Map<EntryId, Warning[]>;
  /** Present while a card is in the air. */
  preview?: IslandPreview;
  /** Insertion index of the dashed slot while this island is hovered. */
  slotIndex?: number;
  isHovered?: boolean;
  liftedEntry?: EntryId;
  onRowPointerDown?: (
    entry: EntryId,
    event: React.PointerEvent<HTMLElement>,
  ) => void;
  onRowKeyDown?: (
    entry: EntryId,
    event: React.KeyboardEvent<HTMLElement>,
  ) => void;
  renderMenu?: (entry: EntryId, term: TermId) => ReactNode;
  renderManualMenu?: (
    card: ManualCourseCard,
    where: TermId | 'incoming',
  ) => ReactNode;
  rowMinHeight?: number;
  onAddCourse?: () => void;
  /** Edit term, Add term after, Remove term. */
  termMenu?: ReactNode;
  islandRef?: (element: HTMLElement | null) => void;
  extraHeader?: ReactNode;
};

const TONE: Record<IslandPreview['tone'], CSSProperties | undefined> = {
  amber: amberInk,
  green: greenInk,
  neutral: undefined,
};

/** One term column on the board: a Card with a header row and course rows. */
export function TermIsland({
  term,
  today,
  facts,
  fillsIndex,
  warningsByEntry,
  preview,
  slotIndex,
  isHovered,
  liftedEntry,
  onRowPointerDown,
  onRowKeyDown,
  renderMenu,
  renderManualMenu,
  rowMinHeight,
  onAddCourse,
  termMenu,
  islandRef,
}: TermIslandProps) {
  const completed = compareTermPosition(term.position, today) < 0;
  const isNow = compareTermPosition(term.position, today) === 0;
  const off = isOffTerm(term.kind);
  const away = isAwayTerm(term.kind);
  const dimmed = preview !== undefined && off;
  const credits = plannedCredits(term);
  const creditsText =
    preview?.creditsAfter !== undefined && preview.creditsAfter !== credits
      ? `${formatCredits(credits)} → ${formatCredits(preview.creditsAfter)} cr`
      : `${formatCredits(credits)} cr`;

  const rows: ReactNode[] = [];
  const slot = (
    <Stack
      key="slot"
      direction="horizontal"
      width="100%"
      paddingInline={2}
      paddingBlock={1.5}
      height={40}
      style={slotRow}
    />
  );

  if (isRiceTerm(term.kind)) {
    const courses = term.kind.rice.courses;
    // The slot index counts rows without the lifted card (it is still drawn,
    // dimmed, in its old place), so walk a separate index that skips it.
    let visible = 0;
    courses.forEach(course => {
      const lifted = course.id === liftedEntry;
      if (!lifted && slotIndex === visible) {
        rows.push(slot);
      }
      if (!lifted) {
        visible += 1;
      }
      rows.push(
        <CourseRow
          key={course.id}
          entry={course.id}
          card={course}
          facts={facts}
          fillsIndex={fillsIndex}
          warnings={warningsByEntry.get(course.id) ?? []}
          onPointerDown={e => onRowPointerDown?.(course.id, e)}
          onKeyDown={e => onRowKeyDown?.(course.id, e)}
          menu={renderMenu?.(course.id, term.id)}
          minHeight={rowMinHeight}
          style={liftedEntry === course.id ? {opacity: 0.4} : undefined}
        />,
      );
    });
    if (slotIndex !== undefined && slotIndex >= visible) {
      rows.push(slot);
    }
  } else if (isAwayTerm(term.kind)) {
    for (const card of term.kind.away.cards) {
      rows.push(
        <CourseRow
          key={card.id}
          entry={card.id}
          card={card}
          facts={facts}
          fillsIndex={fillsIndex}
          warnings={warningsByEntry.get(card.id) ?? []}
          menu={renderManualMenu?.(card, term.id)}
          minHeight={rowMinHeight}
        />,
      );
    }
    if (slotIndex !== undefined) {
      rows.push(slot);
    }
  }

  const empty = rows.length === 0 && !off;

  return (
    <Card
      padding={0}
      width="100%"
      variant={completed ? 'muted' : 'default'}
      ref={islandRef}
      data-term={term.id}
      style={{
        ...(isNow ? islandNow : undefined),
        ...(isHovered ? islandHover : undefined),
        ...(dimmed ? islandDimmed : undefined),
      }}
    >
      <Stack width="100%" gap={0}>
        <Stack
          width="100%"
          paddingInline={2}
          paddingBlock={1.5}
          gap={0.5}
          align="start"
          style={islandHead}
        >
          <Stack
            direction="horizontal"
            width="100%"
            hAlign="between"
            vAlign="center"
            gap={1}
          >
            <Stack direction="horizontal" gap={1} vAlign="center">
              <Text size="sm" weight="semibold">
                {termLabel(term.position)}
              </Text>
              {completed && (
                <Icon
                  icon="check"
                  size="xsm"
                  color="success"
                  label="Completed"
                />
              )}
              {isNow && <Token label="Now" size="sm" color="blue" />}
            </Stack>
            {preview !== undefined && !off && (
              <Text
                type="supporting"
                textWrap="nowrap"
                style={TONE[preview.tone]}
              >
                {away ? 'becomes a manual card' : preview.text}
              </Text>
            )}
            {preview !== undefined && off && (
              <Text type="supporting" textWrap="nowrap">
                Off terms hold no courses
              </Text>
            )}
            <Stack direction="horizontal" gap={1} vAlign="center">
              <Text type="supporting" hasTabularNumbers>
                {off ? '' : creditsText}
              </Text>
              {termMenu}
            </Stack>
          </Stack>
          {term.label !== undefined && (
            <Token
              label={`${away ? 'Away' : off ? 'Off' : 'Rice'} · ${term.label}`}
              size="sm"
              color={away ? 'orange' : 'gray'}
            />
          )}
        </Stack>
        {rows}
        {empty && slotIndex === undefined && (
          <Stack
            width="100%"
            paddingInline={2}
            paddingBlock={3}
            gap={1}
            align="center"
          >
            <Text type="supporting">Drag a course here or search</Text>
            <Button
              label="Add course"
              variant="secondary"
              size="sm"
              icon={<Icon icon={PlusMark} size="sm" />}
              onClick={onAddCourse}
            />
          </Stack>
        )}
        {!empty && !off && (
          <Stack
            direction="horizontal"
            width="100%"
            paddingInline={1}
            paddingBlock={1}
            style={rowDivider}
          >
            <Button
              label="Add course"
              variant="ghost"
              size="sm"
              width="100%"
              icon={<Icon icon={PlusMark} size="sm" />}
              onClick={onAddCourse}
            />
          </Stack>
        )}
        {off && (
          <Stack width="100%" paddingInline={2} paddingBlock={2}>
            <Text type="supporting">
              No courses. Excluded from credit pacing.
            </Text>
          </Stack>
        )}
      </Stack>
    </Card>
  );
}
