/**
 * Plan page state: the plan document, the report the engine derives from it,
 * and the indexes the board and sidebar read. Every edit goes through
 * `dispatch`; the report is recomputed on every change (`08-board-interaction.md`).
 */
import {useEffect, useMemo, useReducer} from 'react';

import {
  compareTermPosition,
  findRule,
  isAwayTerm,
  isOffTerm,
  isRiceTerm,
  nextTermPosition,
  newTermId,
  originForLabel,
  termKindName,
  walkRules,
  type EntryId,
  type ManualCourseCard,
  type Plan,
  type PlanBundle,
  type PlannedCourse,
  type PlanTerm,
  type Program,
  type TermKindName,
  type Report,
  type RuleId,
  type RuleReport,
  type TermId,
  type Warning,
} from '../domain';
import {courseInfo} from '../engine/interim/filter';
import type {CourseFacts} from '../domain';
import {engine} from '../engine';
import {warningTerm} from './labels';
import {dataSource} from '../datasource';

export type PlanAction =
  | {type: 'replace'; plan: Plan}
  | {type: 'moveCourse'; entry: EntryId; to: TermId; index: number}
  | {type: 'addCourse'; to: TermId; index: number; course: PlannedCourse}
  | {type: 'addManualCard'; to: TermId | 'incoming'; card: ManualCourseCard}
  | {type: 'removeEntry'; entry: EntryId}
  | {type: 'setFills'; entry: EntryId; program: Program; rule: RuleId}
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
  /** Remove a term that holds no cards. */
  | {type: 'removeTerm'; term: TermId}
  | {
      type: 'confirmSelfCheck';
      rule: RuleId;
      reason: Plan['selfChecks'][number]['reason'];
      note?: string;
    }
  | {type: 'clearSelfCheck'; rule: RuleId};

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
  if (kind === 'off') {
    const count = isRiceTerm(term.kind)
      ? term.kind.rice.courses.length
      : isAwayTerm(term.kind)
        ? term.kind.away.cards.length
        : 0;
    return count === 0
      ? undefined
      : `Move or remove its ${count} course${count === 1 ? '' : 's'} first; an off term holds none.`;
  }
  if (kind === 'rice' && isAwayTerm(term.kind)) {
    const unmapped = term.kind.away.cards.filter(
      c => c.riceEquivalent === undefined,
    );
    return unmapped.length === 0
      ? undefined
      : `${unmapped.length} card${unmapped.length === 1 ? ' has' : 's have'} no Rice course code and cannot live in a Rice term.`;
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
          origin: originForLabel(label),
          code: `${c.course.subject} ${c.course.number}`,
          title: courseInfo(facts, c.course)?.title ?? '',
          credits: c.credits,
          riceEquivalent: c.course,
          fills: c.fills,
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
    case 'removeEntry':
      return withoutEntry(plan, action.entry);
    case 'setFills': {
      const inProgram = new Set<RuleId>();
      walkRules(action.program.root, r => inProgram.add(r.id));
      return mapEntry(plan, action.entry, card => ({
        ...card,
        fills: [...card.fills.filter(r => !inProgram.has(r)), action.rule],
      }));
    }
    case 'setCredits':
      return mapEntry(plan, action.entry, card => ({
        ...card,
        credits: action.credits,
      }));
    case 'confirmSelfCheck':
      return {
        ...plan,
        selfChecks: [
          ...plan.selfChecks.filter(s => s.rule !== action.rule),
          {rule: action.rule, reason: action.reason, note: action.note},
        ],
      };
    case 'clearSelfCheck':
      return {
        ...plan,
        selfChecks: plan.selfChecks.filter(s => s.rule !== action.rule),
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
    case 'removeTerm': {
      const term = plan.terms.find(t => t.id === action.term);
      if (
        term === undefined ||
        (termKindChangeBlocker(term, 'off') !== undefined &&
          !isOffTerm(term.kind))
      ) {
        return plan;
      }
      return {...plan, terms: plan.terms.filter(t => t.id !== action.term)};
    }
  }
}

/** Per-entry view of the report: which rules the card fills, with an area path. */
export type FillsIndex = Map<EntryId, {programName: string; path: string}[]>;

function buildFillsIndex(report: Report, programs: Program[]): FillsIndex {
  const index: FillsIndex = new Map();
  for (const program of report.programs) {
    const source = programs.find(p => p.id === program.program);
    const walk = (rule: RuleReport, ancestors: string[]): void => {
      // Credit allowances consume cards for the fills-no-requirement check but
      // are not something a card "fills" in the student's eyes.
      const kind =
        source === undefined
          ? undefined
          : findRule(source, rule.rule)?.body.kind;
      for (const entry of kind === 'credits' ? [] : rule.filledBy) {
        const list = index.get(entry) ?? [];
        const area = ancestors[0];
        const path =
          area === undefined || area === rule.label
            ? rule.label
            : `${area.replace(/ Requirements?$/i, '')} › ${rule.label}`;
        list.push({programName: program.name, path});
        index.set(entry, list);
      }
      for (const child of rule.children) {
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

/** Debounced save: a burst of drops is one write (`08-board-interaction.md`). */
const SAVE_DEBOUNCE_MS = 500;

export function usePlan(initial: PlanBundle): PlanState {
  const [plan, dispatch] = useReducer(reducePlan, initial.plan);
  useEffect(() => {
    if (plan === initial.plan) {
      return undefined;
    }
    const timer = setTimeout(
      () => void dataSource.savePlan(plan),
      SAVE_DEBOUNCE_MS,
    );
    return () => clearTimeout(timer);
  }, [plan, initial.plan]);
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
