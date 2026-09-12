import {Icon} from '@astryxdesign/core/Icon';
import {Link} from '@astryxdesign/core/Link';
import {Stack} from '@astryxdesign/core/Stack';
import {StatusDot} from '@astryxdesign/core/StatusDot';
import {Text} from '@astryxdesign/core/Text';
import {useState} from 'react';
import type {PointerEvent as ReactPointerEvent} from 'react';

import {
  courseInfo,
  courseKey,
  creditRangeMin,
  formatCourseCode,
  isRiceTerm,
  type CourseCode,
  type Credits,
  type PlanBundle,
  type Program,
  type RuleId,
  type RuleReport,
} from '../domain';
import {engine} from '../engine';
import {GripMark} from './marks';
import {chipStyle, subjectHue} from './paint';

const CHIP_LIMIT = 4;

type RuleSuggestionsProps = {
  bundle: PlanBundle;
  program: Program;
  rule: RuleReport;
  saved: CourseCode[];
  /** Stand-in for the catalog query; the real one is `GET /api/v1/sections` per `08-board-interaction.md`. */
  catalog: CourseCode[];
  onCoursePointerDown: (
    course: CourseCode,
    credits: Credits,
    fills: RuleId[] | undefined,
    event: ReactPointerEvent<HTMLElement>,
  ) => void;
};

type Chip = {
  course: CourseCode;
  credits: Credits;
  offered: boolean;
  inPlan: boolean;
  saved: boolean;
};

/** Up to four draggable chips under an unmet rule: saved first, then offered this term. */
export function RuleSuggestions({
  bundle,
  program,
  rule,
  saved,
  catalog,
  onCoursePointerDown,
}: RuleSuggestionsProps) {
  const [expanded, setExpanded] = useState(false);

  const inPlan = new Set<string>();
  for (const term of bundle.plan.terms) {
    if (isRiceTerm(term.kind)) {
      for (const c of term.kind.rice.courses) {
        inPlan.add(courseKey(c.course));
      }
    }
  }
  const savedKeys = new Set(saved.map(courseKey));
  const matched = engine.ruleMatches(
    program,
    rule.rule,
    [...saved, ...catalog],
    bundle.facts,
  );
  const seen = new Set<string>();
  const chips: Chip[] = [];
  for (const course of matched) {
    const key = courseKey(course);
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    const info = courseInfo(bundle.facts, course);
    chips.push({
      course,
      credits: info === undefined ? 0 : creditRangeMin(info.credits),
      offered: info?.offeredNow ?? false,
      inPlan: inPlan.has(key),
      saved: savedKeys.has(key),
    });
  }
  chips.sort((a, b) => {
    if (a.saved !== b.saved) {
      return a.saved ? -1 : 1;
    }
    if (a.offered !== b.offered) {
      return a.offered ? -1 : 1;
    }
    return formatCourseCode(a.course).localeCompare(formatCourseCode(b.course));
  });
  if (chips.length === 0) {
    return null;
  }
  const shown = expanded ? chips : chips.slice(0, CHIP_LIMIT);

  return (
    <Stack
      direction="horizontal"
      width="100%"
      gap={1}
      wrap="wrap"
      vAlign="center"
      paddingInlineStart={3}
    >
      {shown.map(chip => (
        <Stack
          key={courseKey(chip.course)}
          direction="horizontal"
          gap={1}
          vAlign="center"
          paddingInline={1.5}
          paddingBlock={0.5}
          style={chipStyle(subjectHue(chip.course.subject))}
          onPointerDown={e =>
            onCoursePointerDown(chip.course, chip.credits, [rule.rule], e)
          }
        >
          <Icon
            icon={GripMark}
            size="sm"
            color="secondary"
            label="Drag handle"
          />
          <Text size="sm" hasTabularNumbers textWrap="nowrap">
            {formatCourseCode(chip.course)}
          </Text>
          {chip.inPlan ? (
            <Icon
              icon="check"
              size="xsm"
              color="success"
              label="Already in the plan"
            />
          ) : chip.offered ? (
            <StatusDot variant="success" label="Offered this term" />
          ) : (
            <Text type="supporting" size="xsm" textWrap="nowrap">
              not this term
            </Text>
          )}
        </Stack>
      ))}
      {chips.length > CHIP_LIMIT && (
        <Link
          href="#"
          size="sm"
          onClick={e => {
            e.preventDefault();
            setExpanded(v => !v);
          }}
        >
          {expanded ? 'Show fewer' : 'See all ›'}
        </Link>
      )}
    </Stack>
  );
}
