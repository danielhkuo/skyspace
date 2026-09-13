import {Collapsible} from '@astryxdesign/core/Collapsible';
import {Divider} from '@astryxdesign/core/Divider';
import {Icon} from '@astryxdesign/core/Icon';
import {Button} from '@astryxdesign/core/Button';
import {DropdownMenu} from '@astryxdesign/core/DropdownMenu';
import {Link} from '@astryxdesign/core/Link';
import {ProgressBar} from '@astryxdesign/core/ProgressBar';
import {Stack, StackItem} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {Token} from '@astryxdesign/core/Token';
import {createContext, useContext, useState, type ReactNode} from 'react';

import {
  findRequirement,
  formatCatalogYear,
  formatCourseCode,
  shortTermLabel,
  type EntryId,
  type Plan,
  type Program,
  type ProgramReport,
  type Report,
  type RequirementId,
  type RequirementReport,
} from '../domain';
import {SELF_CHECK_REASON_LABEL} from './labels';
import {HalfMark, HollowMark} from './marks';
import {
  amberInk,
  requirementRow,
  requirementRowRaised,
  selfCheckRow,
  violetInk,
} from './paint';
import {locateEntry, type PlanAction} from './usePlan';

type SidebarProps = {
  plan: Plan;
  programs: Program[];
  report: Report;
  dispatch: (action: PlanAction) => void;
  /** Requirements whose progress would rise if the lifted card landed; painted with the accent wash. */
  raisedRequirements?: Set<RequirementId>;
  /** Slot for the suggestion strip under an unmet requirement (step 12). */
  renderSuggestions?: (
    program: Program,
    requirement: RequirementReport,
  ) => ReactNode;
  /** Slot for a drop-target wrapper around a requirement row (step 12). */
  wrapRequirement?: (
    program: Program,
    requirement: RequirementReport,
    row: ReactNode,
  ) => ReactNode;
  /** Open the self-check dialog for a requirement (`Plan 6 Requirement Choice`). */
  onOpenSelfCheck?: (program: Program, requirement: RequirementReport) => void;
  /** Open "Which requirement does this fill?" for a card. */
  onEditCourse?: (entry: EntryId) => void;
};

type SidebarActions = Pick<SidebarProps, 'onOpenSelfCheck' | 'onEditCourse'>;

/** The two dialog openers, so deep rows need no prop threading. */
const Actions = createContext<SidebarActions>({});

function StatusMark({report}: {report: RequirementReport}) {
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

/** "change" on a filled requirement: open the requirement chooser for the card, or pick which card first. */
function ChangeLink({plan, entries}: {plan: Plan; entries: EntryId[]}) {
  const {onEditCourse} = useContext(Actions);
  if (onEditCourse === undefined) {
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
          onEditCourse(only);
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
        return {id: entry, label, onClick: () => onEditCourse(entry)};
      })}
    />
  );
}

/** Where a card sits, for the expanded rows: "Fall 2024", or "Incoming credit". */
function entryTerm(plan: Plan, entry: EntryId): string {
  const located = locateEntry(plan, entry);
  if (located === undefined || located.where === 'incoming') {
    return 'Incoming credit';
  }
  const term = plan.terms.find(t => t.id === located.term);
  return term === undefined ? '' : shortTermLabel(term.position);
}

/**
 * A leaf course requirement, or a run of sibling slots, as exactly one line:
 * mark, label, the codes filling it, and the count. Everything else (one row
 * per card, the change link, suggestions) lives behind the chevron, so the
 * list reads the same whether a requirement holds one card or eight.
 */
function LeafRow({
  plan,
  program,
  requirements,
  label,
  raised,
  renderSuggestions,
  wrapRequirement,
}: {
  plan: Plan;
  program: Program;
  requirements: RequirementReport[];
  label: string;
  raised: boolean;
  renderSuggestions?: SidebarProps['renderSuggestions'];
  wrapRequirement?: SidebarProps['wrapRequirement'];
}) {
  const [open, setOpen] = useState(false);
  const first = requirements[0];
  if (first === undefined) {
    return null;
  }
  const filled = requirements.flatMap(r => r.filledBy);
  const claimed = new Set(requirements.flatMap(r => r.claimedBy));
  const total = requirements.reduce(
    (n, r) => n + r.progress.requirementsCheckable,
    0,
  );
  const met = requirements.reduce((n, r) => n + r.progress.requirementsMet, 0);
  const outcome: RequirementReport = {
    ...first,
    outcome:
      met >= total
        ? {outcome: 'met'}
        : filled.length > 0
          ? {outcome: 'partial'}
          : {outcome: 'unmet'},
  };
  const requirement = findRequirement(program, first.requirement);
  const isSelectLike =
    requirement?.body.kind === 'course' &&
    requirement.body.filter.include.length > 1;
  const unmet = outcome.outcome.outcome !== 'met';
  const summary = filled
    .map(e => {
      const {label: code} = entryLabel(plan, e);
      return claimed.has(e) ? `${code}*` : code;
    })
    .join(', ');
  // The chevron has something to show when there are cards, a choice to make,
  // or suggestions to offer; a met single-code requirement is just a line.
  const expandable =
    filled.length > 0 || (unmet && renderSuggestions !== undefined);

  const row = (
    <Stack
      direction="horizontal"
      width="100%"
      gap={1.5}
      vAlign="center"
      paddingInline={1.5}
      paddingBlock={1}
      style={{
        ...(raised ? requirementRowRaised : requirementRow),
        cursor: expandable ? 'pointer' : undefined,
      }}
      onClick={expandable ? () => setOpen(v => !v) : undefined}
    >
      <StatusMark report={outcome} />
      <StackItem size="fill">
        <Stack direction="horizontal" gap={1.5} vAlign="center" width="100%">
          <Text size="sm" maxLines={1}>
            {label}
          </Text>
          <Text type="supporting" maxLines={1}>
            {summary}
          </Text>
        </Stack>
      </StackItem>
      {total > 1 && (
        <Text type="supporting" hasTabularNumbers textWrap="nowrap">
          {raised
            ? `${met} of ${total} → ${Math.min(met + 1, total)} of ${total}`
            : `${met} of ${total}`}
        </Text>
      )}
      {expandable && (
        <Icon
          icon={open ? 'chevronDown' : 'chevronRight'}
          size="sm"
          color="secondary"
          label={open ? 'Collapse' : 'Expand'}
        />
      )}
    </Stack>
  );
  return (
    <Stack width="100%" gap={0}>
      {wrapRequirement === undefined
        ? row
        : wrapRequirement(program, first, row)}
      {open && (
        <Stack width="100%" gap={0} paddingInlineStart={3} paddingBlock={0.5}>
          {filled.map(entry => {
            const {label: code, manual} = entryLabel(plan, entry);
            const bySayso = claimed.has(entry);
            return (
              <Stack
                key={entry}
                direction="horizontal"
                width="100%"
                gap={1.5}
                vAlign="center"
                paddingBlock={0.5}
              >
                <Text
                  size="sm"
                  weight="semibold"
                  hasTabularNumbers
                  textWrap="nowrap"
                >
                  {code}
                </Text>
                <Text type="supporting" textWrap="nowrap">
                  {entryTerm(plan, entry)}
                </Text>
                {(bySayso || manual) && (
                  <Token
                    label={bySayso ? 'on your say-so' : 'your choice'}
                    size="sm"
                    color="purple"
                    description={
                      bySayso
                        ? 'Pinned here by you; Skyspace cannot verify it and does not count it as met'
                        : undefined
                    }
                  />
                )}
              </Stack>
            );
          })}
          {isSelectLike && filled.length > 0 && (
            <Stack direction="horizontal" width="100%" paddingBlock={0.5}>
              <ChangeLink plan={plan} entries={filled} />
            </Stack>
          )}
          {unmet && renderSuggestions?.(program, first)}
        </Stack>
      )}
    </Stack>
  );
}

/** One line: violet mark, the requirement's name, and a button into the dialog that holds the full text. */
function SelfCheckRow({
  requirement,
  program,
  dispatch,
}: {
  requirement: RequirementReport;
  program: Program;
  dispatch: SidebarProps['dispatch'];
}) {
  const {onOpenSelfCheck} = useContext(Actions);
  const confirmed =
    requirement.outcome.outcome === 'needsStudentCheck' &&
    requirement.outcome.confirmed !== undefined;
  return (
    <Stack
      direction="horizontal"
      width="100%"
      gap={1.5}
      vAlign="center"
      paddingInline={1.5}
      paddingBlock={1}
      style={selfCheckRow}
    >
      <Icon
        icon={HollowMark}
        size="sm"
        label={confirmed ? 'Confirmed by you' : 'Check this yourself'}
        style={violetInk}
      />
      <StackItem size="fill">
        <Text size="sm" maxLines={1}>
          {requirement.label}
        </Text>
      </StackItem>
      {confirmed ? (
        <>
          <Text type="supporting" textWrap="nowrap" style={violetInk}>
            Confirmed ·{' '}
            {requirement.outcome.outcome === 'needsStudentCheck' &&
            requirement.outcome.confirmed !== undefined
              ? SELF_CHECK_REASON_LABEL[requirement.outcome.confirmed]
              : ''}
          </Text>
          <Link
            href="#"
            size="sm"
            onClick={e => {
              e.preventDefault();
              onOpenSelfCheck?.(program, requirement);
            }}
          >
            edit
          </Link>
          <Link
            href="#"
            size="sm"
            onClick={e => {
              e.preventDefault();
              dispatch({
                type: 'clearSelfCheck',
                requirement: requirement.requirement,
              });
            }}
          >
            clear
          </Link>
        </>
      ) : (
        <Button
          label="Check yourself"
          variant="ghost"
          size="sm"
          endContent={<Icon icon="chevronRight" size="sm" />}
          onClick={() => onOpenSelfCheck?.(program, requirement)}
        />
      )}
    </Stack>
  );
}

/** Render an `all`/`select` node's children, collapsing sibling slots with one label. */
function RequirementChildren({
  plan,
  program,
  node,
  depth,
  dispatch,
  raisedRequirements,
  renderSuggestions,
  wrapRequirement,
}: {
  plan: Plan;
  program: Program;
  node: RequirementReport;
  depth: number;
  dispatch: SidebarProps['dispatch'];
  raisedRequirements?: Set<RequirementId>;
  renderSuggestions?: SidebarProps['renderSuggestions'];
  wrapRequirement?: SidebarProps['wrapRequirement'];
}) {
  const out: ReactNode[] = [];
  const children = node.children;
  let i = 0;
  while (i < children.length) {
    const child = children[i];
    if (child === undefined) {
      break;
    }
    const source = findRequirement(program, child.requirement);
    const kind = source?.body.kind;
    if (
      kind === 'nonCourse' ||
      kind === 'unverifiable' ||
      (kind === 'distinctDepartments' &&
        child.outcome.outcome === 'needsStudentCheck')
    ) {
      out.push(
        <SelfCheckRow
          key={child.requirement}
          requirement={child}
          program={program}
          dispatch={dispatch}
        />,
      );
      i += 1;
      continue;
    }
    if (kind === 'course' || kind === 'credits') {
      // Collapse a run of leaf siblings that share a label into one row.
      const run: RequirementReport[] = [child];
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
          key={child.requirement}
          plan={plan}
          program={program}
          requirements={run}
          label={child.label}
          raised={run.some(
            r => raisedRequirements?.has(r.requirement) ?? false,
          )}
          renderSuggestions={renderSuggestions}
          wrapRequirement={wrapRequirement}
        />,
      );
      i = j;
      continue;
    }
    // An area (all/select): a heading, then its children.
    const areaCount = child.progress.requirementsCheckable;
    const areaMet = child.progress.requirementsMet;
    // A group whose children are all same-labelled slots reads as one row with a count.
    const allSlots =
      child.children.length > 0 &&
      child.children.every(c => {
        const k = findRequirement(program, c.requirement)?.body.kind;
        return c.label === child.label && k === 'course';
      });
    if (allSlots) {
      out.push(
        <LeafRow
          key={child.requirement}
          plan={plan}
          program={program}
          requirements={child.children}
          label={child.label}
          raised={child.children.some(
            r => raisedRequirements?.has(r.requirement) ?? false,
          )}
          renderSuggestions={renderSuggestions}
          wrapRequirement={wrapRequirement}
        />,
      );
    } else if (
      child.children.length > 0 &&
      child.children.every(
        c =>
          c.label === child.label ||
          ['unverifiable', 'distinctDepartments'].includes(
            findRequirement(program, c.requirement)?.body.kind ?? '',
          ),
      )
    ) {
      // Distribution Group I: three slots plus a self-check under one label.
      const slots = child.children.filter(c => c.label === child.label);
      const checks = child.children.filter(c => c.label !== child.label);
      out.push(
        <Stack key={child.requirement} width="100%" gap={0} align="start">
          <LeafRow
            plan={plan}
            program={program}
            requirements={slots}
            label={child.label}
            raised={slots.some(
              r => raisedRequirements?.has(r.requirement) ?? false,
            )}
            renderSuggestions={renderSuggestions}
            wrapRequirement={wrapRequirement}
          />
          {checks.map(check =>
            check.outcome.outcome === 'needsStudentCheck' ? (
              <SelfCheckRow
                key={check.requirement}
                requirement={check}
                program={program}
                dispatch={dispatch}
              />
            ) : (
              <Stack
                key={check.requirement}
                direction="horizontal"
                width="100%"
                gap={1.5}
                vAlign="center"
                paddingInline={1.5}
                paddingBlock={1}
                style={requirementRow}
              >
                <StatusMark report={check} />
                <Text size="sm" maxLines={1}>
                  {check.label}
                </Text>
                <Text type="supporting" textWrap="nowrap">
                  checked from Rice&apos;s department data
                </Text>
              </Stack>
            ),
          )}
        </Stack>,
      );
    } else {
      out.push(
        <Stack
          key={child.requirement}
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
          <RequirementChildren
            plan={plan}
            program={program}
            node={child}
            depth={depth + 1}
            dispatch={dispatch}
            raisedRequirements={raisedRequirements}
            renderSuggestions={renderSuggestions}
            wrapRequirement={wrapRequirement}
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
  raisedRequirements,
  renderSuggestions,
  wrapRequirement,
  defaultOpen,
}: {
  plan: Plan;
  program: Program;
  report: ProgramReport;
  dispatch: SidebarProps['dispatch'];
  raisedRequirements?: Set<RequirementId>;
  renderSuggestions?: SidebarProps['renderSuggestions'];
  wrapRequirement?: SidebarProps['wrapRequirement'];
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
            {progress.requirementsMet} of {progress.requirementsCheckable} reqs
            {progress.requirementsClaimed > 0
              ? ` · ${progress.requirementsClaimed} on your say-so`
              : ''}
          </Text>
          <Link href={program.source.url} size="sm" isExternalLink>
            source
          </Link>
        </Stack>
        {report.evaluatedWith !== report.catalogYear && (
          <Text size="sm" style={amberInk}>
            No reviewed requirements for catalog year{' '}
            {formatCatalogYear(report.catalogYear)} yet. Showing{' '}
            {formatCatalogYear(report.evaluatedWith)} instead; Rice lets you
            follow any year from matriculation to graduation, so confirm which
            one your degree audit uses.
          </Text>
        )}
        <Stack width="100%" gap={2}>
          <ProgressBar
            label="Requirements met"
            value={progress.requirementsMet}
            max={Math.max(progress.requirementsCheckable, 1)}
            variant="accent"
            hasValueLabel
            formatValueLabel={(v, m) => `${v} of ${m} requirements`}
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
        <RequirementChildren
          plan={plan}
          program={program}
          node={report.root}
          depth={0}
          dispatch={dispatch}
          raisedRequirements={raisedRequirements}
          renderSuggestions={renderSuggestions}
          wrapRequirement={wrapRequirement}
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
  raisedRequirements,
  renderSuggestions,
  wrapRequirement,
  onOpenSelfCheck,
  onEditCourse,
}: SidebarProps) {
  return (
    <Actions.Provider value={{onOpenSelfCheck, onEditCourse}}>
      <Stack
        width="100%"
        height="100%"
        gap={2}
        padding={3}
        isScrollable
        style={{overflowX: 'hidden'}}
      >
        <Stack width="100%" gap={0.5}>
          <Text as="p" size="lg" weight="semibold">
            Requirements
          </Text>
          <Text type="supporting">
            Confirm with your advisor. Requirements link to the General
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
                    No reviewed requirements for this program yet. Your courses
                    still count toward University requirements.
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
                raisedRequirements={raisedRequirements}
                renderSuggestions={renderSuggestions}
                wrapRequirement={wrapRequirement}
                defaultOpen={i < 2}
              />
            </Stack>
          );
        })}
      </Stack>
    </Actions.Provider>
  );
}
