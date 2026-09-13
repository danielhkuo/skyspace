/**
 * Interim TypeScript evaluator. `skyspace-core::evaluate` is the real one and
 * arrives through wasm; this keeps the board live until then and follows the
 * same steps (`03-requirements.md`): collect cards, flatten slots, apply
 * choices, match, run credit requirements, fold.
 *
 * Known simplification: matching is greedy in document order, not maximum
 * bipartite matching. The COMP 140 case can come out wrong here.
 */
import {
  courseKey,
  isAwayTerm,
  isRiceTerm,
  plannedCredits,
  type CourseCode,
  type CourseFacts,
  type CourseFilter,
  type Credits,
  type EntryId,
  type Outcome,
  type Plan,
  type PlanBundle,
  type Program,
  type ProgramId,
  type ProgramReport,
  type Progress,
  type Report,
  type Requirement,
  type RequirementId,
  type RequirementReport,
  type SelfCheckReason,
  type TermId,
  type Warning,
  creditRangeMin,
  type CreditOrigin,
  type FillClaim,
} from '../../domain';
import {canonical, courseInfo, filterMatches} from './filter';
import {evaluatePrereq} from './prereq';
import {buildTakenIndex} from './taken';

export const INTERIM_ENGINE_VERSION = 'ts-interim-0';

type Card = {
  entry: EntryId;
  term?: TermId;
  code?: CourseCode;
  credits: Credits;
  fills: RequirementId[];
  claims: FillClaim[];
  origin?: CreditOrigin;
};

type Slot = {
  requirement: RequirementId;
  filter: CourseFilter;
};

function collectCards(plan: Plan, facts: CourseFacts): Card[] {
  const cards: Card[] = [];
  for (const card of plan.incomingCredit) {
    cards.push({
      entry: card.id,
      code:
        card.riceEquivalent === undefined
          ? undefined
          : canonical(facts, card.riceEquivalent),
      credits: card.credits,
      fills: card.fills,
      claims: card.claims ?? [],
      origin: card.origin,
    });
  }
  for (const term of plan.terms) {
    if (isRiceTerm(term.kind)) {
      for (const c of term.kind.rice.courses) {
        cards.push({
          entry: c.id,
          term: term.id,
          code: canonical(facts, c.course),
          credits: c.credits,
          fills: c.fills,
          claims: c.claims ?? [],
        });
      }
    } else if (isAwayTerm(term.kind)) {
      for (const c of term.kind.away.cards) {
        cards.push({
          entry: c.id,
          term: term.id,
          code:
            c.riceEquivalent === undefined
              ? undefined
              : canonical(facts, c.riceEquivalent),
          credits: c.credits,
          fills: c.fills,
          claims: c.claims ?? [],
          origin: c.origin,
        });
      }
    }
  }
  return cards;
}

function flattenSlots(requirement: Requirement, out: Slot[]): void {
  const body = requirement.body;
  if (body.kind === 'course') {
    for (let i = 0; i < body.semesters; i += 1) {
      out.push({requirement: requirement.id, filter: body.filter});
    }
  } else if (body.kind === 'all' || body.kind === 'select') {
    for (const child of body.of) {
      flattenSlots(child, out);
    }
  }
}

/** AP and IB credit counts toward the total and the major, never toward distribution or AD. */
function originBarsAttributeSlots(card: Card): boolean {
  return (
    card.origin === 'advancedPlacement' ||
    card.origin === 'internationalBaccalaureate'
  );
}

function slotNeedsAttribute(slot: Slot): boolean {
  return slot.filter.include.some(sel => sel.kind === 'attribute');
}

function emptyProgress(): Progress {
  return {
    requirementsMet: 0,
    requirementsClaimed: 0,
    requirementsCheckable: 0,
    creditsMet: 0,
    creditsRequired: 0,
    creditsUnknown: 0,
    selfChecks: 0,
    selfChecksConfirmed: 0,
  };
}

function addProgress(a: Progress, b: Progress): Progress {
  return {
    requirementsMet: a.requirementsMet + b.requirementsMet,
    requirementsClaimed: a.requirementsClaimed + b.requirementsClaimed,
    requirementsCheckable: a.requirementsCheckable + b.requirementsCheckable,
    creditsMet: a.creditsMet + b.creditsMet,
    creditsRequired: a.creditsRequired + b.creditsRequired,
    creditsUnknown: a.creditsUnknown + b.creditsUnknown,
    selfChecks: a.selfChecks + b.selfChecks,
    selfChecksConfirmed: a.selfChecksConfirmed + b.selfChecksConfirmed,
  };
}

type Assignment = Map<RequirementId, Card[]>;

type Matching = {
  assigned: Assignment;
  /** Cards sitting in a slot by the student's claim, not by the filter. */
  claimed: Set<EntryId>;
};

function matchCards(
  program: Program,
  cards: Card[],
  facts: CourseFacts,
  warnings: Warning[],
): Matching {
  const slots: Slot[] = [];
  flattenSlots(program.root, slots);
  const assigned: Assignment = new Map();
  const claimed = new Set<EntryId>();
  const slotTaken = new Array<boolean>(slots.length).fill(false);
  const free: Card[] = [];
  const requirementIds = new Set(slots.map(s => s.requirement));

  const give = (slotIndex: number, card: Card): void => {
    const slot = slots[slotIndex];
    if (slot === undefined) {
      return;
    }
    slotTaken[slotIndex] = true;
    const list = assigned.get(slot.requirement) ?? [];
    list.push(card);
    assigned.set(slot.requirement, list);
  };

  // Choices first: a card pinned to a requirement in this program takes that requirement's first free slot.
  for (const card of cards) {
    const choice = card.fills.find(r => requirementIds.has(r));
    if (choice === undefined) {
      free.push(card);
      continue;
    }
    const slotIndex = slots.findIndex(
      (s, i) => s.requirement === choice && !slotTaken[i],
    );
    if (slotIndex === -1) {
      free.push(card);
      continue;
    }
    give(slotIndex, card);
    const slot = slots[slotIndex];
    if (slot === undefined) {
      continue;
    }
    // A pin the filter rejects is a claim: shown in the slot, never counted
    // as met, and it must say why. AP/IB on an attribute slot is one too,
    // whatever the filter says, because Rice's own policy rejects it.
    const barred = originBarsAttributeSlots(card) && slotNeedsAttribute(slot);
    const rejected =
      card.code === undefined ||
      filterMatches(slot.filter, card.code, facts) === 'no';
    if (barred && card.origin !== undefined) {
      claimed.add(card.entry);
      warnings.push({
        kind: 'incomingCreditIneligible',
        value: {entry: card.entry, requirement: choice, origin: card.origin},
      });
    } else if (rejected) {
      claimed.add(card.entry);
      if (card.term !== undefined) {
        warnings.push({
          kind: 'requirementChoiceUnmatched',
          value: {
            term: card.term,
            entry: card.entry,
            requirement: choice,
            basis: card.claims.find(c => c.requirement === choice)?.basis,
          },
        });
      }
    }
  }

  // Maximum bipartite matching by augmenting paths (`03-requirements.md`
  // "Matching"): slots in document order, candidates in board order, so the
  // result is deterministic. Greedy gets the COMP 140 case wrong: a card that
  // fits a core slot and a distribution slot must leave the distribution slot
  // for the card that fits nothing else.
  const candidates: number[][] = slots.map(() => []);
  const unknownWarned = new Set<EntryId>();
  free.forEach((card, cardIndex) => {
    const code = card.code;
    if (code === undefined) {
      return;
    }
    slots.forEach((slot, slotIndex) => {
      if (slotTaken[slotIndex]) {
        return;
      }
      if (originBarsAttributeSlots(card) && slotNeedsAttribute(slot)) {
        return;
      }
      const m = filterMatches(slot.filter, code, facts);
      if (m === 'yes') {
        candidates[slotIndex]?.push(cardIndex);
      } else if (
        m === 'unknown' &&
        card.term !== undefined &&
        !unknownWarned.has(card.entry)
      ) {
        unknownWarned.add(card.entry);
        warnings.push({
          kind: 'attributeUnknown',
          value: {term: card.term, entry: card.entry, course: code},
        });
      }
    });
  });
  const slotOfCard = new Array<number | undefined>(free.length).fill(undefined);
  const augment = (slotIndex: number, seen: Set<number>): boolean => {
    for (const cardIndex of candidates[slotIndex] ?? []) {
      if (seen.has(cardIndex)) {
        continue;
      }
      seen.add(cardIndex);
      const holder = slotOfCard[cardIndex];
      if (holder === undefined || augment(holder, seen)) {
        slotOfCard[cardIndex] = slotIndex;
        return true;
      }
    }
    return false;
  };
  for (let slotIndex = 0; slotIndex < slots.length; slotIndex += 1) {
    if (!slotTaken[slotIndex]) {
      augment(slotIndex, new Set());
    }
  }
  // Hand out in board order so `filledBy` and credit sums stay board-ordered.
  free.forEach((card, cardIndex) => {
    const slotIndex = slotOfCard[cardIndex];
    if (slotIndex !== undefined) {
      give(slotIndex, card);
    }
  });
  return {assigned, claimed};
}

function creditsMetBy(cards: Card[]): Credits {
  return cards.reduce((sum, c) => sum + c.credits, 0);
}

function evaluateRequirement(
  requirement: Requirement,
  program: Program,
  assigned: Assignment,
  claimed: Set<EntryId>,
  consumed: Set<EntryId>,
  cards: Card[],
  facts: CourseFacts,
  confirmed: Map<RequirementId, SelfCheckReason>,
  claims: Map<RequirementId, TermId>,
): RequirementReport {
  const body = requirement.body;
  const base = {
    requirement: requirement.id,
    label: requirement.label,
    source: requirement.source,
    claimedIn: claims.get(requirement.id),
  };

  if (body.kind === 'nonCourse' || body.kind === 'unverifiable') {
    const reason = confirmed.get(requirement.id);
    const progress = emptyProgress();
    progress.selfChecks = 1;
    progress.selfChecksConfirmed = reason === undefined ? 0 : 1;
    const outcome: Outcome =
      reason === undefined
        ? {outcome: 'needsStudentCheck'}
        : {outcome: 'needsStudentCheck', confirmed: reason};
    return {
      ...base,
      outcome,
      progress,
      filledBy: [],
      claimedBy: [],
      children: [],
    };
  }

  if (body.kind === 'course') {
    const filled = assigned.get(requirement.id) ?? [];
    const matched = filled.filter(c => !claimed.has(c.entry));
    const progress = emptyProgress();
    progress.requirementsCheckable = 1;
    const met = matched.length >= body.semesters;
    progress.requirementsMet = met ? 1 : 0;
    progress.requirementsClaimed =
      !met && filled.length > matched.length ? 1 : 0;
    progress.creditsMet = creditsMetBy(filled);
    const single =
      body.filter.include.length === 1 &&
      body.filter.include[0]?.kind === 'code'
        ? body.filter.include[0].code
        : undefined;
    if (requirement.hours !== undefined) {
      progress.creditsRequired =
        creditRangeMin(requirement.hours) * body.semesters;
    } else if (single !== undefined && courseInfo(facts, single)) {
      const info = courseInfo(facts, single);
      progress.creditsRequired =
        (info === undefined ? 0 : creditRangeMin(info.credits)) *
        body.semesters;
    } else {
      progress.creditsUnknown = 1;
    }
    const outcome: Outcome = met
      ? {outcome: 'met'}
      : filled.length > 0
        ? {outcome: 'partial'}
        : {outcome: 'unmet'};
    return {
      ...base,
      outcome,
      progress,
      filledBy: filled.map(c => c.entry),
      claimedBy: filled.filter(c => claimed.has(c.entry)).map(c => c.entry),
      children: [],
    };
  }

  if (body.kind === 'credits') {
    // `Additional` counts only cards filling no other requirement; a free-elective
    // allowance consumes cards in board order until it is full, so
    // fills-no-requirement fires only past the allowance (`04-planning.md`).
    const eligible = cards.filter(
      c =>
        (c.code === undefined
          ? body.from.include.length === 0
          : filterMatches(body.from, c.code, facts) === 'yes') &&
        (body.scope === 'any' || !consumed.has(c.entry)),
    );
    const matching: Card[] = [];
    let total = 0;
    for (const card of eligible) {
      if (total >= body.minimum) {
        break;
      }
      matching.push(card);
      total += card.credits;
    }
    const progress = emptyProgress();
    progress.requirementsCheckable = 1;
    progress.creditsMet = creditsMetBy(matching);
    progress.creditsRequired = body.minimum;
    const met = progress.creditsMet >= body.minimum;
    progress.requirementsMet = met ? 1 : 0;
    const outcome: Outcome = met
      ? {outcome: 'met'}
      : progress.creditsMet > 0
        ? {outcome: 'partial'}
        : {outcome: 'unmet'};
    return {
      ...base,
      outcome,
      progress,
      filledBy: matching.map(c => c.entry),
      claimedBy: [],
      children: [],
    };
  }

  // all / select
  const children = body.of.map(child =>
    evaluateRequirement(
      child,
      program,
      assigned,
      claimed,
      consumed,
      cards,
      facts,
      confirmed,
      claims,
    ),
  );
  let progress = emptyProgress();
  let metChildren = 0;
  let checkableChildren = 0;
  // A self-check the student has not confirmed holds its parent at "partial":
  // a group is never Met on the strength of something nobody verified.
  let openSelfChecks = 0;
  for (const child of children) {
    progress = addProgress(progress, child.progress);
    if (child.progress.requirementsCheckable > 0) {
      checkableChildren += 1;
      if (child.outcome.outcome === 'met') {
        metChildren += 1;
      }
    } else if (
      child.outcome.outcome === 'needsStudentCheck' &&
      child.outcome.confirmed === undefined
    ) {
      openSelfChecks += 1;
    }
  }
  if (body.kind === 'select') {
    progress.requirementsMet = Math.min(metChildren, body.count);
    progress.requirementsCheckable = body.count;
    if (requirement.hours !== undefined) {
      progress.creditsRequired = creditRangeMin(requirement.hours);
    }
  }
  const met =
    body.kind === 'select'
      ? metChildren >= body.count
      : openSelfChecks === 0 &&
        (checkableChildren === 0 || metChildren === checkableChildren);
  const anyProgress = children.some(
    c =>
      c.outcome.outcome === 'met' ||
      c.outcome.outcome === 'partial' ||
      (c.outcome.outcome === 'needsStudentCheck' &&
        c.outcome.confirmed !== undefined),
  );
  const outcome: Outcome = met
    ? {outcome: 'met'}
    : anyProgress
      ? {outcome: 'partial'}
      : {outcome: 'unmet'};
  return {
    ...base,
    outcome,
    progress,
    filledBy: [],
    claimedBy: [],
    children,
  };
}

function evaluateProgram(
  plan: Plan,
  program: Program,
  cards: Card[],
  facts: CourseFacts,
  warnings: Warning[],
): ProgramReport {
  const confirmed = new Map<RequirementId, SelfCheckReason>();
  for (const check of plan.selfChecks) {
    confirmed.set(check.requirement, check.reason);
  }
  const claims = new Map<RequirementId, TermId>();
  for (const term of plan.terms) {
    for (const claim of term.nonCourse) {
      claims.set(claim.requirement, term.id);
    }
  }
  const {assigned, claimed} = matchCards(program, cards, facts, warnings);
  const consumed = new Set<EntryId>();
  for (const list of assigned.values()) {
    for (const card of list) {
      consumed.add(card.entry);
    }
  }
  const root = evaluateRequirement(
    program.root,
    program,
    assigned,
    claimed,
    consumed,
    cards,
    facts,
    confirmed,
    claims,
  );
  const progress = {...root.progress};
  if (program.totalCredits !== undefined) {
    progress.creditsRequired = program.totalCredits;
  }
  return {
    program: program.id,
    name: program.name,
    catalogYear: plan.catalogYear,
    evaluatedWith: program.catalogYear,
    root,
    progress,
    declaredCredits: program.totalCredits,
  };
}

function warningsFor(
  bundle: PlanBundle,
  reports: ProgramReport[],
  cards: Card[],
): Warning[] {
  const {plan, facts} = bundle;
  const out: Warning[] = [];

  // The catalog moved under a placed course: compare what it said when the
  // student added it with what it says now. Matching uses today's facts, so a
  // lost designation empties the slot and the student pins it with a basis.
  for (const term of plan.terms) {
    if (!isRiceTerm(term.kind)) {
      continue;
    }
    for (const c of term.kind.rice.courses) {
      const was = c.observed;
      const now = was === undefined ? undefined : courseInfo(facts, c.course);
      if (was === undefined || now === undefined) {
        continue;
      }
      const nowAttrs = new Set(now.attributes);
      const wasAttrs = new Set(was.attributes);
      const lost = was.attributes.filter(a => !nowAttrs.has(a));
      const gained = now.attributes.filter(a => !wasAttrs.has(a));
      const creditsBefore = creditRangeMin(was.credits);
      const creditsNow = creditRangeMin(now.credits);
      if (
        lost.length > 0 ||
        gained.length > 0 ||
        creditsBefore !== creditsNow
      ) {
        out.push({
          kind: 'courseFactsChanged',
          value: {
            term: term.id,
            entry: c.id,
            course: c.course,
            observedYear: was.catalogYear,
            lost,
            gained,
            ...(creditsBefore === creditsNow
              ? {}
              : {creditsBefore, creditsNow}),
          },
        });
      }
    }
  }

  // Hours typed by hand: a card with no Rice equivalent, or one the student
  // overrode. Rice posts what the registrar decides; we cannot check it.
  const manualCards = [
    ...plan.incomingCredit,
    ...plan.terms.flatMap(t => (isAwayTerm(t.kind) ? t.kind.away.cards : [])),
  ];
  for (const card of manualCards) {
    if (card.riceEquivalent === undefined || card.creditsSource === 'manual') {
      out.push({
        kind: 'manualCredits',
        value: {entry: card.id, credits: card.credits},
      });
    }
  }

  // Fills no requirement: a course card in no requirement's filledBy across every program.
  const filled = new Set<EntryId>();
  const walk = (r: RequirementReport): void => {
    for (const e of r.filledBy) {
      filled.add(e);
    }
    r.children.forEach(walk);
  };
  reports.forEach(r => walk(r.root));
  for (const term of plan.terms) {
    if (isRiceTerm(term.kind)) {
      for (const c of term.kind.rice.courses) {
        if (!filled.has(c.id)) {
          out.push({
            kind: 'fillsNoRequirement',
            value: {term: term.id, entry: c.id, course: c.course},
          });
        }
      }
    }
  }

  // Duplicates: same canonical code in two terms, unless repeatable or filling separate slots.
  const seen = new Map<
    string,
    {code: CourseCode; terms: TermId[]; entries: EntryId[]}
  >();
  // Incoming credit counts as a copy too; it just has no term to name.
  for (const card of cards) {
    if (card.code === undefined) {
      continue;
    }
    const key = courseKey(card.code);
    const entry = seen.get(key) ?? {code: card.code, terms: [], entries: []};
    if (card.term !== undefined) {
      entry.terms.push(card.term);
    }
    entry.entries.push(card.entry);
    seen.set(key, entry);
  }
  for (const dup of seen.values()) {
    if (dup.entries.length < 2) {
      continue;
    }
    if (courseInfo(facts, dup.code)?.repeatable) {
      continue;
    }
    if (dup.entries.every(e => filled.has(e))) {
      continue;
    }
    out.push({
      kind: 'duplicateCourse',
      value: {course: dup.code, terms: dup.terms},
    });
  }

  // Prerequisites, per Rice term card.
  const index = buildTakenIndex(plan, facts);
  for (const term of plan.terms) {
    if (!isRiceTerm(term.kind)) {
      continue;
    }
    for (const c of term.kind.rice.courses) {
      const row = bundle.prerequisites.find(
        p =>
          courseKey(canonical(facts, p.course)) ===
          courseKey(canonical(facts, c.course)),
      );
      if (row === undefined || row.fact.kind !== 'requires') {
        continue;
      }
      const result = evaluatePrereq(
        row.fact.value.expr,
        term.position,
        index,
        facts,
      );
      if (result.truth === 'unknown') {
        out.push({
          kind: 'prerequisiteUnparsed',
          value: {
            term: term.id,
            course: c.course,
            published: row.fact.value.published,
          },
        });
      } else if (result.truth === 'missing') {
        for (const missing of result.missing) {
          out.push({
            kind: 'prerequisite',
            value: {
              term: term.id,
              course: c.course,
              prerequisite: missing,
              problem: {kind: 'notInPlan'},
              publishedFor: row.fact.value.publishedFor,
            },
          });
        }
        for (const same of result.sameTerm) {
          out.push({
            kind: 'prerequisite',
            value: {
              term: term.id,
              course: c.course,
              prerequisite: same,
              problem: {kind: 'sameTerm'},
              publishedFor: row.fact.value.publishedFor,
            },
          });
        }
        for (const late of result.later) {
          out.push({
            kind: 'prerequisite',
            value: {
              term: term.id,
              course: c.course,
              prerequisite: late.course,
              problem: {kind: 'later', value: {prerequisiteTerm: late.term}},
              publishedFor: row.fact.value.publishedFor,
            },
          });
        }
      }
    }
  }

  // Mutual exclusion: both codes in the plan, one warning per pair.
  const warned = new Set<string>();
  for (const row of bundle.exclusions) {
    const a = index.get(courseKey(canonical(facts, row.blocked)));
    const b = index.get(courseKey(canonical(facts, row.blocker)));
    if (a?.term === undefined || b?.term === undefined) {
      continue;
    }
    const pairKey = [courseKey(row.blocked), courseKey(row.blocker)]
      .sort()
      .join('|');
    if (warned.has(pairKey)) {
      continue;
    }
    warned.add(pairKey);
    out.push({
      kind: 'mutuallyExclusive',
      value: {
        blocked: row.blocked,
        blockedTerm: a.term,
        blocker: row.blocker,
        blockerTerm: b.term,
        published: row.published,
      },
    });
  }

  // Semester load: informational only.
  for (const term of plan.terms) {
    if (!isRiceTerm(term.kind)) {
      continue;
    }
    const planned = plannedCredits(term);
    const normal =
      term.position.season === 'summer'
        ? bundle.limits.summer
        : bundle.limits.fallSpring;
    if (normal !== undefined && planned > normal) {
      out.push({
        kind: 'overSemesterLoad',
        value: {term: term.id, planned, normal},
      });
    }
  }

  // Self-checks, so an unverifiable requirement is never silent.
  for (const report of reports) {
    const program: ProgramId = report.program;
    const visit = (r: RequirementReport): void => {
      if (
        r.outcome.outcome === 'needsStudentCheck' &&
        r.outcome.confirmed === undefined
      ) {
        out.push({
          kind: 'selfCheck',
          value: {
            program,
            requirement: r.requirement,
            label: r.label,
            text: r.label,
          },
        });
      }
      r.children.forEach(visit);
    };
    visit(report.root);
  }

  return out;
}

/** Total: a report for any input, never a throw. */
export function evaluateInterim(bundle: PlanBundle): Report {
  const cards = collectCards(bundle.plan, bundle.facts);
  const matchWarnings: Warning[] = [];
  const reports = bundle.plan.programs.flatMap(id => {
    const program = bundle.programs.find(p => p.id === id);
    if (program === undefined) {
      matchWarnings.push({
        kind: 'programUnavailable',
        value: {program: id, catalogYear: bundle.plan.catalogYear},
      });
      return [];
    }
    return [
      evaluateProgram(bundle.plan, program, cards, bundle.facts, matchWarnings),
    ];
  });

  // Plan-level progress: a course in two programs is still one course.
  const progress = emptyProgress();
  for (const r of reports) {
    progress.requirementsMet += r.progress.requirementsMet;
    progress.requirementsClaimed += r.progress.requirementsClaimed;
    progress.requirementsCheckable += r.progress.requirementsCheckable;
    progress.creditsUnknown += r.progress.creditsUnknown;
    progress.selfChecks += r.progress.selfChecks;
    progress.selfChecksConfirmed += r.progress.selfChecksConfirmed;
  }
  progress.creditsMet = cards.reduce((sum, c) => sum + c.credits, 0);
  progress.creditsRequired = Math.max(
    0,
    ...reports.map(r => r.declaredCredits ?? 0),
  );

  return {
    plan: bundle.plan.id,
    engineVersion: INTERIM_ENGINE_VERSION,
    programs: reports,
    progress,
    warnings: [
      ...matchWarnings,
      ...warningsFor(bundle, reports, cards),
      ...doubleCounted(bundle, reports, cards),
    ],
  };
}

/**
 * One card in the slots of two programs, where at least one is a minor,
 * certificate or concentration. Rice sets no university-wide cap on courses
 * shared between two departmental majors (GA, "Majors, Minors, Certificates"),
 * but minors and certificates set their own overlap limits on their program
 * pages, which the rule tree cannot express. So this is a note, not a fault.
 */
function doubleCounted(
  bundle: PlanBundle,
  reports: ProgramReport[],
  cards: Card[],
): Warning[] {
  const programsOf = new Map<EntryId, Set<ProgramId>>();
  for (const report of reports) {
    const program = bundle.programs.find(p => p.id === report.program);
    if (program === undefined || program.kind === 'university') {
      continue;
    }
    const visit = (r: RequirementReport): void => {
      for (const entry of r.filledBy) {
        const set = programsOf.get(entry) ?? new Set<ProgramId>();
        set.add(report.program);
        programsOf.set(entry, set);
      }
      r.children.forEach(visit);
    };
    visit(report.root);
  }
  const out: Warning[] = [];
  const kindOf = (id: ProgramId): Program['kind'] | undefined =>
    bundle.programs.find(p => p.id === id)?.kind;
  for (const card of cards) {
    const programs = programsOf.get(card.entry);
    if (
      programs !== undefined &&
      programs.size > 1 &&
      card.code !== undefined &&
      [...programs].some(id => kindOf(id) !== 'major')
    ) {
      out.push({
        kind: 'doubleCounted',
        value: {entry: card.entry, course: card.code, programs: [...programs]},
      });
    }
  }
  return out;
}
