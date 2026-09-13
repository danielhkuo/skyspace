import {Button} from '@astryxdesign/core/Button';
import {CheckboxInput} from '@astryxdesign/core/CheckboxInput';
import {Dialog, DialogHeader} from '@astryxdesign/core/Dialog';
import {Layout, LayoutContent, LayoutFooter} from '@astryxdesign/core/Layout';
import {Selector, SelectorOption} from '@astryxdesign/core/Selector';
import type {SelectorOptionType} from '@astryxdesign/core/Selector';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {TextInput} from '@astryxdesign/core/TextInput';
import {useMemo, useState} from 'react';

import {
  courseInfo,
  creditRangeMin,
  creditsFromHours,
  formatCredits,
  formatCourseCode,
  parseCourseCode,
  walkRequirements,
  type CourseCode,
  type CreditOrigin,
  type EntryId,
  type FillBasis,
  type ManualCourseCard,
  type PlanBundle,
  type PlannedCourse,
  type Program,
  type RequirementId,
} from '../domain';
import {engine} from '../engine';
import {FILL_BASIS_LABEL, requirementPaths} from './labels';
import type {PlanAction} from './usePlan';

type EditCourseDialogProps = {
  entry: EntryId;
  card: PlannedCourse | ManualCourseCard;
  bundle: PlanBundle;
  dispatch: (action: PlanAction) => void;
  onClose: () => void;
};

const AUTO = 'auto';
const MAX_HOURS = 20;

const ORIGINS: {value: CreditOrigin; label: string}[] = [
  {value: 'transfer', label: 'Transfer'},
  {value: 'advancedPlacement', label: 'AP'},
  {value: 'internationalBaccalaureate', label: 'IB'},
  {value: 'studyAbroad', label: 'Study abroad'},
  {value: 'other', label: 'Other'},
];

type Option = {requirement: RequirementId; label: string; matches: boolean};

type Choice = {
  program: Program;
  /** Every course requirement, sibling slots collapsed to one; `matches` is the filter's verdict. */
  options: Option[];
  current: string;
};

function isPlanned(
  card: PlannedCourse | ManualCourseCard,
): card is PlannedCourse {
  return 'course' in card;
}

/** "3", "1.5", "" -> hundredths, or an error string. Empty is an error, never zero. */
function parseHours(raw: string): {credits?: number; error?: string} {
  const t = raw.trim();
  if (t === '' || !/^\d+(\.\d{1,2})?$/.test(t)) {
    return {error: 'A number of hours, like 3 or 1.5'};
  }
  const n = Number(t);
  if (n > MAX_HOURS) {
    return {error: `No more than ${MAX_HOURS} hours`};
  }
  return {credits: creditsFromHours(n)};
}

/**
 * The one place a card is edited: what it is, how many hours, and which
 * requirement it fills in each program. Reached from the card's ⋯ menu, the
 * sidebar's "change" link, and every warning that names a card.
 *
 * A pick the filter rejects asks for a basis. The engine keeps such a card
 * in the slot as a claim, never counts it as met, and repeats the basis on
 * the PDF: the honest answer for a course that counted under an older
 * catalog, or one an advisor approved.
 */
export function EditCourseDialog({
  entry,
  card,
  bundle,
  dispatch,
  onClose,
}: EditCourseDialogProps) {
  const planned = isPlanned(card);
  const [hours, setHours] = useState(formatCredits(card.credits));
  const [origin, setOrigin] = useState<CreditOrigin>(
    planned ? 'other' : card.origin,
  );
  const [code, setCode] = useState(planned ? '' : card.code);
  const [title, setTitle] = useState(planned ? '' : card.title);
  const [institution, setInstitution] = useState(
    planned ? '' : (card.institution ?? ''),
  );
  const [equivalent, setEquivalent] = useState(
    planned || card.riceEquivalent === undefined
      ? ''
      : formatCourseCode(card.riceEquivalent),
  );
  const [byHand, setByHand] = useState(
    !planned && card.creditsSource === 'manual',
  );

  const equivalentCode: CourseCode | undefined = planned
    ? card.course
    : (parseCourseCode(equivalent) ?? undefined);
  const equivalentBad =
    !planned && equivalent.trim() !== '' && equivalentCode === undefined;
  const equivalentInfo =
    planned || equivalentCode === undefined
      ? undefined
      : courseInfo(bundle.facts, equivalentCode);
  const hoursFromEquivalent =
    equivalentInfo === undefined
      ? undefined
      : creditRangeMin(equivalentInfo.credits);
  const hoursLocked = !planned && !byHand && hoursFromEquivalent !== undefined;
  const hoursParsed = hoursLocked
    ? {credits: hoursFromEquivalent}
    : parseHours(hours);
  const label = planned ? formatCourseCode(card.course) : card.code;
  const fills = card.fills;
  const claims = card.claims ?? [];
  const course = equivalentCode;

  const choices = useMemo<Choice[]>(
    () =>
      bundle.plan.programs.flatMap(id => {
        const program = bundle.programs.find(p => p.id === id);
        if (program === undefined) {
          return [];
        }
        const paths = requirementPaths(program);
        const pinned = fills.find(r => paths.has(r));
        const seen = new Set<string>();
        const options: Option[] = [];
        walkRequirements(program.root, req => {
          if (req.body.kind !== 'course') {
            return;
          }
          const path = paths.get(req.id) ?? req.label;
          if (seen.has(path) && req.id !== pinned) {
            return;
          }
          seen.add(path);
          options.push({
            requirement: req.id,
            label: path,
            matches:
              course !== undefined &&
              engine.requirementMatches(program, req.id, [course], bundle.facts)
                .length > 0,
          });
        });
        return options.length === 0
          ? []
          : [{program, options, current: pinned ?? AUTO}];
      }),
    [bundle, course, fills],
  );
  const [picked, setPicked] = useState<Record<string, string>>(() =>
    Object.fromEntries(choices.map(c => [c.program.id, c.current])),
  );
  const [bases, setBases] = useState<Record<string, FillBasis['kind']>>(() =>
    Object.fromEntries(
      choices.map(c => [
        c.program.id,
        claims.find(k => k.requirement === c.current)?.basis.kind ?? 'unsure',
      ]),
    ),
  );
  const [notes, setNotes] = useState<Record<string, string>>(() =>
    Object.fromEntries(
      choices.map(c => [
        c.program.id,
        claims.find(k => k.requirement === c.current)?.note ?? '',
      ]),
    ),
  );
  // With no Rice code there is nothing for Skyspace to decide: every pin is a claim.
  const canAuto = course !== undefined;
  const pickOf = (choice: Choice): string =>
    picked[choice.program.id] ?? (canAuto ? AUTO : '');
  const rejected = (choice: Choice): boolean => {
    const option = choice.options.find(o => o.requirement === pickOf(choice));
    return option !== undefined && !option.matches;
  };
  const anyRejected = choices.some(rejected);

  const dirty =
    hoursParsed.credits !== card.credits ||
    (!planned &&
      (origin !== card.origin ||
        code.trim() !== card.code ||
        title.trim() !== card.title ||
        institution.trim() !== (card.institution ?? '') ||
        equivalent.trim() !==
          (card.riceEquivalent === undefined
            ? ''
            : formatCourseCode(card.riceEquivalent)) ||
        byHand !== (card.creditsSource === 'manual'))) ||
    choices.some(c => {
      const next = pickOf(c);
      const claim = claims.find(k => k.requirement === next);
      return (
        next !== c.current ||
        (rejected(c) &&
          ((bases[c.program.id] ?? 'unsure') !==
            (claim?.basis.kind ?? 'unsure') ||
            (notes[c.program.id] ?? '').trim() !== (claim?.note ?? '')))
      );
    });
  const valid =
    hoursParsed.credits !== undefined &&
    !equivalentBad &&
    (planned || code.trim() !== '');

  const save = (): void => {
    if (hoursParsed.credits === undefined) {
      return;
    }
    if (hoursParsed.credits !== card.credits) {
      dispatch({type: 'setCredits', entry, credits: hoursParsed.credits});
    }
    if (!planned) {
      dispatch({
        type: 'editManualCard',
        entry,
        patch: {
          origin,
          code: code.trim(),
          title: title.trim(),
          institution:
            institution.trim() === '' ? undefined : institution.trim(),
          riceEquivalent: equivalentCode,
          creditsSource: byHand ? 'manual' : 'equivalent',
        },
      });
    }
    for (const choice of choices) {
      const next = pickOf(choice);
      if (next === '') {
        continue;
      }
      if (next !== choice.current) {
        if (next === AUTO) {
          dispatch({type: 'clearFill', entry, program: choice.program});
        } else {
          dispatch({
            type: 'setFills',
            entry,
            program: choice.program,
            requirement: next as RequirementId,
          });
        }
      }
      if (next !== AUTO && rejected(choice)) {
        const note = (notes[choice.program.id] ?? '').trim();
        dispatch({
          type: 'setFillClaim',
          entry,
          claim: {
            requirement: next as RequirementId,
            basis: {kind: bases[choice.program.id] ?? 'unsure'},
            ...(note === '' ? {} : {note}),
          },
        });
      }
    }
    onClose();
  };

  const requirementOptions = (choice: Choice): SelectorOptionType[] => {
    const matching = choice.options.filter(o => o.matches);
    const rest = choice.options.filter(o => !o.matches);
    const toOption = (o: Option) => ({
      value: o.requirement,
      label: o.label,
      description: o.matches ? undefined : 'Not as published; on your say-so',
      icon: o.matches ? ('check' as const) : ('warning' as const),
    });
    return [
      ...(canAuto
        ? [
            {
              value: AUTO,
              label: 'Let Skyspace decide',
              description: 'Whichever open requirement it fits',
            },
          ]
        : []),
      ...(matching.length > 0
        ? [
            {
              type: 'section' as const,
              title: 'Matches',
              options: matching.map(toOption),
            },
          ]
        : []),
      ...(rest.length > 0
        ? [
            {
              type: 'section' as const,
              title: 'Every requirement',
              options: rest.map(toOption),
            },
          ]
        : []),
    ];
  };

  const close = (open: boolean): void => {
    if (!open) {
      onClose();
    }
  };

  return (
    <Dialog
      isOpen
      onOpenChange={close}
      purpose="form"
      width={480}
      maxHeight="85dvh"
    >
      <Layout
        header={
          <DialogHeader
            title={label}
            subtitle={planned ? undefined : 'A course from outside Rice'}
            onOpenChange={close}
          />
        }
        content={
          <LayoutContent>
            <Stack width="100%" gap={3} align="start">
              {!planned && (
                <Stack width="100%" gap={2} align="start">
                  <Stack
                    direction="horizontal"
                    width="100%"
                    gap={2}
                    align="start"
                  >
                    <Selector
                      label="Origin"
                      size="sm"
                      value={origin}
                      options={ORIGINS}
                      onChange={v => setOrigin(v as CreditOrigin)}
                      width={132}
                    />
                    <TextInput
                      label="Their code"
                      size="sm"
                      value={code}
                      onChange={setCode}
                      placeholder="INF 3221"
                      width={132}
                      status={
                        code.trim() === ''
                          ? {type: 'error', message: 'Required'}
                          : undefined
                      }
                    />
                    <TextInput
                      label="Title"
                      size="sm"
                      value={title}
                      onChange={setTitle}
                      width="100%"
                    />
                  </Stack>
                  <TextInput
                    label="Institution"
                    isOptional
                    size="sm"
                    value={institution}
                    onChange={setInstitution}
                    placeholder="Universidad Politécnica de Madrid"
                    width="100%"
                  />
                </Stack>
              )}

              <Stack direction="horizontal" width="100%" gap={2} align="start">
                <TextInput
                  label="Credit hours"
                  description={
                    hoursLocked
                      ? `From ${equivalent.trim()}`
                      : planned
                        ? undefined
                        : 'By hand: hours only, flagged'
                  }
                  size="sm"
                  value={
                    hoursLocked
                      ? formatCredits(hoursFromEquivalent ?? 0)
                      : hours
                  }
                  onChange={setHours}
                  isReadOnly={hoursLocked}
                  width={132}
                  status={
                    !hoursLocked && hoursParsed.error !== undefined
                      ? {type: 'error', message: hoursParsed.error}
                      : undefined
                  }
                />
                {!planned && (
                  <TextInput
                    label="Rice equivalent"
                    isOptional
                    description="What the registrar posts it as"
                    size="sm"
                    value={equivalent}
                    onChange={setEquivalent}
                    placeholder="COMP 321"
                    width="100%"
                    status={
                      equivalentBad
                        ? {type: 'error', message: 'Not a Rice code'}
                        : undefined
                    }
                  />
                )}
              </Stack>
              {!planned && hoursFromEquivalent !== undefined && (
                <CheckboxInput
                  label="Set the hours by hand"
                  description="Only if the registrar posted different hours; flagged as unverified"
                  size="sm"
                  value={byHand}
                  onChange={setByHand}
                  width="100%"
                />
              )}

              {choices.map(choice => (
                <Stack
                  key={choice.program.id}
                  width="100%"
                  gap={1.5}
                  align="start"
                >
                  <Selector
                    label={choice.program.name}
                    description={
                      canAuto
                        ? undefined
                        : 'No Rice code, so nothing matches; pick where it should count'
                    }
                    size="sm"
                    width="100%"
                    placeholder="Pick a requirement"
                    hasSearch={choice.options.length > 8}
                    value={pickOf(choice) === '' ? undefined : pickOf(choice)}
                    options={requirementOptions(choice)}
                    onChange={v =>
                      setPicked(prev => ({...prev, [choice.program.id]: v}))
                    }
                    renderOption={o => (
                      <SelectorOption
                        label={o.label ?? o.value}
                        description={o.description}
                        icon={o.icon}
                        layout="inline"
                      />
                    )}
                    status={
                      rejected(choice)
                        ? {
                            type: 'warning',
                            message: 'Counts on your say-so, never as met',
                          }
                        : undefined
                    }
                  />
                  {rejected(choice) && (
                    <Stack
                      direction="horizontal"
                      width="100%"
                      gap={2}
                      align="start"
                    >
                      <Selector
                        label="Why it counts"
                        size="sm"
                        width="50%"
                        value={bases[choice.program.id] ?? 'unsure'}
                        options={(
                          Object.keys(FILL_BASIS_LABEL) as FillBasis['kind'][]
                        ).map(kind => ({
                          value: kind,
                          label: FILL_BASIS_LABEL[kind],
                        }))}
                        onChange={v =>
                          setBases(prev => ({
                            ...prev,
                            [choice.program.id]: v as FillBasis['kind'],
                          }))
                        }
                      />
                      <TextInput
                        label="Note"
                        isOptional
                        size="sm"
                        value={notes[choice.program.id] ?? ''}
                        onChange={v =>
                          setNotes(prev => ({...prev, [choice.program.id]: v}))
                        }
                        placeholder="Who approved it, or where it is posted"
                        width="50%"
                      />
                    </Stack>
                  )}
                </Stack>
              ))}
              {choices.length === 0 && (
                <Text type="supporting">
                  No program in this plan has course requirements.
                </Text>
              )}
            </Stack>
          </LayoutContent>
        }
        footer={
          <LayoutFooter>
            <Stack
              direction="horizontal"
              width="100%"
              gap={2}
              hAlign={anyRejected ? 'between' : 'end'}
              vAlign="center"
            >
              {anyRejected && (
                <Text type="supporting">
                  Skyspace can&apos;t verify a pick that doesn&apos;t match; it
                  stays out of your met count and is noted on the PDF.
                </Text>
              )}
              <Stack direction="horizontal" gap={1.5}>
                <Button
                  label="Cancel"
                  variant="ghost"
                  size="sm"
                  onClick={onClose}
                />
                <Button
                  label="Save"
                  variant="primary"
                  size="sm"
                  isDisabled={!valid || !dirty}
                  onClick={save}
                />
              </Stack>
            </Stack>
          </LayoutFooter>
        }
      />
    </Dialog>
  );
}
