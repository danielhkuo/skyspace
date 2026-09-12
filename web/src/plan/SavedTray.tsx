import {Button} from '@astryxdesign/core/Button';
import {Icon} from '@astryxdesign/core/Icon';
import {Section} from '@astryxdesign/core/Section';
import {
  SegmentedControl,
  SegmentedControlItem,
} from '@astryxdesign/core/SegmentedControl';
import {Stack, StackItem} from '@astryxdesign/core/Stack';
import {Switch} from '@astryxdesign/core/Switch';
import {Text} from '@astryxdesign/core/Text';
import {useState} from 'react';
import type {PointerEvent as ReactPointerEvent} from 'react';

import {
  courseKey,
  creditRangeMin,
  formatCourseCode,
  formatCredits,
  isRiceTerm,
  shortTermLabel,
  type CourseCode,
  type Credits,
  type PlanBundle,
  type Report,
  type RuleId,
} from '../domain';
import {courseInfo} from '../engine/interim/filter';
import {rulesRaisedBy} from '../engine/interim/preview';
import type {SavedCollection} from '../fixtures/csStats';
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

type SavedTrayProps = {
  bundle: PlanBundle;
  report: Report;
  collections: SavedCollection[];
  isOpen: boolean;
  onToggle: () => void;
  isDropHovered: boolean;
  isDragging: boolean;
  onCoursePointerDown: (
    course: CourseCode,
    credits: Credits,
    fills: RuleId[] | undefined,
    event: ReactPointerEvent<HTMLElement>,
  ) => void;
  registerTarget: (key: string, element: HTMLElement | null) => void;
  onAddTo: (course: CourseCode) => void;
};

type Item = {
  course: CourseCode;
  title: string;
  credits: Credits;
  inTerm?: string;
  fits: string[];
};

function ruleLabel(bundle: PlanBundle, rule: RuleId): string {
  for (const program of bundle.programs) {
    const stack = [program.root];
    while (stack.length > 0) {
      const r = stack.pop();
      if (r === undefined) {
        break;
      }
      if (r.id === rule) {
        return r.label;
      }
      if (r.body.kind === 'all' || r.body.kind === 'select') {
        stack.push(...r.body.of);
      }
    }
  }
  return '';
}

function buildItems(
  bundle: PlanBundle,
  report: Report,
  courses: CourseCode[],
): Item[] {
  const inPlan = new Map<string, string>();
  for (const term of bundle.plan.terms) {
    if (isRiceTerm(term.kind)) {
      for (const c of term.kind.rice.courses) {
        inPlan.set(courseKey(c.course), shortTermLabel(term.position));
      }
    }
  }
  return courses.map(course => {
    const info = courseInfo(bundle.facts, course);
    const fits = rulesRaisedBy(bundle, report, course).map(([, rule]) =>
      ruleLabel(bundle, rule),
    );
    return {
      course,
      title: info?.title ?? '',
      credits: info === undefined ? 0 : creditRangeMin(info.credits),
      inTerm: inPlan.get(courseKey(course)),
      fits: [...new Set(fits)],
    };
  });
}

/** Left rail of bookmarked courses. Dragging out never unbookmarks; dropping a card in parks it. */
export function SavedTray({
  bundle,
  report,
  collections,
  isOpen,
  onToggle,
  isDropHovered,
  isDragging,
  onCoursePointerDown,
  registerTarget,
  onAddTo,
}: SavedTrayProps) {
  const [collection, setCollection] = useState('all');
  const [fitsOnly, setFitsOnly] = useState(false);

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
            label="Saved"
            variant="ghost"
            size="sm"
            isIconOnly
            icon={<Icon icon="chevronsRight" size="sm" />}
            onClick={onToggle}
            tooltip="Show saved courses"
          />
        </Stack>
      </Section>
    );
  }

  const codes =
    collection === 'all'
      ? collections.flatMap(c => c.courses)
      : (collections.find(c => c.id === collection)?.courses ?? []);
  const seen = new Set<string>();
  const unique = codes.filter(c => {
    const k = courseKey(c);
    if (seen.has(k)) {
      return false;
    }
    seen.add(k);
    return true;
  });
  const items = buildItems(bundle, report, unique).filter(
    item => !fitsOnly || item.fits.length > 0,
  );

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
          width="100%"
          gap={1.5}
          padding={2}
          align="start"
          style={islandHead}
        >
          <Stack
            direction="horizontal"
            width="100%"
            hAlign="between"
            vAlign="center"
          >
            <Text as="p" type="label" weight="semibold">
              {isDragging ? 'Drop here to park it' : 'Saved'}
            </Text>
            <Button
              label="Collapse tray"
              variant="ghost"
              size="sm"
              isIconOnly
              icon={<Icon icon="chevronsLeft" size="sm" />}
              onClick={onToggle}
            />
          </Stack>
          <SegmentedControl
            label="Collection"
            value={collection}
            onChange={setCollection}
            size="sm"
            layout="fill"
          >
            <SegmentedControlItem value="all" label="All" />
            {collections.map(c => (
              <SegmentedControlItem key={c.id} value={c.id} label={c.name} />
            ))}
          </SegmentedControl>
          <Switch
            label="Fits an open rule"
            size="sm"
            value={fitsOnly}
            onChange={setFitsOnly}
            labelPosition="start"
            labelSpacing="spread"
            width="100%"
          />
        </Stack>

        {items.map(item => (
          <Stack
            key={courseKey(item.course)}
            direction="horizontal"
            width="100%"
            paddingInline={2}
            paddingBlock={1.5}
            gap={1.5}
            vAlign="start"
            style={{...rowStyle(subjectHue(item.course.subject)), ...grabbable}}
            onPointerDown={e =>
              onCoursePointerDown(item.course, item.credits, undefined, e)
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
                <Stack direction="horizontal" gap={1} vAlign="center">
                  <Text
                    size="sm"
                    weight="semibold"
                    hasTabularNumbers
                    textWrap="nowrap"
                  >
                    {formatCourseCode(item.course)}
                  </Text>
                  <Text type="supporting" maxLines={1}>
                    {item.title}
                  </Text>
                </Stack>
                <Text type="supporting" size="xsm">
                  {[
                    item.inTerm === undefined ? undefined : `in ${item.inTerm}`,
                    item.fits.length > 0
                      ? `fills ${item.fits[0]}`
                      : 'fills nothing open',
                  ]
                    .filter(Boolean)
                    .join(' · ')}
                </Text>
              </Stack>
            </StackItem>
            {item.inTerm !== undefined ? (
              <Icon
                icon="check"
                size="xsm"
                color="success"
                label="In the plan"
              />
            ) : (
              <Text size="sm" color="secondary" hasTabularNumbers>
                {formatCredits(item.credits)}
              </Text>
            )}
            <Button
              label="Add to…"
              variant="ghost"
              size="sm"
              isIconOnly
              icon={<Icon icon="chevronRight" size="sm" />}
              tooltip="Add to a term"
              onClick={() => onAddTo(item.course)}
            />
          </Stack>
        ))}

        <Stack width="100%" gap={1} padding={2} style={rowDivider}>
          <Text type="supporting">
            Bookmark courses from the catalog with ☆. They stay saved across
            terms.
          </Text>
        </Stack>
      </Stack>
    </Section>
  );
}
