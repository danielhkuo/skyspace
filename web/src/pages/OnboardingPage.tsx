import {Button} from '@astryxdesign/core/Button';
import {Card} from '@astryxdesign/core/Card';
import {Selector} from '@astryxdesign/core/Selector';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {TextInput} from '@astryxdesign/core/TextInput';
import {Token} from '@astryxdesign/core/Token';
import {useEffect, useMemo, useState} from 'react';
import {useNavigate} from 'react-router';

import {dataSource} from '../datasource';
import {setLeaveGuard} from '../shell/leaveGuard';
import {queuePlanSave} from '../datasource/planSaver';
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
  type Program,
  type ProgramId,
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
  const [available, setAvailable] = useState<Program[]>([]);
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
  const [draft, setDraft] = useState({
    origin: 'advancedPlacement' as CreditOrigin,
    code: '',
    title: '',
    hours: '3',
    equivalent: '',
  });

  useEffect(() => {
    void dataSource.listPrograms().then(setAvailable);
    void dataSource.loadBundle().then(b => setFacts(b.facts));
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
          catalogYear > graduation.academicYear
        ? 'Pick a catalog year between matriculation and graduation.'
        : undefined;
  const semesters = useMemo(() => semesterOptions(), []);
  const years = useMemo(() => {
    const out: CatalogYear[] = [];
    for (
      let y = matriculation.academicYear - 1;
      y <= graduation.academicYear;
      y += 1
    ) {
      out.push(y);
    }
    return out;
  }, [matriculation, graduation]);

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
    queuePlanSave(plan, 0);
    setLeaveGuard(undefined);
    void navigate('/plan');
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
        <Card padding={0} width="100%">
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
                    Rice has 351 programs; reviewed requirements exist for a
                    few. A program without reviewed requirements still counts
                    toward University requirements.
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
                    description="Rice lets you follow any year between when you matriculated and when you graduate"
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
                    Give each card the Rice course it posts as; the hours then
                    come from that course. A card with no Rice equivalent counts
                    as hours only and is flagged. Only the registrar&apos;s
                    evaluation makes an equivalence official; AP and IB never
                    count toward distribution.
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
                <Button
                  label="Create plan"
                  variant="primary"
                  size="sm"
                  onClick={finish}
                />
              )}
            </Stack>
          </Stack>
        </Card>
      </Stack>
    </Stack>
  );
}
