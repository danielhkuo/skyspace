import {Button} from '@astryxdesign/core/Button';
import {Dialog} from '@astryxdesign/core/Dialog';
import {RadioList, RadioListItem} from '@astryxdesign/core/RadioList';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {TextInput} from '@astryxdesign/core/TextInput';
import {useState} from 'react';

import {
  termKindName,
  termLabel,
  type PlanTerm,
  type TermKindName,
} from '../domain';
import {termKindChangeBlocker} from './usePlan';

type EditTermDialogProps = {
  term: PlanTerm | undefined;
  onClose: () => void;
  onSave: (kind: TermKindName, label: string) => void;
};

const KINDS: {value: TermKindName; label: string; description: string}[] = [
  {
    value: 'rice',
    label: 'At Rice',
    description:
      'Courses from the catalog. Prerequisites and requirements are checked.',
  },
  {
    value: 'away',
    label: 'Away',
    description:
      'Study abroad, transfer, visiting another school. Courses are cards you type in and assign to requirements yourself.',
  },
  {
    value: 'off',
    label: 'Off',
    description:
      'Gap semester, gap year, leave, co-op. No courses and no credit expected; skipped in pacing.',
  },
];

/** Kind and label for one term. A change that would lose cards is refused with the reason shown. */
export function EditTermDialog({term, onClose, onSave}: EditTermDialogProps) {
  return term === undefined ? null : (
    <EditTermForm key={term.id} term={term} onClose={onClose} onSave={onSave} />
  );
}

function EditTermForm({
  term,
  onClose,
  onSave,
}: EditTermDialogProps & {term: PlanTerm}) {
  const [kind, setKind] = useState<TermKindName>(termKindName(term.kind));
  const [label, setLabel] = useState(term.label ?? '');
  const blocker = termKindChangeBlocker(term, kind);

  return (
    <Dialog
      isOpen
      onOpenChange={open => {
        if (!open) {
          onClose();
        }
      }}
      width={460}
      padding={3}
    >
      <Stack gap={3} width="100%">
        <Text as="p" size="lg" weight="semibold">
          {termLabel(term.position)}
        </Text>
        <RadioList
          label="What kind of term is this?"
          value={kind}
          onChange={v => setKind(v as TermKindName)}
          size="sm"
          width="100%"
        >
          {KINDS.map(k => (
            <RadioListItem
              key={k.value}
              value={k.value}
              label={k.label}
              description={k.description}
            />
          ))}
        </RadioList>
        <TextInput
          label="Label"
          isOptional
          value={label}
          onChange={setLabel}
          placeholder={
            kind === 'away'
              ? 'Study abroad — Madrid'
              : kind === 'off'
                ? 'Co-op — Chevron'
                : 'Shown under the term name'
          }
          width="100%"
          onEnter={() => {
            if (blocker === undefined) {
              onSave(kind, label);
            }
          }}
        />
        {blocker !== undefined && (
          <Text size="sm" color="secondary">
            {blocker}
          </Text>
        )}
        <Stack direction="horizontal" gap={1} hAlign="end" width="100%">
          <Button label="Cancel" variant="ghost" size="sm" onClick={onClose} />
          <Button
            label="Save"
            variant="primary"
            size="sm"
            isDisabled={blocker !== undefined}
            onClick={() => onSave(kind, label)}
          />
        </Stack>
      </Stack>
    </Dialog>
  );
}
