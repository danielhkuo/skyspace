import {Collapsible} from '@astryxdesign/core/Collapsible';
import {Divider} from '@astryxdesign/core/Divider';
import {Icon} from '@astryxdesign/core/Icon';
import {Button} from '@astryxdesign/core/Button';
import {DropdownMenu} from '@astryxdesign/core/DropdownMenu';
import {Link} from '@astryxdesign/core/Link';
import {ProgressBar} from '@astryxdesign/core/ProgressBar';
import {Section} from '@astryxdesign/core/Section';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {Token} from '@astryxdesign/core/Token';
import {createContext, useContext, type ReactNode} from 'react';

import {
  findRule,
  formatCatalogYear,
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
import {SELF_CHECK_REASON_LABEL} from './labels';
import {HalfMark, HollowMark} from './marks';
import {
  amberInk,
  ruleRow,
  ruleRowRaised,
  selfCheckRow,
  violetInk,
} from './paint';
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
  /** Open the self-check dialog for a rule (`Plan 6 Rule Choice`). */
  onOpenSelfCheck?: (program: Program, rule: RuleReport) => void;
  /** Open "Which rule does this fill?" for a card. */
  onChooseRule?: (entry: EntryId) => void;
};

type SidebarActions = Pick<SidebarProps, 'onOpenSelfCheck' | 'onChooseRule'>;

/** The two dialog openers, so deep rows need no prop threading. */
const Actions = createContext<SidebarActions>({});

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

function FilledTokens({
  plan,
  entries,
  claimed,
}: {
  plan: Plan;
  entries: EntryId[];
  claimed?: EntryId[];
}) {
  return (
    <>
      {entries.map(entry => {
        const {label, manual} = entryLabel(plan, entry);
        const bySayso = claimed?.includes(entry) ?? false;
        return (
          <Token
            key={entry}
            label={bySayso ? `${label} · on your say-so` : label}
            size="sm"
            color={bySayso || manual ? 'purple' : 'default'}
            description={
              bySayso
                ? 'Pinned here by you; Skyspace cannot verify it and does not count it as met'
                : undefined
            }
          />
        );
      })}
    </>
  );
}

/** "change" on a filled rule: open the rule chooser for the card, or pick which card first. */
function ChangeLink({plan, entries}: {plan: Plan; entries: EntryId[]}) {
  const {onChooseRule} = useContext(Actions);
  if (onChooseRule === undefined) {
    return null;
  }
  const only = entries[0];
  if (entries.length === 1 && only !== undefined) {
    return (
      <Link
        href="#"
        size="sm"
        onClick={e => {
          e.preventDefault();
          onChooseRule(only);
        }}
      >
        change
      </Link>
    );
  }
  return (
    <DropdownMenu
      button={{label: 'change', variant: 'ghost', size: 'sm'}}
      hasChevron
      items={entries.map(entry => {
        const located = locateEntry(plan, entry);
        const label =
          located === undefined
            ? 'Card'
            : located.where === 'rice'
              ? formatCourseCode(located.course.course)
              : located.card.code;
        return {id: entry, label, onClick: () => onChooseRule(entry)};
      })}
    />
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
  const claimed = rules.flatMap(r => r.claimedBy);
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
      paddingInline={1.5}
      paddingBlock={1}
      style={raised ? ruleRowRaised : ruleRow}
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
      <FilledTokens plan={plan} entries={filled} claimed={claimed} />
      {isSelectLike && filled.length > 0 && (
        <ChangeLink plan={plan} entries={filled} />
      )}
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
  rule,
  program,
  dispatch,
}: {
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
  const {onOpenSelfCheck} = useContext(Actions);
  const confirmed =
    rule.outcome.outcome === 'needsStudentCheck' &&
    rule.outcome.confirmed !== undefined;
  return (
    <Stack
      width="100%"
      gap={1}
      align="start"
      paddingInline={1.5}
      paddingBlock={1.5}
      style={selfCheckRow}
    >
      <Stack direction="horizontal" gap={1} vAlign="center" wrap="wrap">
        <Token label="Check this yourself" size="sm" color="purple" />
        <Text type="supporting" size="xsm">
          Skyspace can't verify it and doesn't count it
        </Text>
      </Stack>
      <Text size="sm">{text}</Text>
      {confirmed ? (
        <Stack direction="horizontal" gap={1.5} vAlign="center" wrap="wrap">
          <Text size="sm" weight="medium" style={violetInk}>
            Confirmed ·{' '}
            {rule.outcome.outcome === 'needsStudentCheck' &&
            rule.outcome.confirmed !== undefined
              ? SELF_CHECK_REASON_LABEL[rule.outcome.confirmed]
              : ''}
          </Text>
          <Link
            href="#"
            size="sm"
            onClick={e => {
              e.preventDefault();
              onOpenSelfCheck?.(program, rule);
            }}
          >
            edit
          </Link>
          <Link
            href="#"
            size="sm"
            onClick={e => {
              e.preventDefault();
              dispatch({type: 'clearSelfCheck', rule: rule.rule});
            }}
          >
            clear
          </Link>
        </Stack>
      ) : (
        <Button
          label="Mark satisfied…"
          variant="secondary"
          size="sm"
          onClick={() => onOpenSelfCheck?.(program, rule)}
        />
      )}
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
            {progress.rulesMet} of {progress.rulesCheckable} rules
            {progress.rulesClaimed > 0
              ? ` · ${progress.rulesClaimed} on your say-so`
              : ''}{' '}
            · {formatCredits(progress.creditsMet)} of{' '}
            {formatCredits(creditsRequired)} credit hours
          </Text>
          <Link href={program.source.url} size="sm" isExternalLink>
            source
          </Link>
        </Stack>
        {report.evaluatedWith !== report.catalogYear && (
          <Text size="sm" style={amberInk}>
            No reviewed rules for catalog year{' '}
            {formatCatalogYear(report.catalogYear)} yet. Showing{' '}
            {formatCatalogYear(report.evaluatedWith)} instead; Rice lets you
            follow any year from matriculation to graduation, so confirm which
            one your degree audit uses.
          </Text>
        )}
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
  onOpenSelfCheck,
  onChooseRule,
}: SidebarProps) {
  return (
    <Actions.Provider value={{onOpenSelfCheck, onChooseRule}}>
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
              Confirm with your advisor. Rules link to the General
              Announcements.
            </Text>
          </Stack>
          {plan.programs
            .filter(id => !report.programs.some(r => r.program === id))
            .map(id => {
              const program = programs.find(p => p.id === id);
              return (
                <Stack key={id} width="100%" gap={2}>
                  <Divider />
                  <Stack width="100%" gap={1.5} align="start" paddingInline={2}>
                    <Text as="h3" type="label" weight="semibold">
                      {program?.name ?? 'Program'}
                    </Text>
                    <Text size="sm">
                      No reviewed rules for this program yet. Your courses still
                      count toward University requirements.
                    </Text>
                    <Link
                      href="mailto:sugarlanddevs@gmail.com?subject=Skyspace%3A%20request%20a%20program"
                      size="sm"
                    >
                      Request this program
                    </Link>
                  </Stack>
                </Stack>
              );
            })}
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
    </Actions.Provider>
  );
}
