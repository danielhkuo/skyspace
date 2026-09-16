import {Banner} from '@astryxdesign/core/Banner';
import {BottomSheet} from '@astryxdesign/core/BottomSheet';
import {Button} from '@astryxdesign/core/Button';
import {Card} from '@astryxdesign/core/Card';
import {Stack, StackItem} from '@astryxdesign/core/Stack';
import {Layout, LayoutContent, LayoutPanel} from '@astryxdesign/core/Layout';
import {MoreMenu} from '@astryxdesign/core/MoreMenu';
import {useResizable, ResizeHandle} from '@astryxdesign/core/Resizable';
import {Text} from '@astryxdesign/core/Text';
import {VisuallyHidden} from '@astryxdesign/core/VisuallyHidden';
import {useCallback, useEffect, useMemo, useState} from 'react';
import type {ReactNode} from 'react';
import {useNavigate} from 'react-router';

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
  type ProgramSummary,
  type RequirementReport,
  type TermId,
  type Warning,
} from '../domain';
import {dataSource} from '../datasource';
import {NoPlanError, UnauthenticatedError} from '../datasource/types';
import {DESKTOP_WIDTH, useViewportWidth} from '../shell/useViewportWidth';
import {AddCourseDialog} from './AddCourseDialog';
import {Board} from './Board';
import {CardMenu} from './CardMenu';
import {ManualCardMenu} from './ManualCardMenu';
import {PlanSettingsDialog} from './PlanSettingsDialog';
import {EditCourseDialog} from './EditCourseDialog';
import {SelfCheckDialog} from './SelfCheckDialog';
import {DragGhost} from './DragGhost';
import {EditTermDialog} from './EditTermDialog';
import {requirementHit} from './paint';
import {PlanHeader} from './PlanHeader';
import {RequirementsSidebar} from './RequirementsSidebar';
import {RequirementSuggestions} from './RequirementSuggestions';
import {FavoritesTray} from './FavoritesTray';
import {TermPickerDialog} from './TermPickerDialog';
import {requirementTargetKey, useBoardDrag} from './useBoardDrag';
import {isRecordedClaim, planWarnings, termName} from './labels';
import {locateEntry, usePlan, termRemoveBlocker} from './usePlan';
import {WarningsPanel} from './WarningsPanel';
import {LoadErrorCard} from '../shell/LoadErrorCard';

function entryOf(warning: Warning): EntryId | undefined {
  switch (warning.kind) {
    case 'fillsNoRequirement':
    case 'requirementChoiceUnmatched':
    case 'requirementChoiceMissing':
    case 'attributeUnknown':
    case 'incomingCreditIneligible':
    case 'doubleCounted':
    case 'manualCredits':
    case 'courseFactsChanged':
      return warning.value.entry;
    default:
      return undefined;
  }
}

/** A guest has no plan to load (`08-board-interaction.md`): say so, and offer the door. */
function SignInToPlanCard() {
  const navigate = useNavigate();
  return (
    <Stack width="100%" padding={4} align="center">
      <Card padding={3} width={448} maxWidth="100%">
        <Stack width="100%" gap={2} align="start">
          <Text as="h2" size="lg" weight="semibold">
            Sign in to plan a degree
          </Text>
          <Text type="supporting">
            A plan lives on your account. The catalog and schedules work without
            one.
          </Text>
          <Button
            label="Sign in"
            variant="primary"
            size="sm"
            onClick={() => void navigate('/sign-in')}
          />
        </Stack>
      </Card>
    </Stack>
  );
}

/** Loads the plan from the data source, then hands it to the board. */
export function PlanPage() {
  const navigate = useNavigate();
  const [loaded, setLoaded] = useState<
    | {
        bundle: PlanBundle;
        favorites: CourseCode[];
        candidates: CourseCode[];
        available: ProgramSummary[];
      }
    | undefined
  >(undefined);
  const [failed, setFailed] = useState<'error' | 'signedOut' | undefined>(
    undefined,
  );
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let cancelled = false;
    void Promise.all([
      dataSource.loadBundle(),
      dataSource.loadFavorites(),
      dataSource.catalogCandidates(),
      dataSource.listPrograms(),
    ])
      .then(([bundle, favorites, candidates, available]) => {
        if (!cancelled) {
          setLoaded({bundle, favorites, candidates, available});
        }
      })
      .catch((error: unknown) => {
        if (cancelled) {
          return;
        }
        if (error instanceof NoPlanError) {
          // Signed in with nothing to show: onboarding makes the first plan.
          void navigate('/plan/new', {replace: true});
          return;
        }
        setFailed(
          error instanceof UnauthenticatedError ? 'signedOut' : 'error',
        );
      });
    return () => {
      cancelled = true;
    };
  }, [attempt, navigate]);
  if (failed === 'signedOut') {
    return <SignInToPlanCard />;
  }
  if (failed === 'error') {
    return (
      <LoadErrorCard
        what="your plan"
        onRetry={() => {
          setFailed(undefined);
          setAttempt(n => n + 1);
        }}
      />
    );
  }
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
      available={loaded.available}
    />
  );
}

type PlanBoardPageProps = {
  available: ProgramSummary[];
  initial: PlanBundle;
  initialFavorites: CourseCode[];
  catalogCandidates: CourseCode[];
};

function PlanBoardPage({
  available,
  initial,
  initialFavorites,
  catalogCandidates,
}: PlanBoardPageProps) {
  const state = usePlan(initial);
  const {bundle, report, fillsIndex, dispatch} = state;
  const width = useViewportWidth();
  const desktop = width >= DESKTOP_WIDTH;
  const [showWarnings, setShowWarnings] = useState(true);
  const favoritesResizable = useResizable({
    defaultSize: 320,
    minSize: 320,
    maxSize: 480,
    collapsible: true,
    collapsedSize: 40,
  });

  const requirementsResizable = useResizable({
    defaultSize: 400,
    minSize: 360,
    maxSize: 640,
  });
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
  const [editingCourse, setEditingCourse] = useState<EntryId | undefined>(
    undefined,
  );
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [selfCheck, setSelfCheck] = useState<
    {program: Program; requirement: RequirementReport} | undefined
  >(undefined);
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
  const [notice, setNotice] = useState<string | undefined>(undefined);
  useEffect(() => {
    if (notice === undefined) {
      return undefined;
    }
    const t = setTimeout(() => setNotice(undefined), 6000);
    return () => clearTimeout(t);
  }, [notice]);
  const onDuplicate = useCallback(
    (course: CourseCode, where: TermId | 'incoming') => {
      setNotice(
        `${formatCourseCode(course)} is already in ${where === 'incoming' ? 'incoming credit' : termName(bundle.plan, where)}. You only get credit once; move it from there instead.`,
      );
    },
    [bundle.plan],
  );
  const dragCallbacks = useMemo(
    () => ({onPark: park, onDuplicate}),
    [park, onDuplicate],
  );
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
    (program: Program, requirement: RequirementReport): ReactNode => (
      <RequirementSuggestions
        bundle={bundle}
        program={program}
        requirement={requirement}
        saved={favorites}
        catalog={catalogCandidates}
        onCoursePointerDown={drag.onCoursePointerDown}
      />
    ),
    [bundle, favorites, catalogCandidates, drag.onCoursePointerDown],
  );

  const wrapRequirement = useCallback(
    (
      program: Program,
      requirement: RequirementReport,
      row: ReactNode,
    ): ReactNode => {
      const isTarget =
        hovered?.kind === 'requirement' &&
        hovered.program === program.id &&
        hovered.requirement === requirement.requirement;
      return (
        <Stack
          width="100%"
          gap={0.5}
          ref={el =>
            drag.registerTarget(
              requirementTargetKey(program.id, requirement.requirement),
              el,
            )
          }
          style={isTarget ? requirementHit : undefined}
          padding={isTarget ? 1 : 0}
        >
          {row}
          {isTarget && drag.state?.payload.kind === 'card' && (
            <Stack direction="horizontal" gap={1} paddingInlineStart={3}>
              <Text size="sm" weight="medium">
                Fill this requirement with{' '}
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
          onEditCourse={() => setEditingCourse(entry)}
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
        onEditCourse={() => setEditingCourse(card.id)}
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
    <>
      <VisuallyHidden as="div" aria-live="polite">
        {announcement}
      </VisuallyHidden>
      <Layout
        header={
          <PlanHeader
            plan={bundle.plan}
            programs={bundle.programs}
            report={report}
            warningCount={
              planWarnings(report.warnings).filter(w => !isRecordedClaim(w))
                .length
            }
            onToggleWarnings={() => setShowWarnings(v => !v)}
            onOpenSettings={() => setSettingsOpen(true)}
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
        }
        start={
          desktop ? (
            <>
              <LayoutPanel
                resizable={favoritesResizable.props}
                hasDivider={false}
                isScrollable={false}
              >
                <FavoritesTray
                  bundle={bundle}
                  favorites={favorites}
                  isOpen={!favoritesResizable.isCollapsed}
                  onToggle={() => {
                    if (favoritesResizable.isCollapsed) {
                      favoritesResizable.expand();
                    } else {
                      favoritesResizable.collapse();
                    }
                  }}
                  isDropHovered={hovered?.kind === 'tray'}
                  isDragging={dragging && drag.state?.payload.kind === 'card'}
                  onCoursePointerDown={drag.onCoursePointerDown}
                  registerTarget={drag.registerTarget}
                  onAddTo={addTo}
                />
              </LayoutPanel>
              <ResizeHandle
                direction="horizontal"
                hasDivider
                resizable={favoritesResizable.props}
                label="Resize favorites"
              />
            </>
          ) : undefined
        }
        content={
          <LayoutContent padding={0} isScrollable={false}>
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
                    <>
                      {notice !== undefined && (
                        <Banner
                          status="info"
                          container="card"
                          collapsible={false}
                          isDismissable
                          title={notice}
                          onDismiss={() => setNotice(undefined)}
                        />
                      )}
                      {showWarnings && (
                        <WarningsPanel
                          plan={bundle.plan}
                          warnings={planWarnings(report.warnings)}
                          onEditCourse={setEditingCourse}
                        />
                      )}
                    </>
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
          </LayoutContent>
        }
        end={
          desktop ? (
            <>
              <ResizeHandle
                direction="horizontal"
                hasDivider
                resizable={requirementsResizable.props}
                label="Resize requirements"
                isReversed
              />
              <LayoutPanel
                resizable={requirementsResizable.props}
                hasDivider={false}
                isScrollable={false}
              >
                <RequirementsSidebar
                  plan={bundle.plan}
                  programs={bundle.programs}
                  report={report}
                  dispatch={dispatch}
                  raisedRequirements={drag.state?.raisedRequirements}
                  renderSuggestions={renderSuggestions}
                  wrapRequirement={wrapRequirement}
                  onOpenSelfCheck={(program, requirement) =>
                    setSelfCheck({program, requirement})
                  }
                  onEditCourse={setEditingCourse}
                />
              </LayoutPanel>
            </>
          ) : undefined
        }
      ></Layout>
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
            onOpenSelfCheck={(program, requirement) =>
              setSelfCheck({program, requirement})
            }
            onEditCourse={setEditingCourse}
          />
        </BottomSheet>
      )}
      <AddCourseDialog
        isOpen={addingTo !== undefined}
        bundle={bundle}
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
      {(() => {
        if (editingCourse === undefined) {
          return null;
        }
        const located = locateEntry(bundle.plan, editingCourse);
        if (located === undefined) {
          return null;
        }
        return (
          <EditCourseDialog
            entry={editingCourse}
            card={located.where === 'rice' ? located.course : located.card}
            bundle={bundle}
            dispatch={dispatch}
            onClose={() => setEditingCourse(undefined)}
          />
        );
      })()}
      {settingsOpen && (
        <PlanSettingsDialog
          bundle={bundle}
          available={available}
          dispatch={dispatch}
          onClose={() => setSettingsOpen(false)}
        />
      )}
      {selfCheck !== undefined && (
        <SelfCheckDialog
          program={selfCheck.program}
          requirement={selfCheck.requirement}
          catalogYear={bundle.plan.catalogYear}
          existing={bundle.plan.selfChecks.find(
            s => s.requirement === selfCheck.requirement.requirement,
          )}
          dispatch={dispatch}
          onClose={() => setSelfCheck(undefined)}
        />
      )}
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
                ? 'fills this requirement'
                : 'from Saved'
              : `lifted from ${shortTermLabel(liftedFrom.position)}`
          }
        />
      )}
    </>
  );
}
