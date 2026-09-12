import {Card} from '@astryxdesign/core/Card';
import {Stack, StackItem} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import type {KeyboardEvent, PointerEvent, ReactNode} from 'react';

import {
  formatCredits,
  type CourseFacts,
  type EntryId,
  type Plan,
  type PlanTerm,
  type TermId,
  type TermPosition,
  type Warning,
} from '../domain';
import {CourseRow} from './CourseRow';
import {islandHead} from './paint';
import {TermIsland, type IslandPreview} from './TermIsland';
import {termTargetKey} from './useBoardDrag';
import type {FillsIndex} from './usePlan';

export type BoardDragView = {
  liftedEntry?: EntryId;
  hoveredTerm?: TermId;
  slotIndex?: number;
  previews: Map<TermId, IslandPreview>;
};

type BoardProps = {
  plan: Plan;
  today: TermPosition;
  facts: CourseFacts;
  fillsIndex: FillsIndex;
  warningsByEntry: Map<EntryId, Warning[]>;
  drag?: BoardDragView;
  onRowPointerDown?: (entry: EntryId, event: PointerEvent<HTMLElement>) => void;
  onRowKeyDown?: (entry: EntryId, event: KeyboardEvent<HTMLElement>) => void;
  renderMenu?: (entry: EntryId, term: TermId) => ReactNode;
  onAddCourse?: (term: TermId) => void;
  registerTarget?: (key: string, element: HTMLElement | null) => void;
  /** Rows of islands; two per row on desktop, one on tablet. */
  columns: 1 | 2;
  /** Tablet rows are 56 px so a finger can pick one up. */
  rowMinHeight?: number;
  /** Rendered above the incoming-credit island: the warnings panel. */
  header?: ReactNode;
};

/** Group terms into academic years, in board order. */
function yearRows(terms: PlanTerm[]): {label: string; terms: PlanTerm[]}[] {
  const byYear = new Map<number, PlanTerm[]>();
  for (const term of terms) {
    const list = byYear.get(term.position.academicYear) ?? [];
    list.push(term);
    byYear.set(term.position.academicYear, list);
  }
  const years = [...byYear.keys()].sort((a, b) => a - b);
  return years.map((year, i) => ({
    label: `Year ${i + 1} · ${year - 1}–${String(year).slice(2)}`,
    terms: byYear.get(year) ?? [],
  }));
}

function sumCredits(terms: PlanTerm[]): number {
  return terms.reduce((sum, t) => {
    if (typeof t.kind === 'object' && 'rice' in t.kind) {
      return sum + t.kind.rice.courses.reduce((s, c) => s + c.credits, 0);
    }
    if (typeof t.kind === 'object' && 'away' in t.kind) {
      return sum + t.kind.away.cards.reduce((s, c) => s + c.credits, 0);
    }
    return sum;
  }, 0);
}

function RowHeader({label, credits}: {label: string; credits: number}) {
  return (
    <Stack
      direction="horizontal"
      width="100%"
      hAlign="between"
      vAlign="end"
      gap={2}
    >
      <Text as="p" type="label" weight="semibold" textWrap="nowrap">
        {label}
      </Text>
      <Text type="supporting" hasTabularNumbers>
        {formatCredits(credits)} credit hours
      </Text>
    </Stack>
  );
}

export function Board({
  plan,
  today,
  facts,
  fillsIndex,
  warningsByEntry,
  drag,
  onRowPointerDown,
  onRowKeyDown,
  renderMenu,
  onAddCourse,
  registerTarget,
  columns,
  rowMinHeight,
  header,
}: BoardProps) {
  const incomingCredits = plan.incomingCredit.reduce(
    (s, c) => s + c.credits,
    0,
  );
  const rows = yearRows(plan.terms);

  const grid = (children: ReactNode[]): ReactNode => (
    <Stack
      direction="horizontal"
      width="100%"
      gap={2}
      align="start"
      wrap="wrap"
    >
      {children.map((child, i) => (
        <StackItem
          key={i}
          size="fill"
          style={{
            flexBasis: columns === 2 ? 'calc(50% - 8px)' : '100%',
            minWidth: 0,
          }}
        >
          {child}
        </StackItem>
      ))}
    </Stack>
  );

  return (
    <Stack width="100%" gap={4} padding={3}>
      {header}
      <Stack width="100%" gap={1.5}>
        <RowHeader label="Incoming credit" credits={incomingCredits} />
        {grid([
          <Card key="incoming" padding={0} width="100%" data-term="incoming">
            <Stack width="100%" gap={0}>
              <Stack
                direction="horizontal"
                width="100%"
                paddingInline={2}
                paddingBlock={1.5}
                hAlign="between"
                vAlign="center"
                style={islandHead}
              >
                <Text size="sm" weight="semibold">
                  Transfer and test credit
                </Text>
                {drag?.liftedEntry !== undefined && (
                  <Text type="supporting" textWrap="nowrap">
                    becomes a manual card
                  </Text>
                )}
                <Text type="supporting" hasTabularNumbers>
                  {formatCredits(incomingCredits)} cr
                </Text>
              </Stack>
              {plan.incomingCredit.map(card => (
                <CourseRow
                  key={card.id}
                  entry={card.id}
                  card={card}
                  facts={facts}
                  fillsIndex={fillsIndex}
                  warnings={warningsByEntry.get(card.id) ?? []}
                />
              ))}
            </Stack>
          </Card>,
        ])}
      </Stack>

      {rows.map(row => (
        <Stack key={row.label} width="100%" gap={1.5}>
          <RowHeader label={row.label} credits={sumCredits(row.terms)} />
          {grid(
            row.terms.map(term => (
              <TermIsland
                key={term.id}
                term={term}
                today={today}
                facts={facts}
                fillsIndex={fillsIndex}
                warningsByEntry={warningsByEntry}
                preview={drag?.previews.get(term.id)}
                slotIndex={
                  drag?.hoveredTerm === term.id ? drag.slotIndex : undefined
                }
                isHovered={drag?.hoveredTerm === term.id}
                liftedEntry={drag?.liftedEntry}
                onRowPointerDown={onRowPointerDown}
                onRowKeyDown={onRowKeyDown}
                renderMenu={renderMenu}
                rowMinHeight={rowMinHeight}
                onAddCourse={() => onAddCourse?.(term.id)}
                islandRef={el => registerTarget?.(termTargetKey(term.id), el)}
              />
            )),
          )}
        </Stack>
      ))}
    </Stack>
  );
}
