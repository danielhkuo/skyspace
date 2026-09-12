import {Button} from '@astryxdesign/core/Button';
import {Icon} from '@astryxdesign/core/Icon';
import {Section} from '@astryxdesign/core/Section';
import {Stack, StackItem} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import type {PointerEvent as ReactPointerEvent} from 'react';

import {
  courseInfo,
  courseKey,
  creditRangeMin,
  formatCourseCode,
  formatCredits,
  isRiceTerm,
  shortTermLabel,
  type CourseCode,
  type Credits,
  type PlanBundle,
  type RequirementId,
} from '../domain';
import {GripMark} from './marks';
import {
  grabbable,
  islandHead,
  islandHover,
  rowDivider,
  rowStyle,
  subjectHue,
} from './paint';
import {TRAY_TARGET_KEY} from './useBoardDrag';

export const TRAY_WIDTH = 280;
const TAB_WIDTH = 40;

type FavoritesTrayProps = {
  bundle: PlanBundle;
  favorites: CourseCode[];
  isOpen: boolean;
  onToggle: () => void;
  isDropHovered: boolean;
  isDragging: boolean;
  onCoursePointerDown: (
    course: CourseCode,
    credits: Credits,
    fills: RequirementId[] | undefined,
    event: ReactPointerEvent<HTMLElement>,
  ) => void;
  registerTarget: (key: string, element: HTMLElement | null) => void;
  onAddTo: (course: CourseCode) => void;
};

/**
 * Starred courses, one flat list. Drag one onto a term; drop a board card
 * here to take it off the plan and star it. Dragging out never unstars.
 */
export function FavoritesTray({
  bundle,
  favorites,
  isOpen,
  onToggle,
  isDropHovered,
  isDragging,
  onCoursePointerDown,
  registerTarget,
  onAddTo,
}: FavoritesTrayProps) {
  if (!isOpen) {
    return (
      <Section
        variant="section"
        dividers={['end']}
        padding={0}
        width={TAB_WIDTH}
        height="100%"
      >
        <Stack height="100%" vAlign="start" align="center" paddingBlock={2}>
          <Button
            label="Favorites"
            variant="ghost"
            size="sm"
            isIconOnly
            icon={<Icon icon="chevronsRight" size="sm" />}
            onClick={onToggle}
            tooltip="Show favorites"
          />
        </Stack>
      </Section>
    );
  }

  const inPlan = new Map<string, string>();
  for (const term of bundle.plan.terms) {
    if (isRiceTerm(term.kind)) {
      for (const c of term.kind.rice.courses) {
        inPlan.set(courseKey(c.course), shortTermLabel(term.position));
      }
    }
  }

  return (
    <Section
      variant="section"
      dividers={['end']}
      padding={0}
      width={TRAY_WIDTH}
      height="100%"
      ref={el => registerTarget(TRAY_TARGET_KEY, el)}
      style={isDropHovered ? islandHover : undefined}
    >
      <Stack width="100%" height="100%" gap={0} isScrollable>
        <Stack
          direction="horizontal"
          width="100%"
          padding={2}
          hAlign="between"
          vAlign="center"
          style={islandHead}
        >
          <Text as="p" type="label" weight="semibold">
            {isDragging ? 'Drop here to remove from plan' : 'Favorites'}
          </Text>
          <Button
            label="Collapse favorites"
            variant="ghost"
            size="sm"
            isIconOnly
            icon={<Icon icon="chevronsLeft" size="sm" />}
            onClick={onToggle}
          />
        </Stack>

        {favorites.map(course => {
          const info = courseInfo(bundle.facts, course);
          const credits = info === undefined ? 0 : creditRangeMin(info.credits);
          const term = inPlan.get(courseKey(course));
          return (
            <Stack
              key={courseKey(course)}
              direction="horizontal"
              width="100%"
              paddingInline={2}
              paddingBlock={1.5}
              gap={1.5}
              vAlign="center"
              style={{...rowStyle(subjectHue(course.subject)), ...grabbable}}
              onPointerDown={e =>
                onCoursePointerDown(course, credits, undefined, e)
              }
            >
              <Icon
                icon={GripMark}
                size="sm"
                color="secondary"
                label="Drag handle"
              />
              <StackItem size="fill">
                <Stack width="100%" gap={0} align="start">
                  <Text
                    size="sm"
                    weight="semibold"
                    hasTabularNumbers
                    textWrap="nowrap"
                  >
                    {formatCourseCode(course)}
                  </Text>
                  <Text type="supporting" maxLines={1}>
                    {info?.title ?? ''}
                  </Text>
                  <Text type="supporting" size="xsm">
                    {formatCredits(credits)} cr
                    {term === undefined ? '' : ` · in ${term}`}
                  </Text>
                </Stack>
              </StackItem>
              <Button
                label="Add"
                variant="ghost"
                size="sm"
                tooltip="Add to a term"
                onClick={() => onAddTo(course)}
              />
            </Stack>
          );
        })}

        {favorites.length === 0 && (
          <Stack width="100%" padding={2}>
            <Text type="supporting">No favorites yet.</Text>
          </Stack>
        )}

        <Stack width="100%" gap={1} padding={2} style={rowDivider}>
          <Text type="supporting">
            Star courses in the catalog to see them here.
          </Text>
        </Stack>
      </Stack>
    </Section>
  );
}
