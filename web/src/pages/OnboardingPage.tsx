import {Button} from '@astryxdesign/core/Button';
import {Section} from '@astryxdesign/core/Section';
import {Selector} from '@astryxdesign/core/Selector';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {TextInput} from '@astryxdesign/core/TextInput';
import {Token} from '@astryxdesign/core/Token';
import {useEffect, useMemo, useState} from 'react';
import {useNavigate} from 'react-router';

import {AlertDialog} from '@astryxdesign/core/AlertDialog';

import {dataSource} from '../datasource';
import {setLeaveGuard} from '../shell/leaveGuard';
import {
  compareTermPosition,
  courseInfo,
  creditRangeMin,
  creditsFromHours,
  formatCatalogYear,
  newEntryId,
  newTermId,
  nextTermPosition,
  parseCourseCode,
  shortTermLabel,
  type CatalogYear,
  type CourseFacts,
  type CreditOrigin,
  type ManualCourseCard,
  type Plan,
  type PlanId,
  type PlanTerm,
  type ProgramId,
  type ProgramSummary,
  type TermPosition,
} from '../domain';
import {islandHead, rowDivider} from '../plan/paint';
import {ProgramPicker} from '../plan/ProgramPicker';

type Step = 1 | 2 | 3;

const ORIGINS: {value: CreditOrigin; label: string}[] = [
  {value: 'advancedPlacement', label: 'AP'},
  {value: 'internationalBaccalaureate', label: 'IB'},
  {value: 'transfer', label: 'Transfer'},
  {value: 'studyAbroad', label: 'Study abroad'},
  {value: 'other', label: 'Other'},
];

const encode = (p: TermPosition): string => `${p.academicYear}-${p.season}`;
const decode = (v: string): TermPosition => {
  const [year, season] = v.split('-');
  return {
    academicYear: Number(year),
    season:
      season === 'spring' ? 'spring' : season === 'summer' ? 'summer' : 'fall',
  };
};

/** Fall and spring positions the pickers offer, Fall 2019 to Spring 2033. */
function semesterOptions(): {value: string; label: string}[] {
  const out: {value: string; label: string}[] = [];
  for (let y = 2020; y <= 2033; y += 1) {
    for (const season of ['fall', 'spring'] as const) {
      const p: TermPosition = {academicYear: y, season};
      out.push({value: encode(p), label: shortTermLabel(p)});
    }
  }
  return out;
}

/** Every fall and spring from matriculation to graduation, as empty Rice terms. */
function termsBetween(from: TermPosition, to: TermPosition): PlanTerm[] {
  const out: PlanTerm[] = [];
  let p = from;
  let guard = 0;
  while (compareTermPosition(p, to) <= 0 && guard < 40) {
    if (p.season !== 'summer') {
      out.push({
        id: newTermId(),
        position: p,
        kind: {rice: {courses: []}},
        nonCourse: [],
      });
    }
    p = nextTermPosition(p);
    guard += 1;
  }
  return out;
}

/**
 * Three steps, once: programs, timeline, incoming credit (`Plan 7 Settings
 * and Onboarding`). Afterwards every field is editable in plan settings.
 * The demo holds one plan, so finishing replaces it and says so.
 */
export function OnboardingPage() {
  const navigate = useNavigate();
  const [available, setAvailable] = useState<ProgramSummary[]>([]);
  const [facts, setFacts] = useState<CourseFacts | undefined>(undefined);
  const [step, setStep] = useState<Step>(1);
  const [majors, setMajors] = useState<ProgramId[]>([]);
  const [minors, setMinors] = useState<ProgramId[]>([]);
  const [catalogYear, setCatalogYear] = useState<CatalogYear>(2026);
  const [matriculation, setMatriculation] = useState<TermPosition>({
    academicYear: 2025,
    season: 'fall',
  });
  const [graduation, setGraduation] = useState<TermPosition>({
    academicYear: 2029,
    season: 'spring',
  });
  const [incoming, setIncoming] = useState<ManualCourseCard[]>([]);
  const [confirming, setConfirming] = useState(false);
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | undefined>(undefined);
  const [draft, setDraft] = useState({
    origin: 'advancedPlacement' as CreditOrigin,
    code: '',
    title: '',
    hours: '3',
    equivalent: '',
  });

  useEffect(() => {
    void dataSource.listPrograms().then(setAvailable);
    // A guest has no bundle; the credit step then takes hours by hand.
    void dataSource
      .loadBundle()
      .then(b => setFacts(b.facts))
      .catch(() => setFacts(undefined));
  }, []);
  // Anything chosen is worth a question before it is thrown away.
  const dirty =
    majors.length > 0 || minors.length > 0 || incoming.length > 0 || step > 1;
  useEffect(() => {
    setLeaveGuard(
      dirty
        ? 'Leave without creating the plan? What you chose here will be lost.'
        : undefined,
    );
    return () => setLeaveGuard(undefined);
  }, [dirty]);
  const timelineProblem =
    compareTermPosition(graduation, matriculation) <= 0
      ? 'Graduation must come after matriculation.'
      : catalogYear < matriculation.academicYear - 1 ||
          catalogYear > graduation.academicYear - 1
        ? 'Pick a catalog year between matriculation and graduation.'
        : undefined;
  const semesters = useMemo(() => semesterOptions(), []);
  const years = useMemo(() => {
    const out: CatalogYear[] = [];
    for (
      let y = matriculation.academicYear - 1;
      y <=
      Math.max(graduation.academicYear - 1, matriculation.academicYear - 1);
      y += 1
    ) {
      out.push(y);
    }
    return out;
  }, [matriculation, graduation]);

  /** The store may mint the id; the board loads whatever it kept. */
  const finish = (): void => {
    const university = available
      .filter(p => p.kind === 'university')
      .map(p => p.id);
    const majorNames = majors
      .map(id => available.find(p => p.id === id)?.name)
      .filter((n): n is string => n !== undefined);
    const plan: Plan = {
      id: crypto.randomUUID() as PlanId,
      name: majorNames.length > 0 ? majorNames.join(' + ') : 'My plan',
      catalogYear,
      matriculation,
      programs: [...university, ...majors, ...minors],
      incomingCredit: incoming,
      terms: termsBetween(matriculation, graduation),
      selfChecks: [],
    };
    setCreating(true);
    setCreateError(undefined);
    void dataSource
      .createPlan(plan)
      .then(() => {
        setLeaveGuard(undefined);
        void navigate('/plan');
      })
      .catch(() => {
        setCreateError(
          'The plan could not be created. Nothing you chose here was lost; try again.',
        );
        setCreating(false);
      });
  };

  const draftEquivalent = parseCourseCode(draft.equivalent);
  const draftInfo =
    draftEquivalent === null || facts === undefined
      ? undefined
      : courseInfo(facts, draftEquivalent);
  const addCredit = (): void => {
    const hours =
      draftInfo === undefined
        ? Number(draft.hours)
        : creditRangeMin(draftInfo.credits) / 100;
    if (draft.code.trim() === '' || Number.isNaN(hours)) {
      return;
    }
    const equivalent = parseCourseCode(draft.equivalent);
    setIncoming(prev => [
      ...prev,
      {
        id: newEntryId(),
        origin: draft.origin,
        code: draft.code.trim(),
        title: draft.title.trim(),
        credits: creditsFromHours(hours),
        fills: [],
        ...(equivalent === null
          ? {creditsSource: 'manual' as const}
          : {riceEquivalent: equivalent}),
      },
    ]);
    setDraft(d => ({...d, code: '', title: '', equivalent: ''}));
  };

  const crumb = (n: Step, label: string): React.ReactNode =>
    n === step ? (
      <Token label={`${n} · ${label}`} size="sm" color="blue" />
    ) : n < step ? (
      <Token label={`${n} · ${label}`} size="sm" onClick={() => setStep(n)} />
    ) : (
      <Text type="supporting">
        {n} · {label}
      </Text>
    );

  return (
    <Stack width="100%" height="100%" align="center" isScrollable>
      <Stack width={640} maxWidth="100%" paddingBlock={4} paddingInline={3}>
        <Section variant="section" padding={0} width="100%">
          <Stack width="100%" gap={0}>
            <Stack
              width="100%"
              padding={3}
              gap={2}
              align="start"
              style={islandHead}
            >
              <Text type="supporting">
                You only do this once. Afterwards every field is editable in
                plan settings.
                {dataSource.kind === 'demo'
                  ? ' The demo holds one plan, so finishing replaces the sample plan.'
                  : ''}
              </Text>
              <Stack
                direction="horizontal"
                width="100%"
                gap={1.5}
                vAlign="center"
              >
                {crumb(1, 'Programs')}
                <Text type="supporting">›</Text>
                {crumb(2, 'Timeline')}
                <Text type="supporting">›</Text>
                {crumb(3, 'Credit')}
              </Stack>
            </Stack>

            <Stack width="100%" padding={3} gap={3} align="start">
              {step === 1 && (
                <>
                  <ProgramPicker
                    label="Majors"
                    placeholder={`Search ${available.length} programs by name`}
                    kinds={['major']}
                    available={available}
                    chosen={majors}
                    onChange={setMajors}
                  />
                  <ProgramPicker
                    label="Minors"
                    isOptional
                    placeholder="Search minors"
                    kinds={['minor', 'certificate', 'concentration']}
                    available={available}
                    chosen={minors}
                    onChange={setMinors}
                  />
                  <Text type="supporting">
                    Reviewed requirements exist for a limited number of
                    programs. A program without reviewed requirements still
                    counts toward University requirements.
                  </Text>
                </>
              )}
              {step === 2 && (
                <>
                  <Stack
                    direction="horizontal"
                    width="100%"
                    gap={2}
                    align="start"
                  >
                    <Selector
                      label="Matriculation"
                      size="sm"
                      value={encode(matriculation)}
                      options={semesters}
                      onChange={v => setMatriculation(decode(v))}
                      width="50%"
                    />
                    <Selector
                      label="Expected graduation"
                      description="Sets how many term columns you start with; add or remove terms later"
                      size="sm"
                      value={encode(graduation)}
                      options={semesters}
                      onChange={v => setGraduation(decode(v))}
                      width="50%"
                      status={
                        compareTermPosition(graduation, matriculation) <= 0
                          ? {
                              type: 'error',
                              message: 'Must come after matriculation',
                            }
                          : undefined
                      }
                    />
                  </Stack>
                  <Selector
                    label="Catalog year"
                    description="You can follow any year between matriculation and graduation"
                    size="sm"
                    value={String(catalogYear)}
                    options={years.map(y => ({
                      value: String(y),
                      label: formatCatalogYear(y),
                    }))}
                    onChange={v => setCatalogYear(Number(v))}
                    width="100%"
                  />
                  <Text type="supporting">
                    {termsBetween(matriculation, graduation).length} term
                    columns, fall and spring only. Add summers later from plan
                    settings.
                  </Text>
                </>
              )}
              {step === 3 && (
                <>
                  <Text size="sm" color="secondary">
                    AP, IB, transfer and study-abroad credit you already hold.
                    Skip this if you have none; you can add cards later.
                  </Text>
                  <Stack
                    direction="horizontal"
                    width="100%"
                    gap={1.5}
                    align="end"
                    wrap="wrap"
                  >
                    <Selector
                      label="Origin"
                      size="sm"
                      value={draft.origin}
                      options={ORIGINS}
                      onChange={v =>
                        setDraft(d => ({...d, origin: v as CreditOrigin}))
                      }
                      width={140}
                    />
                    <TextInput
                      label="Code"
                      size="sm"
                      value={draft.code}
                      onChange={v => setDraft(d => ({...d, code: v}))}
                      placeholder="AP Calc BC"
                      width={140}
                    />
                    <TextInput
                      label="Title"
                      size="sm"
                      value={draft.title}
                      onChange={v => setDraft(d => ({...d, title: v}))}
                      placeholder="Calculus"
                      width={160}
                    />
                    <TextInput
                      label="Hours"
                      size="sm"
                      value={
                        draftInfo === undefined
                          ? draft.hours
                          : String(creditRangeMin(draftInfo.credits) / 100)
                      }
                      onChange={v => setDraft(d => ({...d, hours: v}))}
                      isReadOnly={draftInfo !== undefined}
                      width={72}
                    />
                    <TextInput
                      label="Rice equivalent"
                      isOptional
                      size="sm"
                      value={draft.equivalent}
                      onChange={v => setDraft(d => ({...d, equivalent: v}))}
                      placeholder="MATH 105"
                      width={140}
                      onEnter={addCredit}
                    />
                    <Button
                      label="Add"
                      variant="secondary"
                      size="sm"
                      onClick={addCredit}
                    />
                  </Stack>
                  <Stack direction="horizontal" gap={1} wrap="wrap">
                    {incoming.map(card => (
                      <Token
                        key={card.id}
                        label={`${card.code} · ${card.credits / 100} cr${card.riceEquivalent === undefined ? '' : ` · ${card.riceEquivalent.subject} ${card.riceEquivalent.number}`}`}
                        size="sm"
                        onRemove={() =>
                          setIncoming(prev =>
                            prev.filter(c => c.id !== card.id),
                          )
                        }
                      />
                    ))}
                  </Stack>
                  <Text type="supporting">
                    Assign each card its Rice equivalent to apply its credit
                    hours. Cards without an equivalent will only count for raw
                    hours. Only your official transfer evaluation from the
                    registrar is final. count toward distribution.
                  </Text>
                </>
              )}
            </Stack>

            <Stack
              direction="horizontal"
              width="100%"
              padding={3}
              gap={1.5}
              hAlign="between"
              style={rowDivider}
            >
              <Button
                label="Back"
                variant="ghost"
                size="sm"
                isDisabled={step === 1}
                onClick={() => setStep(s => (s === 3 ? 2 : 1))}
              />
              {step < 3 ? (
                <Button
                  label="Continue"
                  variant="primary"
                  size="sm"
                  isDisabled={
                    (step === 1 && majors.length === 0) ||
                    (step === 2 && timelineProblem !== undefined)
                  }
                  tooltip={
                    step === 2
                      ? timelineProblem
                      : step === 1 && majors.length === 0
                        ? 'Pick at least one major'
                        : undefined
                  }
                  onClick={() => setStep(s => (s === 1 ? 2 : 3))}
                />
              ) : (
                <Stack direction="horizontal" gap={1.5} vAlign="center">
                  {createError !== undefined && (
                    <Text
                      size="sm"
                      role="alert"
                      style={{color: 'var(--color-text-red)'}}
                    >
                      {createError}
                    </Text>
                  )}
                  <Button
                    label="Create plan"
                    variant="primary"
                    size="sm"
                    isLoading={creating}
                    onClick={() =>
                      dataSource.kind === 'demo'
                        ? setConfirming(true)
                        : finish()
                    }
                  />
                </Stack>
              )}
            </Stack>
          </Stack>
        </Section>
      </Stack>
      <AlertDialog
        isOpen={confirming}
        onOpenChange={setConfirming}
        title="Replace the current plan?"
        description="The demo holds one plan. Creating this one replaces the plan on the board, including its courses and self-checks."
        cancelLabel="Keep the current plan"
        actionLabel="Replace it"
        actionVariant="destructive"
        onAction={() => {
          setConfirming(false);
          finish();
        }}
      />
    </Stack>
  );
}
