/**
 * Plan page state: the plan document, the report the engine derives from it,
 * and the indexes the board and sidebar read. Every edit goes through
 * `dispatch`; the report is recomputed on every change (`08-board-interaction.md`).
 */
import {useEffect, useMemo, useReducer} from 'react';

import {
  courseInfo,
  compareTermPosition,
  findRequirement,
  isAwayTerm,
  isOffTerm,
  isRiceTerm,
  nextTermPosition,
  newTermId,
  originForLabel,
  termKindName,
  walkRequirements,
  type EntryId,
  type ProgramId,
  type CatalogYear,
  type FillClaim,
  type ManualCourseCard,
  type Plan,
  type PlanBundle,
  type PlannedCourse,
  type PlanTerm,
  type Program,
  type TermKindName,
  type Report,
  type RequirementId,
  type RequirementReport,
  type TermId,
  type TermPosition,
  type Warning,
} from '../domain';
import type {CourseFacts} from '../domain';
import {engine} from '../engine';
import {warningTerm} from './labels';
import {flushPlanSave, queuePlanSave} from '../datasource/planSaver';

export type PlanAction =
  | {type: 'replace'; plan: Plan}
  | {type: 'moveCourse'; entry: EntryId; to: TermId; index: number}
  | {type: 'addCourse'; to: TermId; index: number; course: PlannedCourse}
  | {type: 'addManualCard'; to: TermId | 'incoming'; card: ManualCourseCard}
  /** An away or incoming card to another away term or to incoming credit. */
  | {type: 'moveManualCard'; entry: EntryId; to: TermId | 'incoming'}
  | {type: 'removeEntry'; entry: EntryId}
  | {
      type: 'setFills';
      entry: EntryId;
      program: Program;
      requirement: RequirementId;
    }
  /** Drop the pin for one program; the matcher decides again. */
  | {type: 'clearFill'; entry: EntryId; program: Program}
  /** Details of an away or incoming card; the id, fills and claims stay. */
  | {
      type: 'editManualCard';
      entry: EntryId;
      patch: Partial<
        Pick<
          ManualCourseCard,
          | 'origin'
          | 'code'
          | 'title'
          | 'institution'
          | 'riceEquivalent'
          | 'creditsSource'
        >
      >;
    }
  /** Why a pin the filter rejects should count. Replaces any claim on that requirement. */
  | {type: 'setFillClaim'; entry: EntryId; claim: FillClaim}
  | {type: 'setCredits'; entry: EntryId; credits: number}
  /** Change a term's kind and label. Refused when cards would be lost; see `termKindChangeBlocker`. */
  | {
      type: 'setTerm';
      term: TermId;
      kind: TermKindName;
      label?: string;
      /** For the titles of cards converted to manual cards. */
      facts: CourseFacts;
    }
  /** Insert an empty Rice term at the next free board position after `after`. */
  | {type: 'addTermAfter'; after: TermId}
  /** Insert an empty Rice term at a position, if none is there. Summers included. */
  | {type: 'addTermAt'; position: TermPosition}
  /** Remove a term that holds no cards. */
  | {type: 'removeTerm'; term: TermId}
  | {type: 'renamePlan'; name: string}
  | {type: 'setCatalogYear'; year: CatalogYear}
  /** The programs a plan follows; University stays. Pins to a dropped program's requirements go with it. */
  | {type: 'setPrograms'; programs: ProgramId[]; available: Program[]}
  | {
      type: 'confirmSelfCheck';
      requirement: RequirementId;
      reason: Plan['selfChecks'][number]['reason'];
      note?: string;
    }
  | {type: 'clearSelfCheck'; requirement: RequirementId};

type Located =
  | {where: 'rice'; term: TermId; course: PlannedCourse}
  | {where: 'away'; term: TermId; card: ManualCourseCard}
  | {where: 'incoming'; card: ManualCourseCard};

export function locateEntry(plan: Plan, entry: EntryId): Located | undefined {
  for (const card of plan.incomingCredit) {
    if (card.id === entry) {
      return {where: 'incoming', card};
    }
  }
  for (const term of plan.terms) {
    if (isRiceTerm(term.kind)) {
      const course = term.kind.rice.courses.find(c => c.id === entry);
      if (course !== undefined) {
        return {where: 'rice', term: term.id, course};
      }
    } else if (isAwayTerm(term.kind)) {
      const card = term.kind.away.cards.find(c => c.id === entry);
      if (card !== undefined) {
        return {where: 'away', term: term.id, card};
      }
    }
  }
  return undefined;
}

function withoutEntry(plan: Plan, entry: EntryId): Plan {
  return {
    ...plan,
    incomingCredit: plan.incomingCredit.filter(c => c.id !== entry),
    terms: plan.terms.map(term => {
      if (isRiceTerm(term.kind)) {
        return {
          ...term,
          kind: {
            rice: {
              ...term.kind.rice,
              courses: term.kind.rice.courses.filter(c => c.id !== entry),
            },
          },
        };
      }
      if (isAwayTerm(term.kind)) {
        return {
          ...term,
          kind: {
            away: {cards: term.kind.away.cards.filter(c => c.id !== entry)},
          },
        };
      }
      return term;
    }),
  };
}

function insertCourse(
  plan: Plan,
  to: TermId,
  index: number,
  course: PlannedCourse,
): Plan {
  return {
    ...plan,
    terms: plan.terms.map(term => {
      if (term.id !== to || !isRiceTerm(term.kind)) {
        return term;
      }
      const courses = [...term.kind.rice.courses];
      courses.splice(Math.min(index, courses.length), 0, course);
      return {...term, kind: {rice: {...term.kind.rice, courses}}};
    }),
  };
}

function insertManualCard(
  plan: Plan,
  to: TermId | 'incoming',
  card: ManualCourseCard,
): Plan {
  if (to === 'incoming') {
    return {...plan, incomingCredit: [...plan.incomingCredit, card]};
  }
  return {
    ...plan,
    terms: plan.terms.map(term => {
      if (term.id !== to || !isAwayTerm(term.kind)) {
        return term;
      }
      return {...term, kind: {away: {cards: [...term.kind.away.cards, card]}}};
    }),
  };
}

function mapEntry(
  plan: Plan,
  entry: EntryId,
  f: <T extends PlannedCourse | ManualCourseCard>(card: T) => T,
): Plan {
  return {
    ...plan,
    incomingCredit: plan.incomingCredit.map(c => (c.id === entry ? f(c) : c)),
    terms: plan.terms.map(term => {
      if (isRiceTerm(term.kind)) {
        return {
          ...term,
          kind: {
            rice: {
              ...term.kind.rice,
              courses: term.kind.rice.courses.map(c =>
                c.id === entry ? f(c) : c,
              ),
            },
          },
        };
      }
      if (isAwayTerm(term.kind)) {
        return {
          ...term,
          kind: {
            away: {
              cards: term.kind.away.cards.map(c => (c.id === entry ? f(c) : c)),
            },
          },
        };
      }
      return term;
    }),
  };
}

/** Why a kind change is refused, or `undefined` when it is allowed. Nothing is ever deleted by a toggle. */
export function termKindChangeBlocker(
  term: PlanTerm,
  kind: TermKindName,
): string | undefined {
  if (kind === termKindName(term.kind)) {
    return undefined;
  }
  const held = termCardLabels(term);
  const list = held.length === 0 ? '' : ` (${held.join(', ')})`;
  if (kind === 'off') {
    return held.length === 0
      ? undefined
      : `Move or remove its ${held.length} course${held.length === 1 ? '' : 's'} first${list}; an off term holds none.`;
  }
  if (kind === 'rice' && isAwayTerm(term.kind)) {
    // Lossless both ways now: a card keeps its own code and title on the Rice
    // card (`carried`). Only a card with no Rice equivalent has nowhere to go.
    const unmapped = term.kind.away.cards.filter(
      c => c.riceEquivalent === undefined,
    );
    return unmapped.length === 0
      ? undefined
      : `${unmapped.map(c => c.code).join(', ')} ${unmapped.length === 1 ? 'has' : 'have'} no Rice equivalent, so ${unmapped.length === 1 ? 'it' : 'they'} cannot sit in a Rice term. Open Edit course… and set one, or move ${unmapped.length === 1 ? 'it' : 'them'} first.`;
  }
  return undefined;
}

/** The codes a term holds, for the refusal messages. */
export function termCardLabels(term: PlanTerm): string[] {
  if (isRiceTerm(term.kind)) {
    return term.kind.rice.courses.map(
      c => `${c.course.subject} ${c.course.number}`,
    );
  }
  if (isAwayTerm(term.kind)) {
    return term.kind.away.cards.map(c => c.code);
  }
  return [];
}

/** Why a term cannot be removed, or `undefined`. Cards and claims both count. */
export function termRemoveBlocker(term: PlanTerm): string | undefined {
  const cards = termKindChangeBlocker(term, 'off');
  if (cards !== undefined && !isOffTerm(term.kind)) {
    return cards;
  }
  if (term.nonCourse.length > 0) {
    return `Clear its ${term.nonCourse.length} claimed requirement${term.nonCourse.length === 1 ? '' : 's'} first (${term.nonCourse.map(c => c.label).join(', ')}).`;
  }
  return undefined;
}

function convertTerm(
  term: PlanTerm,
  kind: TermKindName,
  label: string | undefined,
  facts: CourseFacts,
): PlanTerm {
  const base = {...term, label};
  if (kind === termKindName(term.kind)) {
    return base;
  }
  if (kind === 'off') {
    return {...base, kind: 'off'};
  }
  if (kind === 'away') {
    const cards: ManualCourseCard[] = isRiceTerm(term.kind)
      ? term.kind.rice.courses.map(c => ({
          id: c.id,
          // A card that was manual before keeps its own code and title; a
          // Rice course becomes a card named after itself.
          origin: c.carried?.origin ?? originForLabel(label),
          code: c.carried?.code ?? `${c.course.subject} ${c.course.number}`,
          title: c.carried?.title ?? courseInfo(facts, c.course)?.title ?? '',
          credits: c.credits,
          institution: c.carried?.institution,
          riceEquivalent: c.course,
          fills: c.fills,
          claims: c.claims,
          note: c.note,
        }))
      : [];
    return {...base, kind: {away: {cards}}};
  }
  const courses: PlannedCourse[] = isAwayTerm(term.kind)
    ? term.kind.away.cards.flatMap(c =>
        c.riceEquivalent === undefined
          ? []
          : [
              {
                id: c.id,
                course: c.riceEquivalent,
                credits: c.credits,
                fills: c.fills,
                claims: c.claims,
                carried: {
                  origin: c.origin,
                  code: c.code,
                  title: c.title,
                  institution: c.institution,
                },
                note: c.note,
              },
            ],
      )
    : [];
  return {...base, kind: {rice: {courses}}};
}

export function reducePlan(plan: Plan, action: PlanAction): Plan {
  switch (action.type) {
    case 'replace':
      return action.plan;
    case 'moveCourse': {
      const located = locateEntry(plan, action.entry);
      if (located === undefined || located.where !== 'rice') {
        return plan;
      }
      // Same-column moves: compute the index against the column without the card.
      const stripped = withoutEntry(plan, action.entry);
      return insertCourse(stripped, action.to, action.index, located.course);
    }
    case 'addCourse':
      return insertCourse(plan, action.to, action.index, action.course);
    case 'addManualCard':
      return insertManualCard(plan, action.to, action.card);
    case 'moveManualCard': {
      const located = locateEntry(plan, action.entry);
      if (located === undefined || located.where === 'rice') {
        return plan;
      }
      return insertManualCard(
        withoutEntry(plan, action.entry),
        action.to,
        located.card,
      );
    }
    case 'removeEntry':
      return withoutEntry(plan, action.entry);
    case 'setFills': {
      const inProgram = new Set<RequirementId>();
      walkRequirements(action.program.root, r => inProgram.add(r.id));
      return mapEntry(plan, action.entry, card => ({
        ...card,
        fills: [
          ...card.fills.filter(r => !inProgram.has(r)),
          action.requirement,
        ],
        claims: (card.claims ?? []).filter(c => !inProgram.has(c.requirement)),
      }));
    }
    case 'editManualCard':
      return mapEntry(plan, action.entry, card =>
        'course' in card
          ? card
          : ({
              ...card,
              ...action.patch,
              ...(action.patch.institution === undefined
                ? {institution: undefined}
                : {}),
              ...(action.patch.riceEquivalent === undefined
                ? {riceEquivalent: undefined}
                : {}),
            } as typeof card),
      );
    case 'setFillClaim':
      return mapEntry(plan, action.entry, card => ({
        ...card,
        claims: [
          ...(card.claims ?? []).filter(
            c => c.requirement !== action.claim.requirement,
          ),
          action.claim,
        ],
      }));
    case 'clearFill': {
      const inProgram = new Set<RequirementId>();
      walkRequirements(action.program.root, r => inProgram.add(r.id));
      return mapEntry(plan, action.entry, card => ({
        ...card,
        fills: card.fills.filter(r => !inProgram.has(r)),
        claims: (card.claims ?? []).filter(c => !inProgram.has(c.requirement)),
      }));
    }
    case 'setCredits':
      return mapEntry(plan, action.entry, card => ({
        ...card,
        credits: action.credits,
      }));
    case 'confirmSelfCheck': {
      // The checkbox path sends a bare reason; never let it erase a note or
      // a reason the student wrote in the dialog.
      const previous = plan.selfChecks.find(
        s => s.requirement === action.requirement,
      );
      const next = {
        requirement: action.requirement,
        reason: previous?.reason ?? action.reason,
        note: action.note ?? previous?.note,
      };
      return {
        ...plan,
        selfChecks: [
          ...plan.selfChecks.filter(s => s.requirement !== action.requirement),
          next,
        ],
      };
    }
    case 'clearSelfCheck':
      return {
        ...plan,
        selfChecks: plan.selfChecks.filter(
          s => s.requirement !== action.requirement,
        ),
      };
    case 'setTerm': {
      const term = plan.terms.find(t => t.id === action.term);
      if (
        term === undefined ||
        termKindChangeBlocker(term, action.kind) !== undefined
      ) {
        return plan;
      }
      const label =
        action.label?.trim() === '' ? undefined : action.label?.trim();
      return {
        ...plan,
        terms: plan.terms.map(t =>
          t.id === action.term
            ? convertTerm(t, action.kind, label, action.facts)
            : t,
        ),
      };
    }
    case 'addTermAfter': {
      const after = plan.terms.find(t => t.id === action.after);
      if (after === undefined) {
        return plan;
      }
      let position = nextTermPosition(after.position);
      while (
        plan.terms.some(t => compareTermPosition(t.position, position) === 0)
      ) {
        position = nextTermPosition(position);
      }
      const term: PlanTerm = {
        id: newTermId(),
        position,
        kind: {rice: {courses: []}},
        nonCourse: [],
      };
      return {
        ...plan,
        terms: [...plan.terms, term].sort((a, b) =>
          compareTermPosition(a.position, b.position),
        ),
      };
    }
    case 'renamePlan':
      return {
        ...plan,
        name: action.name.trim() === '' ? plan.name : action.name.trim(),
      };
    case 'setCatalogYear':
      return {...plan, catalogYear: action.year};
    case 'setPrograms': {
      // A plan always has at least one major (`features/plan.md`); a change
      // that would leave none is refused whole.
      const majorsLeft = action.programs.filter(
        id => action.available.find(p => p.id === id)?.kind === 'major',
      );
      if (majorsLeft.length === 0) {
        return plan;
      }
      const dropped = new Set<RequirementId>();
      for (const program of action.available) {
        if (
          plan.programs.includes(program.id) &&
          !action.programs.includes(program.id)
        ) {
          walkRequirements(program.root, r => dropped.add(r.id));
        }
      }
      const strip = <T extends {fills: RequirementId[]; claims?: FillClaim[]}>(
        card: T,
      ): T => ({
        ...card,
        fills: card.fills.filter(r => !dropped.has(r)),
        claims: (card.claims ?? []).filter(c => !dropped.has(c.requirement)),
      });
      return {
        ...plan,
        programs: action.programs,
        selfChecks: plan.selfChecks.filter(s => !dropped.has(s.requirement)),
        incomingCredit: plan.incomingCredit.map(strip),
        terms: plan.terms.map(term =>
          isRiceTerm(term.kind)
            ? {
                ...term,
                kind: {
                  rice: {
                    ...term.kind.rice,
                    courses: term.kind.rice.courses.map(strip),
                  },
                },
              }
            : isAwayTerm(term.kind)
              ? {
                  ...term,
                  kind: {away: {cards: term.kind.away.cards.map(strip)}},
                }
              : term,
        ),
      };
    }
    case 'addTermAt': {
      if (
        plan.terms.some(
          t => compareTermPosition(t.position, action.position) === 0,
        )
      ) {
        return plan;
      }
      const term: PlanTerm = {
        id: newTermId(),
        position: action.position,
        kind: {rice: {courses: []}},
        nonCourse: [],
      };
      return {
        ...plan,
        terms: [...plan.terms, term].sort((a, b) =>
          compareTermPosition(a.position, b.position),
        ),
      };
    }
    case 'removeTerm': {
      const term = plan.terms.find(t => t.id === action.term);
      if (term === undefined || termRemoveBlocker(term) !== undefined) {
        return plan;
      }
      return {...plan, terms: plan.terms.filter(t => t.id !== action.term)};
    }
  }
}

/** Per-entry view of the report: which requirements the card fills, with an area path. */
export type FillsIndex = Map<EntryId, {programName: string; path: string}[]>;

function buildFillsIndex(report: Report, programs: Program[]): FillsIndex {
  const index: FillsIndex = new Map();
  for (const program of report.programs) {
    const source = programs.find(p => p.id === program.program);
    const walk = (
      requirement: RequirementReport,
      ancestors: string[],
    ): void => {
      // Credit allowances consume cards for the fills-no-requirement check but
      // are not something a card "fills" in the student's eyes.
      const kind =
        source === undefined
          ? undefined
          : findRequirement(source, requirement.requirement)?.body.kind;
      for (const entry of kind === 'credits' ? [] : requirement.filledBy) {
        const list = index.get(entry) ?? [];
        const area = ancestors[0];
        const path =
          area === undefined || area === requirement.label
            ? requirement.label
            : `${area.replace(/ Requirements?$/i, '')} › ${requirement.label}`;
        list.push({programName: program.name, path});
        index.set(entry, list);
      }
      for (const child of requirement.children) {
        walk(child, ancestors.length === 0 ? [child.label] : ancestors);
      }
    };
    // Root is the program itself; its direct children are the areas.
    for (const area of program.root.children) {
      walk(area, [area.label]);
    }
  }
  return index;
}

export type PlanState = {
  bundle: PlanBundle;
  report: Report;
  fillsIndex: FillsIndex;
  warningsByTerm: Map<TermId, Warning[]>;
  dispatch: (action: PlanAction) => void;
};

export function usePlan(initial: PlanBundle): PlanState {
  const [plan, dispatch] = useReducer(reducePlan, initial.plan);
  // Every edit goes to the one save coordinator: a burst of drops is one
  // write (`08-board-interaction.md`), and leaving the page flushes it.
  useEffect(() => {
    if (plan !== initial.plan) {
      queuePlanSave(plan);
    }
  }, [plan, initial.plan]);
  useEffect(() => () => void flushPlanSave(), []);
  const bundle = useMemo(() => ({...initial, plan}), [initial, plan]);
  const report = useMemo(() => engine.evaluate(bundle), [bundle]);
  const fillsIndex = useMemo(
    () => buildFillsIndex(report, initial.programs),
    [report, initial.programs],
  );
  const warningsByTerm = useMemo(() => {
    const map = new Map<TermId, Warning[]>();
    for (const warning of report.warnings) {
      const term = warningTerm(warning);
      if (term === undefined) {
        continue;
      }
      map.set(term, [...(map.get(term) ?? []), warning]);
    }
    return map;
  }, [report]);
  return {bundle, report, fillsIndex, warningsByTerm, dispatch};
}
