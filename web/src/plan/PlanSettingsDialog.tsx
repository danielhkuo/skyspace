import {AlertDialog} from '@astryxdesign/core/AlertDialog';
import {Button} from '@astryxdesign/core/Button';
import {Dialog, DialogHeader} from '@astryxdesign/core/Dialog';
import {Icon} from '@astryxdesign/core/Icon';
import {IconButton} from '@astryxdesign/core/IconButton';
import {Layout, LayoutContent, LayoutFooter} from '@astryxdesign/core/Layout';
import {
  SegmentedControl,
  SegmentedControlItem,
} from '@astryxdesign/core/SegmentedControl';
import {Selector} from '@astryxdesign/core/Selector';
import {Stack, StackItem} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {TextInput} from '@astryxdesign/core/TextInput';
import {useMemo, useState} from 'react';
import {useNavigate} from 'react-router';

import {
  compareTermPosition,
  formatCatalogYear,
  nextTermPosition,
  shortTermLabel,
  termKindName,
  walkRequirements,
  type CatalogYear,
  type PlanBundle,
  type PlanTerm,
  type Program,
  type ProgramId,
  type RequirementId,
  type TermKindName,
  type TermPosition,
} from '../domain';
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

const LABEL_PLACEHOLDER: Record<TermKindName, string> = {
  rice: 'Shown under the term name',
  away: 'Study abroad, exchange, visiting',
  off: 'Gap semester, co-op, leave',
};

/** "5 courses" or "empty", for a Rice term with no label. */
function termCount(term: PlanTerm): string {
  const n =
    typeof term.kind === 'object' && 'rice' in term.kind
      ? term.kind.rice.courses.length
      : typeof term.kind === 'object' && 'away' in term.kind
        ? term.kind.away.cards.length
        : 0;
  return n === 0 ? 'empty' : `${n} course${n === 1 ? '' : 's'}`;
}

/** The label sits in a term header; past this it wraps. Enforced in the reducer too. */
export const TERM_LABEL_MAX = 40;

const encode = (p: TermPosition): string => `${p.academicYear}-${p.season}`;
const decode = (v: string): TermPosition => {
  const [year, season] = v.split('-');
  return {
    academicYear: Number(year),
    season:
      season === 'spring' ? 'spring' : season === 'summer' ? 'summer' : 'fall',
  };
};

/**
 * Plan settings (`Plan 7`): name, programs, catalog year, the terms list.
 * Every change applies as it is made, and the body says so; only dropping
 * a program that has pins or self-checks on it asks first, because that
 * clears them.
 */
export function PlanSettingsDialog({
  bundle,
  available,
  dispatch,
  onClose,
}: PlanSettingsDialogProps) {
  const {plan} = bundle;
  const navigate = useNavigate();
  const [name, setName] = useState(plan.name);
  const [refusal, setRefusal] = useState<Record<string, string>>({});
  const [pendingDrop, setPendingDrop] = useState<
    {programs: ProgramId[]; dropped: Program; affected: number} | undefined
  >(undefined);

  const first = plan.terms[0]?.position ?? plan.matriculation;
  const last =
    plan.terms[plan.terms.length - 1]?.position ?? plan.matriculation;
  // Catalog years Rice allows: from matriculation to the last term's year.
  // Spring 2029 is academic year 2029 and the 2028-29 announcements.
  // Rice allows any year from matriculation to graduation (one General
  // Announcements edition per academic year). Skyspace can only offer the
  // years it holds reviewed requirements for: there is no historical data.
  const years = useMemo(() => {
    const held = new Set(available.map(p => p.catalogYear));
    const out: CatalogYear[] = [];
    for (
      let y = plan.matriculation.academicYear - 1;
      y <= last.academicYear - 1;
      y += 1
    ) {
      if (held.has(y)) {
        out.push(y);
      }
    }
    if (!out.includes(plan.catalogYear)) {
      out.push(plan.catalogYear);
      out.sort();
    }
    return out;
  }, [
    available,
    plan.matriculation.academicYear,
    last.academicYear,
    plan.catalogYear,
  ]);

  // Every fall, spring and summer from the first term to a year past the last, minus what exists.
  const openPositions = useMemo(() => {
    const out: TermPosition[] = [];
    let p: TermPosition = {academicYear: first.academicYear, season: 'fall'};
    const end: TermPosition = {
      academicYear: last.academicYear + 1,
      season: 'summer',
    };
    let guard = 0;
    while (compareTermPosition(p, end) <= 0 && guard < 60) {
      if (!plan.terms.some(t => compareTermPosition(t.position, p) === 0)) {
        out.push(p);
      }
      p = nextTermPosition(p);
      guard += 1;
    }
    return out;
  }, [plan.terms, first.academicYear, last.academicYear]);

  const university = available
    .filter(p => p.kind === 'university')
    .map(p => p.id);
  const kindOf = (id: ProgramId): Program['kind'] | undefined =>
    available.find(p => p.id === id)?.kind;
  const majors = plan.programs.filter(id => kindOf(id) === 'major');
  const minors = plan.programs.filter(id => {
    const kind = kindOf(id);
    return (
      kind === 'minor' || kind === 'certificate' || kind === 'concentration'
    );
  });

  /** Pins, claims and self-checks that point at a program's requirements. */
  const affectedBy = (program: Program): number => {
    const ids = new Set<RequirementId>();
    walkRequirements(program.root, r => ids.add(r.id));
    let n = plan.selfChecks.filter(s => ids.has(s.requirement)).length;
    const count = (fills: RequirementId[]): void => {
      n += fills.filter(f => ids.has(f)).length;
    };
    plan.incomingCredit.forEach(c => count(c.fills));
    for (const term of plan.terms) {
      if (typeof term.kind === 'object' && 'rice' in term.kind) {
        term.kind.rice.courses.forEach(c => count(c.fills));
      } else if (typeof term.kind === 'object' && 'away' in term.kind) {
        term.kind.away.cards.forEach(c => count(c.fills));
      }
    }
    return n;
  };

  const setPrograms = (
    nextMajors: ProgramId[],
    nextMinors: ProgramId[],
  ): void => {
    const programs = [...university, ...nextMajors, ...nextMinors];
    const droppedId = plan.programs.find(id => !programs.includes(id));
    const dropped =
      droppedId === undefined
        ? undefined
        : available.find(p => p.id === droppedId);
    const affected = dropped === undefined ? 0 : affectedBy(dropped);
    if (dropped !== undefined && affected > 0) {
      setPendingDrop({programs, dropped, affected});
      return;
    }
    dispatch({type: 'setPrograms', programs, available});
  };

  const changeKind = (term: PlanTerm, kind: TermKindName): void => {
    const reason = termKindChangeBlocker(term, kind);
    setRefusal(prev => ({...prev, [term.id]: reason ?? ''}));
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

  const close = (): void => {
    if (name.trim() !== '' && name.trim() !== plan.name) {
      dispatch({type: 'renamePlan', name});
    }
    onClose();
  };

  return (
    <>
      <Dialog
        isOpen
        onOpenChange={open => {
          if (!open) {
            close();
          }
        }}
        purpose="form"
        width={480}
        maxHeight="85dvh"
      >
        <Layout
          header={
            <DialogHeader
              title="Plan settings"
              subtitle="Changes save as you make them."
              onOpenChange={open => {
                if (!open) {
                  close();
                }
              }}
            />
          }
          content={
            <LayoutContent>
              <Stack width="100%" gap={3} align="start">
                <Stack width="100%" gap={2} align="start">
                  <TextInput
                    label="Plan name"
                    size="sm"
                    value={name}
                    onChange={v => {
                      setName(v);
                      if (v.trim() !== '') {
                        dispatch({type: 'renamePlan', name: v});
                      }
                    }}
                    width="100%"
                    status={
                      name.trim() === ''
                        ? {type: 'error', message: 'A plan needs a name'}
                        : undefined
                    }
                  />
                  <ProgramPicker
                    label="Majors"
                    placeholder="Search programs"
                    kinds={['major']}
                    available={available}
                    chosen={majors}
                    onChange={next => setPrograms(next, minors)}
                    minimum={1}
                    minimumReason="A plan always has at least one major. Add another before removing this one."
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
                    description={`Requirements follow this year's General Announcements. You can pick any year from matriculation to graduation; Skyspace holds reviewed requirements for ${years.length === 1 ? 'one year' : `${years.length} years`} so far.`}
                    size="sm"
                    value={String(plan.catalogYear)}
                    options={years.map(y => ({
                      value: String(y),
                      label: formatCatalogYear(y),
                    }))}
                    onChange={v =>
                      dispatch({type: 'setCatalogYear', year: Number(v)})
                    }
                    width="100%"
                  />
                </Stack>

                <Stack width="100%" gap={1.5} align="start">
                  <Text type="label" weight="semibold">
                    Terms
                  </Text>
                  {plan.terms.map(term => {
                    const removeReason = termRemoveBlocker(term);
                    const kind = termKindName(term.kind);
                    const termLabel = shortTermLabel(term.position);
                    const wantsLabel =
                      kind !== 'rice' || (term.label ?? '') !== '';
                    return (
                      <Stack key={term.id} width="100%" gap={0.5} align="start">
                        <Stack
                          direction="horizontal"
                          width="100%"
                          gap={1.5}
                          vAlign="center"
                        >
                          <Stack width={88}>
                            <Text size="sm" textWrap="nowrap">
                              {termLabel}
                            </Text>
                          </Stack>
                          <SegmentedControl
                            label={`Kind of ${termLabel}`}
                            value={kind}
                            onChange={v => changeKind(term, v as TermKindName)}
                            size="sm"
                          >
                            <SegmentedControlItem
                              value="rice"
                              label="At Rice"
                            />
                            <SegmentedControlItem value="away" label="Away" />
                            <SegmentedControlItem value="off" label="Off" />
                          </SegmentedControl>
                          <StackItem size="fill">
                            {wantsLabel ? (
                              <TextInput
                                label={`Label for ${termLabel}`}
                                isLabelHidden
                                size="sm"
                                value={term.label ?? ''}
                                onChange={label =>
                                  dispatch({
                                    type: 'setTerm',
                                    term: term.id,
                                    kind,
                                    label: label.slice(0, TERM_LABEL_MAX),
                                    facts: bundle.facts,
                                  })
                                }
                                placeholder={LABEL_PLACEHOLDER[kind]}
                                width="100%"
                              />
                            ) : (
                              <Text type="supporting" textWrap="nowrap">
                                {termCount(term)}
                              </Text>
                            )}
                          </StackItem>
                          <IconButton
                            label={`Remove ${termLabel}`}
                            tooltip={
                              removeReason === undefined
                                ? 'Remove this term'
                                : 'Holds courses; move them first'
                            }
                            variant="ghost"
                            size="sm"
                            icon={<Icon icon="close" size="sm" />}
                            isDisabled={removeReason !== undefined}
                            onClick={() =>
                              dispatch({type: 'removeTerm', term: term.id})
                            }
                          />
                        </Stack>
                        {(refusal[term.id] ?? '') !== '' && (
                          <Text
                            size="sm"
                            role="status"
                            aria-live="polite"
                            style={{color: 'var(--color-text-red)'}}
                          >
                            {refusal[term.id]}
                          </Text>
                        )}
                      </Stack>
                    );
                  })}
                  <Selector
                    label="Add a term"
                    description="Summers included. Pick a position that is not on the board yet."
                    size="sm"
                    width="100%"
                    placeholder="Fall, spring or summer…"
                    hasClear
                    value={null}
                    options={openPositions.map(p => ({
                      value: encode(p),
                      label: shortTermLabel(p),
                    }))}
                    onChange={v => {
                      if (v !== null) {
                        dispatch({type: 'addTermAt', position: decode(v)});
                      }
                    }}
                  />
                </Stack>

                <Stack width="100%" gap={1} align="start">
                  <Button
                    label="Start over with a new plan…"
                    variant="secondary"
                    size="sm"
                    onClick={() => {
                      close();
                      void navigate('/plan/new');
                    }}
                  />
                  <Text type="supporting">
                    Programs, timeline and incoming credit from scratch. The
                    demo holds one plan, so it replaces this one; duplicating
                    and deleting plans arrive with accounts.
                  </Text>
                </Stack>
              </Stack>
            </LayoutContent>
          }
          footer={
            <LayoutFooter>
              <Stack direction="horizontal" width="100%" hAlign="end">
                <Button
                  label="Done"
                  variant="primary"
                  size="sm"
                  onClick={close}
                />
              </Stack>
            </LayoutFooter>
          }
        />
      </Dialog>
      <AlertDialog
        isOpen={pendingDrop !== undefined}
        onOpenChange={open => {
          if (!open) {
            setPendingDrop(undefined);
          }
        }}
        title={`Drop ${pendingDrop?.dropped.name ?? 'this program'}?`}
        description={`${pendingDrop?.affected ?? 0} course pin${pendingDrop?.affected === 1 ? '' : 's'} or self-check${pendingDrop?.affected === 1 ? '' : 's'} on its requirements will be cleared. Adding the program back later does not restore them.`}
        cancelLabel="Keep it"
        actionLabel="Drop program"
        actionVariant="destructive"
        onAction={() => {
          if (pendingDrop !== undefined) {
            dispatch({
              type: 'setPrograms',
              programs: pendingDrop.programs,
              available,
            });
          }
          setPendingDrop(undefined);
        }}
      />
    </>
  );
}
