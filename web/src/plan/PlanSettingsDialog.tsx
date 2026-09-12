import {Button} from '@astryxdesign/core/Button';
import {Dialog} from '@astryxdesign/core/Dialog';
import {Icon} from '@astryxdesign/core/Icon';
import {IconButton} from '@astryxdesign/core/IconButton';
import {
  SegmentedControl,
  SegmentedControlItem,
} from '@astryxdesign/core/SegmentedControl';
import {Selector} from '@astryxdesign/core/Selector';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {TextInput} from '@astryxdesign/core/TextInput';
import {useState} from 'react';
import {useNavigate} from 'react-router';

import {
  formatCatalogYear,
  isOffTerm,
  shortTermLabel,
  termKindName,
  type CatalogYear,
  type PlanBundle,
  type PlanTerm,
  type Program,
  type ProgramId,
  type TermKindName,
} from '../domain';
import {islandHead, rowDivider} from './paint';
import {ProgramPicker} from './ProgramPicker';
import {
  termKindChangeBlocker,
  termRemoveBlocker,
  type PlanAction,
} from './usePlan';

type PlanSettingsDialogProps = {
  bundle: PlanBundle;
  /** Every program the pickers may offer. */
  available: Program[];
  dispatch: (action: PlanAction) => void;
  onClose: () => void;
};

/** Plan settings: name, programs, catalog year, and the terms list (`Plan 7`). Every change applies at once. */
export function PlanSettingsDialog({
  bundle,
  available,
  dispatch,
  onClose,
}: PlanSettingsDialogProps) {
  const {plan} = bundle;
  const navigate = useNavigate();
  const [name, setName] = useState(plan.name);
  const [blocked, setBlocked] = useState<Record<string, string>>({});
  const lastYear =
    plan.terms[plan.terms.length - 1]?.position.academicYear ??
    plan.matriculation.academicYear;
  const years: CatalogYear[] = [];
  for (let y = plan.matriculation.academicYear - 1; y <= lastYear; y += 1) {
    years.push(y);
  }
  const university = available
    .filter(p => p.kind === 'university')
    .map(p => p.id);
  const majors = plan.programs.filter(
    id => available.find(p => p.id === id)?.kind === 'major',
  );
  const minors = plan.programs.filter(id => {
    const kind = available.find(p => p.id === id)?.kind;
    return (
      kind === 'minor' || kind === 'certificate' || kind === 'concentration'
    );
  });
  const setPrograms = (
    nextMajors: ProgramId[],
    nextMinors: ProgramId[],
  ): void =>
    dispatch({
      type: 'setPrograms',
      programs: [...university, ...nextMajors, ...nextMinors],
      available,
    });

  const changeKind = (term: PlanTerm, kind: TermKindName): void => {
    const reason = termKindChangeBlocker(term, kind);
    setBlocked(prev => ({...prev, [term.id]: reason ?? ''}));
    if (reason === undefined) {
      dispatch({
        type: 'setTerm',
        term: term.id,
        kind,
        label: term.label,
        facts: bundle.facts,
      });
    }
  };

  return (
    <Dialog
      isOpen
      onOpenChange={open => {
        if (!open) {
          onClose();
        }
      }}
      width={440}
      padding={0}
      maxHeight="90vh"
    >
      <Stack width="100%" gap={0}>
        <Stack
          direction="horizontal"
          width="100%"
          padding={2}
          hAlign="between"
          vAlign="center"
          style={islandHead}
        >
          <Text as="h3" size="sm" weight="semibold">
            Plan settings
          </Text>
          <IconButton
            label="Close"
            variant="ghost"
            size="sm"
            icon={<Icon icon="close" size="sm" />}
            onClick={onClose}
          />
        </Stack>

        <Stack width="100%" padding={2} gap={2} align="start">
          <TextInput
            label="Plan name"
            size="sm"
            value={name}
            onChange={setName}
            onBlur={() => dispatch({type: 'renamePlan', name})}
            onEnter={() => dispatch({type: 'renamePlan', name})}
            width="100%"
          />
          <ProgramPicker
            label="Majors"
            placeholder="Search programs"
            kinds={['major']}
            available={available}
            chosen={majors}
            onChange={next => setPrograms(next, minors)}
          />
          <ProgramPicker
            label="Minors"
            isOptional
            placeholder="Search minors"
            kinds={['minor', 'certificate', 'concentration']}
            available={available}
            chosen={minors}
            onChange={next => setPrograms(majors, next)}
          />
          <Selector
            label="Catalog year"
            description="Rice lets you follow any year between when you matriculated and when you graduate"
            size="sm"
            value={String(plan.catalogYear)}
            options={years.map(y => ({
              value: String(y),
              label: formatCatalogYear(y),
            }))}
            onChange={v => dispatch({type: 'setCatalogYear', year: Number(v)})}
            width="100%"
          />
        </Stack>

        <Stack
          width="100%"
          padding={2}
          gap={1.5}
          align="start"
          style={rowDivider}
        >
          <Text type="label" weight="semibold">
            Terms
          </Text>
          {plan.terms.map(term => {
            const removeReason = termRemoveBlocker(term);
            const kind = termKindName(term.kind);
            return (
              <Stack key={term.id} width="100%" gap={1} align="start">
                <Stack
                  direction="horizontal"
                  width="100%"
                  gap={1}
                  vAlign="center"
                  wrap="wrap"
                >
                  <Stack width={96}>
                    <Text size="sm" textWrap="nowrap">
                      {shortTermLabel(term.position)}
                    </Text>
                  </Stack>
                  <SegmentedControl
                    label={`Kind of ${shortTermLabel(term.position)}`}
                    value={kind}
                    onChange={v => changeKind(term, v as TermKindName)}
                    size="sm"
                  >
                    <SegmentedControlItem value="rice" label="Rice" />
                    <SegmentedControlItem value="away" label="Away" />
                    <SegmentedControlItem value="off" label="Off" />
                  </SegmentedControl>
                  <IconButton
                    label={`Remove ${shortTermLabel(term.position)}`}
                    tooltip={removeReason}
                    variant="ghost"
                    size="sm"
                    icon={<Icon icon="close" size="sm" />}
                    isDisabled={removeReason !== undefined}
                    onClick={() =>
                      dispatch({type: 'removeTerm', term: term.id})
                    }
                  />
                </Stack>
                {(blocked[term.id] ?? '') !== '' && (
                  <Text type="supporting">{blocked[term.id]}</Text>
                )}
                {!isOffTerm(term.kind) && kind !== 'rice' && (
                  <TextInput
                    label="Label"
                    isLabelHidden
                    size="sm"
                    value={term.label ?? ''}
                    onChange={label =>
                      dispatch({
                        type: 'setTerm',
                        term: term.id,
                        kind,
                        label,
                        facts: bundle.facts,
                      })
                    }
                    placeholder="Study abroad — Madrid"
                    width="100%"
                  />
                )}
                {isOffTerm(term.kind) && (
                  <TextInput
                    label="Label"
                    isLabelHidden
                    size="sm"
                    value={term.label ?? ''}
                    onChange={label =>
                      dispatch({
                        type: 'setTerm',
                        term: term.id,
                        kind,
                        label,
                        facts: bundle.facts,
                      })
                    }
                    placeholder="Gap semester, co-op, leave"
                    width="100%"
                  />
                )}
              </Stack>
            );
          })}
          <Button
            label="Add term"
            variant="ghost"
            size="sm"
            onClick={() => {
              const last = plan.terms[plan.terms.length - 1];
              if (last !== undefined) {
                dispatch({type: 'addTermAfter', after: last.id});
              }
            }}
          />
        </Stack>

        <Stack
          width="100%"
          padding={2}
          gap={1.5}
          align="start"
          style={rowDivider}
        >
          <Button
            label="Start a new plan"
            variant="secondary"
            size="sm"
            onClick={() => {
              onClose();
              void navigate('/plan/new');
            }}
          />
          <Button
            label="Duplicate this plan"
            variant="secondary"
            size="sm"
            isDisabled
            tooltip="The demo holds one plan at a time"
          />
          <Button
            label="Delete this plan"
            variant="destructive"
            size="sm"
            isDisabled
            tooltip="You can't delete your only plan"
          />
          <Text type="supporting">You can&apos;t delete your only plan.</Text>
        </Stack>
      </Stack>
    </Dialog>
  );
}
