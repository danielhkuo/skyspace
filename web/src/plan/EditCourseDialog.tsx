import {Button} from '@astryxdesign/core/Button';
import {CheckboxInput} from '@astryxdesign/core/CheckboxInput';
import {Dialog} from '@astryxdesign/core/Dialog';
import {Link} from '@astryxdesign/core/Link';
import {RadioList, RadioListItem} from '@astryxdesign/core/RadioList';
import {Selector} from '@astryxdesign/core/Selector';
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
import {islandHead, rowDivider} from './paint';
import type {PlanAction} from './usePlan';

type EditCourseDialogProps = {
  entry: EntryId;
  card: PlannedCourse | ManualCourseCard;
  bundle: PlanBundle;
  dispatch: (action: PlanAction) => void;
  onClose: () => void;
};

const AUTO = 'auto';

const ORIGINS: {value: CreditOrigin; label: string}[] = [
  {value: 'transfer', label: 'Transfer'},
  {value: 'advancedPlacement', label: 'AP'},
  {value: 'internationalBaccalaureate', label: 'IB'},
  {value: 'studyAbroad', label: 'Study abroad'},
  {value: 'other', label: 'Other'},
];

type Choice = {
  program: Program;
  /** Every course requirement; `matches` says whether the filter accepts the card. */
  options: {requirement: RequirementId; label: string; matches: boolean}[];
  current: string;
};

function isPlanned(
  card: PlannedCourse | ManualCourseCard,
): card is PlannedCourse {
  return 'course' in card;
}

/**
 * The one place a card is edited (`Plan 6 Requirement Choice`, folded with
 * card details): what it is, how many hours, and which requirement it
 * fills in each program. Reached from the card's ⋯ menu, the sidebar's
 * "change" link, and every warning that names a card.
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
  // Details, edited locally and written on Save.
  const [hours, setHours] = useState(String(card.credits / 100));
  const [origin, setOrigin] = useState<CreditOrigin>(
    planned ? 'other' : card.origin,
  );
  const [code, setCode] = useState(planned ? '' : card.code);
  const [title, setTitle] = useState(planned ? '' : card.title);
  const [institution, setInstitution] = useState(
    planned ? '' : (card.institution ?? ''),
  );
  const [equivalent, setEquivalent] = useState(
    planned
      ? ''
      : card.riceEquivalent === undefined
        ? ''
        : formatCourseCode(card.riceEquivalent),
  );
  const equivalentCode = planned
    ? card.course
    : (parseCourseCode(equivalent) ?? undefined);
  const [byHand, setByHand] = useState(
    !planned && card.creditsSource === 'manual',
  );
  // The Rice equivalent's published hours, when we hold them.
  const equivalentInfo =
    planned || equivalentCode === undefined
      ? undefined
      : courseInfo(bundle.facts, equivalentCode);
  const hoursFromEquivalent =
    equivalentInfo === undefined
      ? undefined
      : creditRangeMin(equivalentInfo.credits);
  const hoursLocked = !planned && !byHand && hoursFromEquivalent !== undefined;
  const equivalentBad =
    !planned && equivalent.trim() !== '' && equivalentCode === undefined;
  const label = planned ? formatCourseCode(card.course) : card.code;
  const fills = card.fills;
  const claims = card.claims ?? [];

  // The matcher's view uses the Rice code as it will be after Save.
  const course: CourseCode | undefined = equivalentCode;
  const choices = useMemo<Choice[]>(
    () =>
      bundle.plan.programs.flatMap(id => {
        const program = bundle.programs.find(p => p.id === id);
        if (program === undefined) {
          return [];
        }
        const paths = requirementPaths(program);
        const pinned = fills.find(r => paths.has(r));
        // Sibling slots share a label (three "Distribution Group III" rows);
        // one option per label, the first id standing for the run.
        const seen = new Set<string>();
        const options: Choice['options'] = [];
        walkRequirements(program.root, req => {
          if (req.body.kind !== 'course') {
            return;
          }
          const path = paths.get(req.id) ?? req.label;
          const matches =
            course !== undefined &&
            engine.requirementMatches(program, req.id, [course], bundle.facts)
              .length > 0;
          if (seen.has(path) && req.id !== pinned) {
            return;
          }
          seen.add(path);
          options.push({requirement: req.id, label: path, matches});
        });
        return [{program, options, current: pinned ?? AUTO}];
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
  const [showAll, setShowAll] = useState<Record<string, boolean>>({});
  const visible = (choice: Choice): Choice['options'] =>
    showAll[choice.program.id]
      ? choice.options
      : choice.options.filter(
          o => o.matches || o.requirement === choice.current,
        );
  const rejected = (choice: Choice): boolean => {
    const req = picked[choice.program.id];
    const option = choice.options.find(o => o.requirement === req);
    return option !== undefined && !option.matches;
  };
  // With no Rice code there is nothing for Skyspace to decide: every pin is a claim.
  const canAuto = course !== undefined;

  const save = (): void => {
    const parsedHours = Number(hours);
    const credits = hoursLocked
      ? hoursFromEquivalent
      : Number.isNaN(parsedHours)
        ? undefined
        : creditsFromHours(parsedHours);
    if (credits !== undefined && credits !== card.credits) {
      dispatch({type: 'setCredits', entry, credits});
    }
    if (!planned) {
      dispatch({
        type: 'editManualCard',
        entry,
        patch: {
          origin,
          code: code.trim() === '' ? card.code : code.trim(),
          title: title.trim(),
          institution:
            institution.trim() === '' ? undefined : institution.trim(),
          riceEquivalent: equivalentCode,
          creditsSource: byHand ? 'manual' : 'equivalent',
        },
      });
    }
    for (const choice of choices) {
      const next = picked[choice.program.id] ?? AUTO;
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

  return (
    <Dialog
      isOpen
      onOpenChange={open => {
        if (!open) {
          onClose();
        }
      }}
      width={480}
      padding={0}
      maxHeight="90vh"
    >
      <Stack width="100%" gap={0}>
        <Stack
          width="100%"
          padding={2}
          gap={0.5}
          align="start"
          style={islandHead}
        >
          <Text as="h3" weight="semibold">
            {label}
          </Text>
          <Text type="supporting">
            {planned
              ? 'Hours, and the requirement it fills in each program.'
              : 'A course from outside Rice: what it was, its hours, and where it counts.'}
          </Text>
        </Stack>

        <Stack width="100%" padding={2} gap={1.5} align="start">
          <Text type="label" weight="semibold">
            Details
          </Text>
          {!planned && (
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
                value={origin}
                options={ORIGINS}
                onChange={v => setOrigin(v as CreditOrigin)}
                width={140}
              />
              <TextInput
                label="Their code"
                size="sm"
                value={code}
                onChange={setCode}
                placeholder="INF 3221"
                width={140}
              />
              <TextInput
                label="Title"
                size="sm"
                value={title}
                onChange={setTitle}
                width={140}
              />
            </Stack>
          )}
          {!planned && (
            <TextInput
              label="Institution"
              isOptional
              size="sm"
              value={institution}
              onChange={setInstitution}
              placeholder="Universidad Politécnica de Madrid"
              width="100%"
            />
          )}
          <Stack
            direction="horizontal"
            width="100%"
            gap={1.5}
            align="end"
            wrap="wrap"
          >
            <TextInput
              label="Credit hours"
              description={
                planned
                  ? 'Rice publishes a range for some courses; set what you will take.'
                  : hoursLocked
                    ? `${formatCredits(hoursFromEquivalent ?? 0)} hours, from ${equivalent.trim()}: credit posts as that course.`
                    : 'Typed by hand: counts as hours, not as a course, and is flagged.'
              }
              size="sm"
              value={
                hoursLocked ? formatCredits(hoursFromEquivalent ?? 0) : hours
              }
              onChange={setHours}
              isReadOnly={hoursLocked}
              width={200}
            />
            {!planned && (
              <TextInput
                label="Rice equivalent"
                isOptional
                description="The Rice course the registrar posts it as. Without one, Skyspace can match it to nothing."
                size="sm"
                value={equivalent}
                onChange={setEquivalent}
                placeholder="COMP 321"
                width={200}
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
              description="Only when the registrar posted hours that differ from the course. Flagged as unverified."
              size="sm"
              value={byHand}
              onChange={setByHand}
              width="100%"
            />
          )}
        </Stack>

        {choices.map(choice => (
          <Stack
            key={choice.program.id}
            width="100%"
            padding={2}
            gap={1.5}
            align="start"
            style={rowDivider}
          >
            <Text type="label" weight="semibold">
              Fills in {choice.program.name}
            </Text>
            {visible(choice).length === 0 && !canAuto && (
              <Text type="supporting">
                Nothing matches a course without a Rice code. Pick a requirement
                below and say why it should count.
              </Text>
            )}
            {(visible(choice).length > 0 || canAuto) && (
              <RadioList
                isLabelHidden
                label={`Requirement in ${choice.program.name}`}
                value={picked[choice.program.id] ?? (canAuto ? AUTO : '')}
                onChange={v =>
                  setPicked(prev => ({...prev, [choice.program.id]: v}))
                }
                size="sm"
                width="100%"
              >
                {visible(choice).map(o => (
                  <RadioListItem
                    key={o.requirement}
                    value={o.requirement}
                    label={o.label}
                    description={
                      o.matches
                        ? undefined
                        : "Doesn't match as published; counts on your say-so and is flagged"
                    }
                  />
                ))}
                {canAuto && (
                  <RadioListItem
                    value={AUTO}
                    label="Let Skyspace decide"
                    description="Whichever open requirement it fits, recomputed on every change"
                  />
                )}
              </RadioList>
            )}
            {!showAll[choice.program.id] &&
              visible(choice).length < choice.options.length && (
                <Link
                  href="#"
                  size="sm"
                  onClick={e => {
                    e.preventDefault();
                    setShowAll(prev => ({...prev, [choice.program.id]: true}));
                  }}
                >
                  Show every requirement in this program
                </Link>
              )}
            {rejected(choice) && (
              <Stack width="100%" gap={1.5} align="start">
                <Text size="sm" weight="medium">
                  Why should it count here? Skyspace can&apos;t verify this, so
                  the requirement stays out of your met count and the reason
                  goes on the PDF for your advisor.
                </Text>
                <RadioList
                  isLabelHidden
                  label={`Basis in ${choice.program.name}`}
                  value={bases[choice.program.id] ?? 'unsure'}
                  onChange={v =>
                    setBases(prev => ({
                      ...prev,
                      [choice.program.id]: v as FillBasis['kind'],
                    }))
                  }
                  size="sm"
                  width="100%"
                >
                  {(Object.keys(FILL_BASIS_LABEL) as FillBasis['kind'][]).map(
                    kind => (
                      <RadioListItem
                        key={kind}
                        value={kind}
                        label={FILL_BASIS_LABEL[kind]}
                      />
                    ),
                  )}
                </RadioList>
                <TextInput
                  label="Note"
                  isOptional
                  size="sm"
                  value={notes[choice.program.id] ?? ''}
                  onChange={v =>
                    setNotes(prev => ({...prev, [choice.program.id]: v}))
                  }
                  placeholder="Who approved it, when, or where it is posted"
                  width="100%"
                />
              </Stack>
            )}
          </Stack>
        ))}

        <Stack
          width="100%"
          padding={2}
          gap={2}
          align="start"
          style={rowDivider}
        >
          <Text type="supporting">
            A course fills at most one requirement per program. Your choice is
            kept even if the course stops matching; it never edits a
            requirement.
          </Text>
          <Stack direction="horizontal" width="100%" gap={1.5} hAlign="end">
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
              isDisabled={equivalentBad}
              onClick={save}
            />
          </Stack>
        </Stack>
      </Stack>
    </Dialog>
  );
}
