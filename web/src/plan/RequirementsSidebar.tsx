import {CheckboxInput} from '@astryxdesign/core/CheckboxInput';
import {Collapsible} from '@astryxdesign/core/Collapsible';
import {Divider} from '@astryxdesign/core/Divider';
import {Icon} from '@astryxdesign/core/Icon';
import {Link} from '@astryxdesign/core/Link';
import {ProgressBar} from '@astryxdesign/core/ProgressBar';
import {Section} from '@astryxdesign/core/Section';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {Token} from '@astryxdesign/core/Token';
import type {ReactNode} from 'react';

import {
  findRule,
  formatCourseCode,
  formatCredits,
  type EntryId,
  type Plan,
  type Program,
  type ProgramReport,
  type Report,
  type RuleId,
  type RuleReport,
} from '../domain';
import {HalfMark, HollowMark} from './marks';
import {selfCheckRow, violetInk} from './paint';
import {locateEntry, type PlanAction} from './usePlan';

type SidebarProps = {
  plan: Plan;
  programs: Program[];
  report: Report;
  dispatch: (action: PlanAction) => void;
  /** Rules whose progress would rise if the lifted card landed; painted with the accent wash. */
  raisedRules?: Set<RuleId>;
  /** Slot for the suggestion strip under an unmet rule (step 12). */
  renderSuggestions?: (program: Program, rule: RuleReport) => ReactNode;
  /** Slot for a drop-target wrapper around a rule row (step 12). */
  wrapRule?: (program: Program, rule: RuleReport, row: ReactNode) => ReactNode;
};

function StatusMark({report}: {report: RuleReport}) {
  switch (report.outcome.outcome) {
    case 'met':
      return <Icon icon="check" size="sm" color="success" label="Met" />;
    case 'partial':
      return (
        <Icon icon={HalfMark} size="sm" color="warning" label="Partly met" />
      );
    case 'unmet':
      return (
        <Icon icon={HollowMark} size="sm" color="secondary" label="Not met" />
      );
    case 'needsStudentCheck':
      return (
        <Icon
          icon={HollowMark}
          size="sm"
          label="Self-check"
          style={violetInk}
        />
      );
  }
}

function entryLabel(
  plan: Plan,
  entry: EntryId,
): {label: string; manual: boolean} {
  const located = locateEntry(plan, entry);
  if (located === undefined) {
    return {label: '?', manual: false};
  }
  if (located.where === 'rice') {
    return {label: formatCourseCode(located.course.course), manual: false};
  }
  const card = located.card;
  const suffix = card.origin === 'advancedPlacement' ? ' (AP)' : '';
  return {label: `${card.code}${suffix}`, manual: card.fills.length > 0};
}

function FilledTokens({plan, entries}: {plan: Plan; entries: EntryId[]}) {
  return (
    <>
      {entries.map(entry => {
        const {label, manual} = entryLabel(plan, entry);
        return (
          <Token
            key={entry}
            label={label}
            size="sm"
            color={manual ? 'purple' : 'default'}
          />
        );
      })}
    </>
  );
}

/** A leaf course rule, or a run of sibling slots collapsed to one row. */
function LeafRow({
  plan,
  program,
  rules,
  label,
  raised,
  renderSuggestions,
  wrapRule,
}: {
  plan: Plan;
  program: Program;
  rules: RuleReport[];
  label: string;
  raised: boolean;
  renderSuggestions?: SidebarProps['renderSuggestions'];
  wrapRule?: SidebarProps['wrapRule'];
}) {
  const first = rules[0];
  if (first === undefined) {
    return null;
  }
  const filled = rules.flatMap(r => r.filledBy);
  const total = rules.reduce((n, r) => n + r.progress.rulesCheckable, 0);
  const met = rules.reduce((n, r) => n + r.progress.rulesMet, 0);
  const outcome: RuleReport = {
    ...first,
    outcome:
      met >= total
        ? {outcome: 'met'}
        : filled.length > 0
          ? {outcome: 'partial'}
          : {outcome: 'unmet'},
  };
  const rule = findRule(program, first.rule);
  const isSelectLike =
    rule?.body.kind === 'course' && rule.body.filter.include.length > 1;
  const showCount = total > 1;
  const row = (
    <Stack
      direction="horizontal"
      width="100%"
      gap={1.5}
      vAlign="center"
      wrap="wrap"
      padding={raised ? 1 : 0}
      style={
        raised
          ? {
              background: 'var(--color-accent-muted)',
              borderRadius: 'var(--radius-inner)',
            }
          : undefined
      }
    >
      <StatusMark report={outcome} />
      <Text size="sm">{label}</Text>
      {showCount && (
        <Text type="supporting" hasTabularNumbers>
          {raised
            ? `${met} of ${total} → ${Math.min(met + 1, total)} of ${total}`
            : `${met} of ${total}`}
        </Text>
      )}
      <FilledTokens plan={plan} entries={filled} />
      {isSelectLike && filled.length > 0 && (
        <Link href="#" size="sm">
          change
        </Link>
      )}
      <Link href={first.source.url} size="sm" isExternalLink>
        source
      </Link>
    </Stack>
  );
  const unmet = outcome.outcome.outcome !== 'met';
  return (
    <Stack width="100%" gap={1}>
      {wrapRule === undefined ? row : wrapRule(program, first, row)}
      {unmet && renderSuggestions?.(program, first)}
    </Stack>
  );
}

function SelfCheckRow({
  plan,
  rule,
  program,
  dispatch,
}: {
  plan: Plan;
  rule: RuleReport;
  program: Program;
  dispatch: SidebarProps['dispatch'];
}) {
  const source = findRule(program, rule.rule);
  const text =
    source?.body.kind === 'unverifiable'
      ? source.body.text
      : source?.body.kind === 'nonCourse'
        ? source.body.description
        : rule.label;
  const confirmed =
    rule.outcome.outcome === 'needsStudentCheck' &&
    rule.outcome.confirmed !== undefined;
  return (
    <Stack
      width="100%"
      gap={0.5}
      align="start"
      paddingInlineStart={1.5}
      paddingBlock={1}
      style={selfCheckRow}
    >
      <Text size="sm" weight="medium">
        {rule.label}
      </Text>
      {text !== rule.label && <Text type="supporting">{text}</Text>}
      <CheckboxInput
        label="I've checked this"
        size="sm"
        value={confirmed}
        width="100%"
        onChange={checked =>
          dispatch(
            checked
              ? {type: 'confirmSelfCheck', rule: rule.rule, reason: 'other'}
              : {type: 'clearSelfCheck', rule: rule.rule},
          )
        }
      />
      <Text type="supporting" size="xsm" style={violetInk}>
        {plan.selfChecks.some(s => s.rule === rule.rule)
          ? 'self-check · not counted'
          : 'not counted'}
      </Text>
    </Stack>
  );
}

/** Render an `all`/`select` node's children, collapsing sibling slots with one label. */
function RuleChildren({
  plan,
  program,
  node,
  depth,
  dispatch,
  raisedRules,
  renderSuggestions,
  wrapRule,
}: {
  plan: Plan;
  program: Program;
  node: RuleReport;
  depth: number;
  dispatch: SidebarProps['dispatch'];
  raisedRules?: Set<RuleId>;
  renderSuggestions?: SidebarProps['renderSuggestions'];
  wrapRule?: SidebarProps['wrapRule'];
}) {
  const out: ReactNode[] = [];
  const children = node.children;
  let i = 0;
  while (i < children.length) {
    const child = children[i];
    if (child === undefined) {
      break;
    }
    const source = findRule(program, child.rule);
    const kind = source?.body.kind;
    if (kind === 'nonCourse' || kind === 'unverifiable') {
      out.push(
        <SelfCheckRow
          key={child.rule}
          plan={plan}
          rule={child}
          program={program}
          dispatch={dispatch}
        />,
      );
      i += 1;
      continue;
    }
    if (kind === 'course' || kind === 'credits') {
      // Collapse a run of leaf siblings that share a label into one row.
      const run: RuleReport[] = [child];
      let j = i + 1;
      while (j < children.length) {
        const next = children[j];
        if (
          next === undefined ||
          next.label !== child.label ||
          next.children.length > 0
        ) {
          break;
        }
        run.push(next);
        j += 1;
      }
      out.push(
        <LeafRow
          key={child.rule}
          plan={plan}
          program={program}
          rules={run}
          label={child.label}
          raised={run.some(r => raisedRules?.has(r.rule) ?? false)}
          renderSuggestions={renderSuggestions}
          wrapRule={wrapRule}
        />,
      );
      i = j;
      continue;
    }
    // An area (all/select): a heading, then its children.
    const areaCount = child.progress.rulesCheckable;
    const areaMet = child.progress.rulesMet;
    // A group whose children are all same-labelled slots reads as one row with a count.
    const allSlots =
      child.children.length > 0 &&
      child.children.every(c => {
        const k = findRule(program, c.rule)?.body.kind;
        return c.label === child.label && k === 'course';
      });
    if (allSlots) {
      out.push(
        <LeafRow
          key={child.rule}
          plan={plan}
          program={program}
          rules={child.children}
          label={child.label}
          raised={child.children.some(r => raisedRules?.has(r.rule) ?? false)}
          renderSuggestions={renderSuggestions}
          wrapRule={wrapRule}
        />,
      );
    } else if (
      child.children.length > 0 &&
      child.children.every(
        c =>
          c.label === child.label ||
          findRule(program, c.rule)?.body.kind === 'unverifiable',
      )
    ) {
      // Distribution Group I: three slots plus a self-check under one label.
      const slots = child.children.filter(c => c.label === child.label);
      const checks = child.children.filter(c => c.label !== child.label);
      out.push(
        <Stack
          key={child.rule}
          width="100%"
          gap={0.5}
          align="start"
          style={checks.length > 0 ? selfCheckRow : undefined}
          paddingInlineStart={checks.length > 0 ? 1.5 : 0}
          paddingBlock={checks.length > 0 ? 1 : 0}
        >
          <LeafRow
            plan={plan}
            program={program}
            rules={slots}
            label={child.label}
            raised={slots.some(r => raisedRules?.has(r.rule) ?? false)}
            renderSuggestions={renderSuggestions}
            wrapRule={wrapRule}
          />
          {checks.map(check => (
            <SelfCheckRow
              key={check.rule}
              plan={plan}
              rule={check}
              program={program}
              dispatch={dispatch}
            />
          ))}
        </Stack>,
      );
    } else {
      out.push(
        <Stack
          key={child.rule}
          width="100%"
          gap={1.5}
          paddingBlockStart={depth === 0 ? 1 : 0}
        >
          <Stack direction="horizontal" width="100%" gap={1.5} vAlign="center">
            {depth > 0 && <StatusMark report={child} />}
            <Text
              type="label"
              weight={depth === 0 ? 'semibold' : 'normal'}
              size="sm"
            >
              {child.label}
            </Text>
            {areaCount > 0 && (
              <Text type="supporting" hasTabularNumbers>
                {areaMet} of {areaCount}
              </Text>
            )}
          </Stack>
          <RuleChildren
            plan={plan}
            program={program}
            node={child}
            depth={depth + 1}
            dispatch={dispatch}
            raisedRules={raisedRules}
            renderSuggestions={renderSuggestions}
            wrapRule={wrapRule}
          />
        </Stack>,
      );
    }
    i += 1;
  }
  return <>{out}</>;
}

function ProgramGroup({
  plan,
  program,
  report,
  dispatch,
  raisedRules,
  renderSuggestions,
  wrapRule,
  defaultOpen,
}: {
  plan: Plan;
  program: Program;
  report: ProgramReport;
  dispatch: SidebarProps['dispatch'];
  raisedRules?: Set<RuleId>;
  renderSuggestions?: SidebarProps['renderSuggestions'];
  wrapRule?: SidebarProps['wrapRule'];
  defaultOpen: boolean;
}) {
  const {progress} = report;
  const creditsRequired = report.declaredCredits ?? progress.creditsRequired;
  return (
    <Collapsible trigger={program.name} defaultIsOpen={defaultOpen}>
      <Stack width="100%" gap={1.5} paddingBlock={1.5}>
        <Stack
          direction="horizontal"
          width="100%"
          hAlign="between"
          vAlign="center"
        >
          <Text type="supporting" hasTabularNumbers>
            {progress.rulesMet} of {progress.rulesCheckable} rules ·{' '}
            {formatCredits(progress.creditsMet)} of{' '}
            {formatCredits(creditsRequired)} credit hours
          </Text>
          <Link href={program.source.url} size="sm" isExternalLink>
            source
          </Link>
        </Stack>
        <Stack width="100%" gap={2}>
          <ProgressBar
            label="Rules met"
            value={progress.rulesMet}
            max={Math.max(progress.rulesCheckable, 1)}
            variant="accent"
            hasValueLabel
            formatValueLabel={(v, m) => `${v} of ${m} rules`}
          />
          <ProgressBar
            label="Credit hours"
            value={progress.creditsMet / 100}
            max={Math.max(creditsRequired / 100, 1)}
            variant="accent"
            hasValueLabel
            formatValueLabel={(v, m) => `${v} of ${m} credit hours`}
          />
        </Stack>
        <RuleChildren
          plan={plan}
          program={program}
          node={report.root}
          depth={0}
          dispatch={dispatch}
          raisedRules={raisedRules}
          renderSuggestions={renderSuggestions}
          wrapRule={wrapRule}
        />
      </Stack>
    </Collapsible>
  );
}

export function RequirementsSidebar({
  plan,
  programs,
  report,
  dispatch,
  raisedRules,
  renderSuggestions,
  wrapRule,
}: SidebarProps) {
  return (
    <Section
      variant="section"
      dividers={['start']}
      padding={0}
      width="100%"
      height="100%"
    >
      <Stack width="100%" height="100%" gap={2} padding={3} isScrollable>
        <Stack width="100%" gap={0.5}>
          <Text as="p" size="lg" weight="semibold">
            Requirements
          </Text>
          <Text type="supporting">
            Confirm with your advisor. Rules link to the General Announcements.
          </Text>
        </Stack>
        {report.programs.map((programReport, i) => {
          const program = programs.find(p => p.id === programReport.program);
          if (program === undefined) {
            return null;
          }
          return (
            <Stack key={program.id} width="100%" gap={2}>
              <Divider />
              <ProgramGroup
                plan={plan}
                program={program}
                report={programReport}
                dispatch={dispatch}
                raisedRules={raisedRules}
                renderSuggestions={renderSuggestions}
                wrapRule={wrapRule}
                defaultOpen={i < 2}
              />
            </Stack>
          );
        })}
      </Stack>
    </Section>
  );
}
