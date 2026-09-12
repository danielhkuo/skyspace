import {Icon} from '@astryxdesign/core/Icon';
import {Stack, StackItem} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {Token} from '@astryxdesign/core/Token';
import type {
  CSSProperties,
  KeyboardEvent,
  PointerEvent,
  ReactNode,
} from 'react';

import {
  formatCourseCode,
  formatCredits,
  type CourseFacts,
  type EntryId,
  type ManualCourseCard,
  type PlannedCourse,
  type Warning,
} from '../domain';
import {courseInfo} from '../engine/interim/filter';
import {warningChip} from './labels';
import {GripMark, HollowMark} from './marks';
import {
  grabbable,
  rowStyle,
  selfCheckRow,
  subjectHue,
  violetInk,
  warningRow,
} from './paint';
import type {FillsIndex} from './usePlan';

type CourseRowProps = {
  entry: EntryId;
  card: PlannedCourse | ManualCourseCard;
  facts: CourseFacts;
  fillsIndex: FillsIndex;
  warnings: Warning[];
  /** Compact rows for tablet are 56 px; desktop rows size to content. */
  minHeight?: number;
  onPointerDown?: (event: PointerEvent<HTMLElement>) => void;
  onKeyDown?: (event: KeyboardEvent<HTMLElement>) => void;
  /** The ⋯ menu, rendered at the row's end. */
  menu?: ReactNode;
  style?: CSSProperties;
};

const ORIGIN_LABEL: Record<ManualCourseCard['origin'], string> = {
  transfer: 'Transfer',
  advancedPlacement: 'AP',
  internationalBaccalaureate: 'IB',
  studyAbroad: 'Study abroad',
  other: 'Other',
};

function isPlanned(
  card: PlannedCourse | ManualCourseCard,
): card is PlannedCourse {
  return 'course' in card;
}

/**
 * One course on the board: grip, subject stripe, code, title, fills line,
 * warning chip, credits. Rows, not cards (`web/AGENTS.md`).
 */
export function CourseRow({
  entry,
  card,
  facts,
  fillsIndex,
  warnings,
  minHeight,
  onPointerDown,
  onKeyDown,
  menu,
  style,
}: CourseRowProps) {
  const planned = isPlanned(card);
  const code = planned ? formatCourseCode(card.course) : card.code;
  const info = planned ? courseInfo(facts, card.course) : undefined;
  const title = planned ? info?.title : card.title;
  const fills = fillsIndex.get(entry) ?? [];
  // A manual card with no Rice equivalent fills a rule only by the student's word.
  const byChoice =
    !planned && card.riceEquivalent === undefined && card.fills.length > 0;
  const chip = warnings.map(warningChip).find(c => c !== undefined);
  const hue = planned ? subjectHue(card.course.subject) : 'gray';
  const base = planned
    ? rowStyle(hue)
    : byChoice
      ? selfCheckRow
      : rowStyle('gray');
  const paint = chip === undefined ? base : {...base, ...warningRow};

  const fillsLine = fills.map(f => f.path).join(' · ');

  return (
    <Stack
      direction="horizontal"
      width="100%"
      paddingInline={2}
      paddingBlock={1.5}
      gap={1.5}
      vAlign="start"
      style={{
        ...paint,
        ...(minHeight === undefined ? {} : {minHeight}),
        ...style,
      }}
      data-entry={entry}
      onPointerDown={onPointerDown}
      onKeyDown={onKeyDown}
      tabIndex={planned ? 0 : undefined}
      aria-label={planned ? `${code}, press Space to lift` : undefined}
    >
      {planned && (
        <Icon
          icon={GripMark}
          size="sm"
          color="secondary"
          label="Drag handle"
          style={grabbable}
        />
      )}
      <Text size="sm" weight="semibold" hasTabularNumbers textWrap="nowrap">
        {code}
      </Text>
      <StackItem size="fill">
        <Stack gap={0} align="start" width="100%">
          {title !== undefined && (
            <Stack direction="horizontal" gap={1} vAlign="center">
              <Text type="supporting" maxLines={1}>
                {title}
              </Text>
              {!planned && (
                <Token label={ORIGIN_LABEL[card.origin]} size="sm" />
              )}
            </Stack>
          )}
          {fills.length > 0 ? (
            <Text
              type="supporting"
              size="xsm"
              style={byChoice ? violetInk : undefined}
            >
              {fillsLine}
              {byChoice ? ' by your choice' : ''}
            </Text>
          ) : (
            <Stack direction="horizontal" gap={1} vAlign="center">
              <Icon
                icon={HollowMark}
                size="xsm"
                color="secondary"
                label="Fills nothing"
              />
              <Text type="supporting" size="xsm">
                fills nothing yet
              </Text>
            </Stack>
          )}
          {chip !== undefined && (
            <Stack
              direction="horizontal"
              gap={1}
              vAlign="center"
              paddingBlockStart={0.5}
            >
              <Token label={chip} size="sm" color="yellow" />
            </Stack>
          )}
        </Stack>
      </StackItem>
      <Text size="sm" color="secondary" hasTabularNumbers>
        {formatCredits(card.credits)}
      </Text>
      {menu}
    </Stack>
  );
}
