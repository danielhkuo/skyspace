import {Card} from '@astryxdesign/core/Card';
import {Icon} from '@astryxdesign/core/Icon';
import {Stack, StackItem} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';

import {formatCourseCode, formatCredits} from '../domain';
import {GripMark} from './marks';
import {liftedCard, subjectHue} from './paint';
import type {DragState} from './useBoardDrag';

const GHOST_WIDTH = 300;

/** The lifted card, following the pointer. Pointer events pass through it. */
export function DragGhost({drag, note}: {drag: DragState; note: string}) {
  const {course, credits} = drag.payload;
  return (
    <Card
      padding={0}
      width={GHOST_WIDTH}
      style={{
        ...liftedCard(subjectHue(course.subject)),
        left: drag.x - 24,
        top: drag.y - 20,
      }}
      aria-hidden
    >
      <Stack
        direction="horizontal"
        width="100%"
        paddingInline={2}
        paddingBlock={1.5}
        gap={1.5}
        vAlign="center"
      >
        <Icon icon={GripMark} size="sm" color="accent" label="" />
        <Text size="sm" weight="semibold" hasTabularNumbers textWrap="nowrap">
          {formatCourseCode(course)}
        </Text>
        <StackItem size="fill">
          <Text type="supporting" maxLines={1}>
            {note}
          </Text>
        </StackItem>
        <Text size="sm" color="secondary" hasTabularNumbers>
          {formatCredits(credits)}
        </Text>
      </Stack>
    </Card>
  );
}
