import {BottomSheet} from '@astryxdesign/core/BottomSheet';
import {Button} from '@astryxdesign/core/Button';
import {Stack, StackItem} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {VisuallyHidden} from '@astryxdesign/core/VisuallyHidden';
import {useCallback, useMemo, useState} from 'react';
import type {ReactNode} from 'react';

import {
  courseKey,
  formatCourseCode,
  isRiceTerm,
  shortTermLabel,
  type CourseCode,
  type EntryId,
  type Program,
  type RuleReport,
  type TermId,
  type Warning,
} from '../domain';
import {
  bundle as fixtureBundle,
  catalogCandidates,
  savedCollections as fixtureCollections,
  type SavedCollection,
} from '../fixtures/csStats';
import {DESKTOP_WIDTH, useViewportWidth} from '../shell/useViewportWidth';
import {Board} from './Board';
import {CardMenu} from './CardMenu';
import {DragGhost} from './DragGhost';
import {ruleHit} from './paint';
import {PlanHeader} from './PlanHeader';
import {RequirementsSidebar} from './RequirementsSidebar';
import {RuleSuggestions} from './RuleSuggestions';
import {SavedTray} from './SavedTray';
import {ruleTargetKey, useBoardDrag} from './useBoardDrag';
import {locateEntry, usePlan} from './usePlan';
import {WarningsDrawer} from './WarningsDrawer';

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

export function PlanPage() {
  const state = usePlan(fixtureBundle);
  const {bundle, report, fillsIndex, dispatch} = state;
  const width = useViewportWidth();
  const desktop = width >= DESKTOP_WIDTH;
  const [showWarnings, setShowWarnings] = useState(true);
  const [trayOpen, setTrayOpen] = useState(true);
  const [sheet, setSheet] = useState<'saved' | 'requirements' | undefined>(
    undefined,
  );
  const [collections, setCollections] =
    useState<SavedCollection[]>(fixtureCollections);

  // Parking a card bookmarks it in the first collection; bookmarks are never removed by a drag.
  const park = useCallback((course: CourseCode) => {
    setCollections(prev => {
      const [first, ...rest] = prev;
      if (first === undefined) {
        return [{id: 'coll-saved', name: 'Saved', courses: [course]}];
      }
      if (first.courses.some(c => courseKey(c) === courseKey(course))) {
        return prev;
      }
      return [{...first, courses: [...first.courses, course]}, ...rest];
    });
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
        const code =
          warning.kind === 'mutuallyExclusive'
            ? warning.value.blocked
            : warning.value.course;
        for (const term of bundle.plan.terms) {
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

  const savedCourses = useMemo(
    () => collections.flatMap(c => c.courses),
    [collections],
  );

  const renderSuggestions = useCallback(
    (program: Program, rule: RuleReport): ReactNode => (
      <RuleSuggestions
        bundle={bundle}
        program={program}
        rule={rule}
        saved={savedCourses}
        catalog={catalogCandidates}
        onCoursePointerDown={drag.onCoursePointerDown}
      />
    ),
    [bundle, savedCourses, drag.onCoursePointerDown],
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
      // Non-drag path: the first future Rice term with room. The term picker is step 13.
      const target = bundle.plan.terms.find(
        t =>
          isRiceTerm(t.kind) &&
          t.position.academicYear >= bundle.today.academicYear,
      );
      if (target === undefined) {
        return;
      }
      drag.dropOn(
        {kind: 'course', course, credits: 300},
        {kind: 'term', term: target.id, slot: Number.MAX_SAFE_INTEGER},
      );
    },
    [bundle, drag],
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
                label="Saved"
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
            <SavedTray
              bundle={bundle}
              report={report}
              collections={collections}
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
                  registerTarget={drag.registerTarget}
                />
              </StackItem>
              {showWarnings && (
                <WarningsDrawer
                  plan={bundle.plan}
                  warnings={report.warnings}
                  collapsed={dragging}
                />
              )}
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
          label="Saved courses"
          height="tall"
        >
          <SavedTray
            bundle={bundle}
            report={report}
            collections={collections}
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
