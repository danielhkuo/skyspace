import {Button} from '@astryxdesign/core/Button';
import {Link} from '@astryxdesign/core/Link';
import {Dialog} from '@astryxdesign/core/Dialog';
import {RadioList, RadioListItem} from '@astryxdesign/core/RadioList';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {useMemo, useState} from 'react';

import {TextInput} from '@astryxdesign/core/TextInput';

import {
  walkRules,
  type CourseCode,
  type EntryId,
  type FillBasis,
  type FillClaim,
  type PlanBundle,
  type Program,
  type RuleId,
} from '../domain';
import {engine} from '../engine';
import {FILL_BASIS_LABEL, rulePaths} from './labels';
import {islandHead, rowDivider} from './paint';
import type {PlanAction} from './usePlan';

type RuleChoiceDialogProps = {
  entry: EntryId;
  /** Absent for a manual card with no Rice equivalent: nothing matches, every pin is a claim. */
  course: CourseCode | undefined;
  /** What the card is called on the board. */
  label: string;
  /** The card's current pins, one per program at most. */
  fills: RuleId[];
  claims: FillClaim[];
  bundle: PlanBundle;
  dispatch: (action: PlanAction) => void;
  onClose: () => void;
};

const AUTO = 'auto';

type Choice = {
  program: Program;
  /** Every course rule; `matches` says whether the filter accepts the card. */
  options: {rule: RuleId; label: string; matches: boolean}[];
  current: string;
};

/**
 * "Which rule does STAT 310 fill?" (`Plan 6 Rule Choice`). One radio list
 * per program: the rules the course matches, plus its current pin even if
 * that no longer matches, plus "let Skyspace decide". The choice is kept on
 * the card, never edits a rule, and survives the course ceasing to match:
 * the engine then warns "doesn't match this rule · your choice" instead of
 * silently dropping it, which is the honest answer for a course that
 * counted under an older catalog.
 */
export function RuleChoiceDialog({
  entry,
  course,
  label,
  fills,
  claims,
  bundle,
  dispatch,
  onClose,
}: RuleChoiceDialogProps) {
  const choices = useMemo<Choice[]>(
    () =>
      bundle.plan.programs.flatMap(id => {
        const program = bundle.programs.find(p => p.id === id);
        if (program === undefined) {
          return [];
        }
        const paths = rulePaths(program);
        const pinned = fills.find(r => paths.has(r));
        // Sibling slots share a label (three "Distribution Group III" rules);
        // one option per label, the first rule id standing for the run.
        const seen = new Set<string>();
        const options: Choice['options'] = [];
        walkRules(program.root, rule => {
          if (rule.body.kind !== 'course') {
            return;
          }
          const path = paths.get(rule.id) ?? rule.label;
          const matches =
            course !== undefined &&
            engine.ruleMatches(program, rule.id, [course], bundle.facts)
              .length > 0;
          if (seen.has(path) && rule.id !== pinned) {
            return;
          }
          seen.add(path);
          options.push({rule: rule.id, label: path, matches});
        });
        return [{program, options, current: pinned ?? AUTO}];
      }),
    [bundle, course, fills],
  );
  const [picked, setPicked] = useState<Record<string, string>>(() =>
    Object.fromEntries(choices.map(c => [c.program.id, c.current])),
  );
  // One basis per program, seeded from the existing claim on the current pin.
  const [bases, setBases] = useState<Record<string, FillBasis['kind']>>(() =>
    Object.fromEntries(
      choices.map(c => [
        c.program.id,
        claims.find(k => k.rule === c.current)?.basis.kind ?? 'unsure',
      ]),
    ),
  );
  const [notes, setNotes] = useState<Record<string, string>>(() =>
    Object.fromEntries(
      choices.map(c => [
        c.program.id,
        claims.find(k => k.rule === c.current)?.note ?? '',
      ]),
    ),
  );
  // Matching rules and the current pin show by default; the rest are one click away.
  const [showAll, setShowAll] = useState<Record<string, boolean>>({});
  const visible = (choice: Choice): Choice['options'] =>
    showAll[choice.program.id]
      ? choice.options
      : choice.options.filter(o => o.matches || o.rule === choice.current);
  const rejected = (choice: Choice): boolean => {
    const rule = picked[choice.program.id];
    const option = choice.options.find(o => o.rule === rule);
    return option !== undefined && !option.matches;
  };

  const save = (): void => {
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
            rule: next as RuleId,
          });
        }
      }
      if (next !== AUTO && rejected(choice)) {
        const note = (notes[choice.program.id] ?? '').trim();
        dispatch({
          type: 'setFillClaim',
          entry,
          claim: {
            rule: next as RuleId,
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
      width={440}
      padding={0}
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
            Which rule does {label} fill?
          </Text>
          <Text type="supporting">
            A course fills at most one rule per program.
          </Text>
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
              {choice.program.name}
            </Text>
            {visible(choice).length === 0 && (
              <Text type="supporting">
                No rule in this program matches it as published.
              </Text>
            )}
            {visible(choice).length > 0 && (
              <RadioList
                isLabelHidden
                label={`Rule in ${choice.program.name}`}
                value={picked[choice.program.id] ?? AUTO}
                onChange={v =>
                  setPicked(prev => ({...prev, [choice.program.id]: v}))
                }
                size="sm"
                width="100%"
              >
                {visible(choice).map(o => (
                  <RadioListItem
                    key={o.rule}
                    value={o.rule}
                    label={o.label}
                    description={
                      o.matches
                        ? undefined
                        : "Doesn't match as published; counts on your say-so and is flagged"
                    }
                  />
                ))}
                <RadioListItem
                  value={AUTO}
                  label="Let Skyspace decide"
                  description="Whichever open rule it fits, recomputed on every change"
                />
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
                  Show every rule in this program
                </Link>
              )}
            {rejected(choice) && (
              <Stack width="100%" gap={1.5} align="start">
                <Text size="sm" weight="medium">
                  Why should it count here? Skyspace can&apos;t verify this, so
                  the rule stays out of your met count and the reason goes on
                  the PDF for your advisor.
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
            Your choice is kept even if the course stops matching. It never
            edits the rule.
          </Text>
          <Stack direction="horizontal" width="100%" gap={1.5} hAlign="end">
            <Button
              label="Cancel"
              variant="ghost"
              size="sm"
              onClick={onClose}
            />
            <Button label="Save" variant="primary" size="sm" onClick={save} />
          </Stack>
        </Stack>
      </Stack>
    </Dialog>
  );
}
