import {BottomSheet} from '@astryxdesign/core/BottomSheet';
import {Button} from '@astryxdesign/core/Button';
import {Stack, StackItem} from '@astryxdesign/core/Stack';
import {MoreMenu} from '@astryxdesign/core/MoreMenu';
import {Text} from '@astryxdesign/core/Text';
import {VisuallyHidden} from '@astryxdesign/core/VisuallyHidden';
import {useCallback, useEffect, useMemo, useState} from 'react';
import type {ReactNode} from 'react';

import {
  courseKey,
  creditRangeMin,
  formatCourseCode,
  isRiceTerm,
  shortTermLabel,
  type CourseCode,
  type EntryId,
  type ManualCourseCard,
  type CourseInfo,
  type PlanBundle,
  type PlanTerm,
  type Program,
  type RuleReport,
  type TermId,
  type Warning,
} from '../domain';
import {dataSource} from '../datasource';
import {DESKTOP_WIDTH, useViewportWidth} from '../shell/useViewportWidth';
import {AddCourseDialog} from './AddCourseDialog';
import {Board} from './Board';
import {CardMenu} from './CardMenu';
import {ManualCardMenu} from './ManualCardMenu';
import {DragGhost} from './DragGhost';
import {EditTermDialog} from './EditTermDialog';
import {ruleHit} from './paint';
import {PlanHeader} from './PlanHeader';
import {RequirementsSidebar} from './RequirementsSidebar';
import {RuleSuggestions} from './RuleSuggestions';
import {FavoritesTray} from './FavoritesTray';
import {TermPickerDialog} from './TermPickerDialog';
import {ruleTargetKey, useBoardDrag} from './useBoardDrag';
import {locateEntry, usePlan, termRemoveBlocker} from './usePlan';
import {WarningsPanel} from './WarningsPanel';

const SIDEBAR_WIDTH = 400;

function entryOf(warning: Warning): EntryId | undefined {
  switch (warning.kind) {
    case 'fillsNoRequirement':
    case 'ruleChoiceUnmatched':
    case 'ruleChoiceMissing':
    case 'attributeUnknown':
      return warning.value.entry;
    default:
      return undefined;
  }
}

/** Loads the plan from the data source, then hands it to the board. */
export function PlanPage() {
  const [loaded, setLoaded] = useState<
    | {bundle: PlanBundle; favorites: CourseCode[]; candidates: CourseCode[]}
    | undefined
  >(undefined);
  useEffect(() => {
    let cancelled = false;
    void Promise.all([
      dataSource.loadBundle(),
      dataSource.loadFavorites(),
      dataSource.catalogCandidates(),
    ]).then(([bundle, favorites, candidates]) => {
      if (!cancelled) {
        setLoaded({bundle, favorites, candidates});
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);
  if (loaded === undefined) {
    return (
      <Stack padding={4}>
        <Text type="supporting">Loading plan…</Text>
      </Stack>
    );
  }
  return (
    <PlanBoardPage
      initial={loaded.bundle}
      initialFavorites={loaded.favorites}
      catalogCandidates={loaded.candidates}
    />
  );
}

type PlanBoardPageProps = {
  initial: PlanBundle;
  initialFavorites: CourseCode[];
  catalogCandidates: CourseCode[];
};

function PlanBoardPage({
  initial,
  initialFavorites,
  catalogCandidates,
}: PlanBoardPageProps) {
  const state = usePlan(initial);
  const {bundle, report, fillsIndex, dispatch} = state;
  const width = useViewportWidth();
  const desktop = width >= DESKTOP_WIDTH;
  const [showWarnings, setShowWarnings] = useState(true);
  const [trayOpen, setTrayOpen] = useState(true);
  const [sheet, setSheet] = useState<'saved' | 'requirements' | undefined>(
    undefined,
  );
  const [favorites, setFavorites] = useState<CourseCode[]>(initialFavorites);
  useEffect(() => {
    if (favorites !== initialFavorites) {
      void dataSource.saveFavorites(favorites);
    }
  }, [favorites, initialFavorites]);
  const [addingTo, setAddingTo] = useState<TermId | undefined>(undefined);
  const [editingTerm, setEditingTerm] = useState<TermId | undefined>(undefined);
  const [picking, setPicking] = useState<
    {course: CourseCode; credits: number} | undefined
  >(undefined);

  // Parking a card stars it; a drag out of the tray never unstars.
  const park = useCallback((course: CourseCode) => {
    setFavorites(prev =>
      prev.some(c => courseKey(c) === courseKey(course))
        ? prev
        : [...prev, course],
    );
  }, []);
  const dragCallbacks = useMemo(() => ({onPark: park}), [park]);
  const drag = useBoardDrag(bundle, report, dispatch, dragCallbacks);
  const dragging = drag.state !== undefined;
  const hovered = drag.state?.hovered;
  const liftedFrom =
    drag.state?.payload.kind === 'card'
      ? bundle.plan.terms.find(
          t =>
            isRiceTerm(t.kind) &&
            t.kind.rice.courses.some(
              c =>
                drag.state?.payload.kind === 'card' &&
                c.id === drag.state.payload.entry,
            ),
        )
      : undefined;

  // Warnings keyed by the card they belong to; course-level ones resolve through the term.
  const warningsByEntry = useMemo(() => {
    const map = new Map<EntryId, Warning[]>();
    const push = (entry: EntryId, w: Warning): void =>
      void map.set(entry, [...(map.get(entry) ?? []), w]);
    for (const warning of report.warnings) {
      const direct = entryOf(warning);
      if (direct !== undefined) {
        push(direct, warning);
        continue;
      }
      if (
        warning.kind === 'prerequisite' ||
        warning.kind === 'prerequisiteUnparsed' ||
        warning.kind === 'mutuallyExclusive' ||
        warning.kind === 'duplicateCourse'
      ) {
        // Attach to the card in the term the warning names, not every card
        // with that code; a duplicate names no single term and marks each copy.
        const code =
          warning.kind === 'mutuallyExclusive'
            ? warning.value.blocked
            : warning.value.course;
        const inTerm =
          warning.kind === 'mutuallyExclusive'
            ? warning.value.blockedTerm
            : warning.kind === 'duplicateCourse'
              ? undefined
              : warning.value.term;
        for (const term of bundle.plan.terms) {
          if (inTerm !== undefined && term.id !== inTerm) {
            continue;
          }
          if (isRiceTerm(term.kind)) {
            for (const c of term.kind.rice.courses) {
              if (courseKey(c.course) === courseKey(code)) {
                push(c.id, warning);
              }
            }
          }
        }
      }
    }
    return map;
  }, [report, bundle.plan]);

  const renderSuggestions = useCallback(
    (program: Program, rule: RuleReport): ReactNode => (
      <RuleSuggestions
        bundle={bundle}
        program={program}
        rule={rule}
        saved={favorites}
        catalog={catalogCandidates}
        onCoursePointerDown={drag.onCoursePointerDown}
      />
    ),
    [bundle, favorites, catalogCandidates, drag.onCoursePointerDown],
  );

  const wrapRule = useCallback(
    (program: Program, rule: RuleReport, row: ReactNode): ReactNode => {
      const isTarget =
        hovered?.kind === 'rule' &&
        hovered.program === program.id &&
        hovered.rule === rule.rule;
      return (
        <Stack
          width="100%"
          gap={0.5}
          ref={el =>
            drag.registerTarget(ruleTargetKey(program.id, rule.rule), el)
          }
          style={isTarget ? ruleHit : undefined}
          padding={isTarget ? 1 : 0}
        >
          {row}
          {isTarget && drag.state?.payload.kind === 'card' && (
            <Stack direction="horizontal" gap={1} paddingInlineStart={3}>
              <Text size="sm" weight="medium">
                Fill this rule with{' '}
                {formatCourseCode(drag.state.payload.course)}
              </Text>
            </Stack>
          )}
        </Stack>
      );
    },
    [drag, hovered],
  );

  const renderMenu = useCallback(
    (entry: EntryId, term: TermId): ReactNode => {
      const located = locateEntry(bundle.plan, entry);
      if (located === undefined || located.where !== 'rice') {
        return null;
      }
      return (
        <CardMenu
          entry={entry}
          course={located.course.course}
          credits={located.course.credits}
          currentTerm={term}
          bundle={bundle}
          report={report}
          dispatch={dispatch}
          dropOn={drag.dropOn}
          onPark={park}
        />
      );
    },
    [bundle, report, dispatch, drag.dropOn, park],
  );

  const renderManualMenu = useCallback(
    (card: ManualCourseCard, where: TermId | 'incoming'): ReactNode => (
      <ManualCardMenu
        card={card}
        where={where}
        plan={bundle.plan}
        dispatch={dispatch}
      />
    ),
    [bundle.plan, dispatch],
  );

  const onRowKeyDown = useCallback(
    (entry: EntryId, event: React.KeyboardEvent<HTMLElement>) => {
      if (event.key === ' ' && drag.state === undefined) {
        event.preventDefault();
        drag.liftByKeyboard(entry);
      }
    },
    [drag],
  );

  const announcement = useMemo(() => {
    const s = drag.state;
    if (s === undefined || !s.keyboard) {
      return '';
    }
    const code = formatCourseCode(s.payload.course);
    const hovered = s.hovered;
    if (hovered === undefined || hovered.kind !== 'term') {
      return `${code} lifted`;
    }
    const term = bundle.plan.terms.find(t => t.id === hovered.term);
    const line = s.previews.get(hovered.term)?.text ?? '';
    return term === undefined
      ? `${code} lifted`
      : `${code} over ${shortTermLabel(term.position)}, position ${hovered.slot + 1}. ${line}`;
  }, [drag.state, bundle.plan.terms]);

  const addTo = useCallback(
    (course: CourseCode) => {
      const info: CourseInfo | undefined = bundle.facts.courses.find(
        c => courseKey(c.code) === courseKey(course),
      );
      setPicking({
        course,
        credits: info === undefined ? 300 : creditRangeMin(info.credits),
      });
    },
    [bundle.facts.courses],
  );

  const addingToTerm = bundle.plan.terms.find(t => t.id === addingTo);
  const editing = bundle.plan.terms.find(t => t.id === editingTerm);

  const renderTermMenu = useCallback(
    (term: PlanTerm): ReactNode => {
      const removeBlocker = termRemoveBlocker(term);
      return (
        <MoreMenu
          label={`Options for ${shortTermLabel(term.position)}`}
          size="sm"
          alignment="end"
          items={[
            {
              id: 'edit',
              label: 'Edit term…',
              description: 'At Rice, away, or off',
              onClick: () => setEditingTerm(term.id),
            },
            {
              id: 'add',
              label: 'Add term after',
              onClick: () => dispatch({type: 'addTermAfter', after: term.id}),
            },
            {type: 'divider'},
            {
              id: 'remove',
              label: 'Remove term',
              variant: 'destructive',
              isDisabled: removeBlocker !== undefined,
              description: removeBlocker ?? undefined,
              onClick: () => dispatch({type: 'removeTerm', term: term.id}),
            },
          ]}
        />
      );
    },
    [dispatch],
  );

  return (
    <Stack width="100%" height="100%" gap={0}>
      <VisuallyHidden as="div" aria-live="polite">
        {announcement}
      </VisuallyHidden>
      <PlanHeader
        plan={bundle.plan}
        programs={bundle.programs}
        report={report}
        warningCount={report.warnings.length}
        onToggleWarnings={() => setShowWarnings(v => !v)}
        extraActions={
          desktop ? undefined : (
            <>
              <Button
                label="Favorites"
                variant="secondary"
                size="sm"
                onClick={() => setSheet('saved')}
              />
              <Button
                label="Requirements"
                variant="secondary"
                size="sm"
                onClick={() => setSheet('requirements')}
              />
            </>
          )
        }
      />
      <StackItem size="fill">
        <Stack
          direction={desktop ? 'horizontal' : 'vertical'}
          width="100%"
          height="100%"
          gap={0}
          align="stretch"
        >
          {desktop && (
            <FavoritesTray
              bundle={bundle}
              favorites={favorites}
              isOpen={trayOpen}
              onToggle={() => setTrayOpen(v => !v)}
              isDropHovered={hovered?.kind === 'tray'}
              isDragging={dragging && drag.state?.payload.kind === 'card'}
              onCoursePointerDown={drag.onCoursePointerDown}
              registerTarget={drag.registerTarget}
              onAddTo={addTo}
            />
          )}
          <StackItem size="fill">
            <Stack width="100%" height="100%" gap={0}>
              <StackItem size="fill" isScrollable>
                <Board
                  plan={bundle.plan}
                  today={bundle.today}
                  facts={bundle.facts}
                  fillsIndex={fillsIndex}
                  warningsByEntry={warningsByEntry}
                  columns={desktop ? 2 : 1}
                  rowMinHeight={desktop ? undefined : 56}
                  header={
                    showWarnings ? (
                      <WarningsPanel
                        plan={bundle.plan}
                        warnings={report.warnings}
                      />
                    ) : undefined
                  }
                  drag={
                    drag.state !== undefined
                      ? {
                          liftedEntry:
                            drag.state.payload.kind === 'card'
                              ? drag.state.payload.entry
                              : undefined,
                          hoveredTerm:
                            hovered?.kind === 'term' ? hovered.term : undefined,
                          slotIndex:
                            hovered?.kind === 'term' ? hovered.slot : undefined,
                          previews: drag.state.previews,
                        }
                      : undefined
                  }
                  onRowPointerDown={drag.onRowPointerDown}
                  onRowKeyDown={onRowKeyDown}
                  renderMenu={renderMenu}
                  renderManualMenu={renderManualMenu}
                  onAddCourse={setAddingTo}
                  renderTermMenu={renderTermMenu}
                  registerTarget={drag.registerTarget}
                />
              </StackItem>
            </Stack>
          </StackItem>
          {desktop && (
            <Stack width={SIDEBAR_WIDTH} height="100%">
              <RequirementsSidebar
                plan={bundle.plan}
                programs={bundle.programs}
                report={report}
                dispatch={dispatch}
                raisedRules={drag.state?.raisedRules}
                renderSuggestions={renderSuggestions}
                wrapRule={wrapRule}
              />
            </Stack>
          )}
        </Stack>
      </StackItem>
      {!desktop && (
        <BottomSheet
          isOpen={sheet === 'saved'}
          onOpenChange={open => setSheet(open ? 'saved' : undefined)}
          label="Favorites"
          height="tall"
        >
          <FavoritesTray
            bundle={bundle}
            favorites={favorites}
            isOpen
            onToggle={() => setSheet(undefined)}
            isDropHovered={false}
            isDragging={false}
            onCoursePointerDown={drag.onCoursePointerDown}
            registerTarget={() => undefined}
            onAddTo={course => {
              addTo(course);
              setSheet(undefined);
            }}
          />
        </BottomSheet>
      )}
      {!desktop && (
        <BottomSheet
          isOpen={sheet === 'requirements'}
          onOpenChange={open => setSheet(open ? 'requirements' : undefined)}
          label="Requirements"
          height="tall"
        >
          <RequirementsSidebar
            plan={bundle.plan}
            programs={bundle.programs}
            report={report}
            dispatch={dispatch}
            renderSuggestions={renderSuggestions}
          />
        </BottomSheet>
      )}
      <AddCourseDialog
        isOpen={addingTo !== undefined}
        termLabel={
          addingToTerm === undefined
            ? ''
            : shortTermLabel(addingToTerm.position)
        }
        onClose={() => setAddingTo(undefined)}
        onPick={(course, credits) => {
          if (addingTo !== undefined) {
            drag.dropOn(
              {kind: 'course', course, credits},
              {kind: 'term', term: addingTo, slot: Number.MAX_SAFE_INTEGER},
            );
          }
        }}
      />
      <EditTermDialog
        term={editing}
        onClose={() => setEditingTerm(undefined)}
        onSave={(kind, label) => {
          if (editing !== undefined) {
            dispatch({
              type: 'setTerm',
              term: editing.id,
              kind,
              label,
              facts: bundle.facts,
            });
          }
          setEditingTerm(undefined);
        }}
      />
      <TermPickerDialog
        isOpen={picking !== undefined}
        course={picking?.course}
        credits={picking?.credits ?? 0}
        bundle={bundle}
        report={report}
        onClose={() => setPicking(undefined)}
        onPick={term => {
          if (picking !== undefined) {
            drag.dropOn(
              {
                kind: 'course',
                course: picking.course,
                credits: picking.credits,
              },
              {kind: 'term', term, slot: Number.MAX_SAFE_INTEGER},
            );
          }
        }}
      />
      {drag.state !== undefined && (
        <DragGhost
          drag={drag.state}
          note={
            liftedFrom === undefined
              ? drag.state.payload.kind === 'course' &&
                drag.state.payload.fills !== undefined
                ? 'fills this rule'
                : 'from Saved'
              : `lifted from ${shortTermLabel(liftedFrom.position)}`
          }
        />
      )}
    </Stack>
  );
}
