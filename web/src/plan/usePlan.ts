/**
 * Plan page state: the plan document, the report the engine derives from it,
 * and the indexes the board and sidebar read. Every edit goes through
 * `dispatch`; the report is recomputed on every change (`08-board-interaction.md`).
 */
import {useEffect, useMemo, useReducer} from 'react';

import {
  findRule,
  isAwayTerm,
  isRiceTerm,
  walkRules,
  type EntryId,
  type ManualCourseCard,
  type Plan,
  type PlanBundle,
  type PlannedCourse,
  type Program,
  type Report,
  type RuleId,
  type RuleReport,
  type TermId,
  type Warning,
} from '../domain';
import {engine} from '../engine';
import {warningTerm} from './labels';
import {loadPlan, savePlan, SAVE_DEBOUNCE_MS} from './persistence';

export type PlanAction =
  | {type: 'replace'; plan: Plan}
  | {type: 'moveCourse'; entry: EntryId; to: TermId; index: number}
  | {type: 'addCourse'; to: TermId; index: number; course: PlannedCourse}
  | {type: 'addManualCard'; to: TermId | 'incoming'; card: ManualCourseCard}
  | {type: 'removeEntry'; entry: EntryId}
  | {type: 'setFills'; entry: EntryId; program: Program; rule: RuleId}
  | {type: 'setCredits'; entry: EntryId; credits: number}
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

export function usePlan(initial: PlanBundle): PlanState {
  const [plan, dispatch] = useReducer(
    reducePlan,
    initial.plan,
    seed => loadPlan(seed.id) ?? seed,
  );
  // Debounced save: a burst of drops is one write (`08-board-interaction.md`).
  useEffect(() => {
    const timer = setTimeout(() => savePlan(plan), SAVE_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [plan]);
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
