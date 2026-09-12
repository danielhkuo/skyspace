import {Button} from '@astryxdesign/core/Button';
import {Dialog} from '@astryxdesign/core/Dialog';
import {Link} from '@astryxdesign/core/Link';
import {RadioList, RadioListItem} from '@astryxdesign/core/RadioList';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {TextArea} from '@astryxdesign/core/TextArea';
import {useState} from 'react';

import {
  findRule,
  type Program,
  type RuleReport,
  type SelfCheck,
  type SelfCheckReason,
} from '../domain';
import {SELF_CHECK_REASON_LABEL} from './labels';
import {islandHead, rowDivider, violetInk} from './paint';
import {ReportRuleDialog} from './ReportRuleDialog';
import type {PlanAction} from './usePlan';

type SelfCheckDialogProps = {
  program: Program;
  rule: RuleReport;
  existing: SelfCheck | undefined;
  dispatch: (action: PlanAction) => void;
  onClose: () => void;
};

const REASONS: SelfCheckReason[] = [
  'transfer',
  'apOrIb',
  'studyAbroad',
  'advisorApproved',
  'other',
];

/**
 * "Why are you marking this as satisfied?" (`Plan 6 Rule Choice`). A
 * self-check is the student's word with a reason attached; it is listed on
 * the PDF and never counted in a total.
 */
export function SelfCheckDialog({
  program,
  rule,
  existing,
  dispatch,
  onClose,
}: SelfCheckDialogProps) {
  const [reason, setReason] = useState<SelfCheckReason>(
    existing?.reason ?? 'advisorApproved',
  );
  const [note, setNote] = useState(existing?.note ?? '');
  const [reporting, setReporting] = useState(false);
  const source = findRule(program, rule.rule);
  const text =
    source?.body.kind === 'unverifiable'
      ? source.body.text
      : source?.body.kind === 'nonCourse'
        ? source.body.description
        : rule.label;

  return (
    <>
      <Dialog
        isOpen
        onOpenChange={open => {
          if (!open) {
            onClose();
          }
        }}
        width={520}
        padding={0}
      >
        <Stack width="100%" gap={0}>
          <Stack
            width="100%"
            padding={2}
            gap={1}
            align="start"
            style={islandHead}
          >
            <Stack
              direction="horizontal"
              width="100%"
              hAlign="between"
              vAlign="start"
              gap={2}
            >
              <Text as="h3" weight="semibold">
                {rule.label}
              </Text>
              <Link
                href="#"
                size="sm"
                onClick={e => {
                  e.preventDefault();
                  setReporting(true);
                }}
              >
                Report this rule
              </Link>
            </Stack>
            <Text size="sm" color="secondary">
              {text} Skyspace cannot verify this from Rice&apos;s data, so it is
              yours to confirm.
            </Text>
            <Link href={rule.source.url} size="sm" isExternalLink>
              General Announcements · source
            </Link>
          </Stack>
          <Stack
            width="100%"
            padding={2}
            gap={1.5}
            align="start"
            style={rowDivider}
          >
            <Text type="label" weight="semibold">
              Why are you marking this as satisfied?
            </Text>
            <RadioList
              isLabelHidden
              label="Reason"
              value={reason}
              onChange={v => setReason(v as SelfCheckReason)}
              size="sm"
              width="100%"
            >
              {REASONS.map(r => (
                <RadioListItem
                  key={r}
                  value={r}
                  label={SELF_CHECK_REASON_LABEL[r]}
                />
              ))}
            </RadioList>
            <TextArea
              label="Note"
              isOptional
              size="sm"
              value={note}
              onChange={setNote}
              placeholder="Shown on the PDF your advisor sees"
              width="100%"
              rows={2}
            />
          </Stack>
          <Stack
            width="100%"
            padding={2}
            gap={2}
            align="start"
            style={rowDivider}
          >
            <Text type="supporting" style={violetInk}>
              Self-checks are listed on the PDF and never counted in progress
              totals. Confirm with your advisor or the degree audit in Esther.
            </Text>
            <Stack direction="horizontal" width="100%" gap={1.5} hAlign="end">
              {existing !== undefined && (
                <Button
                  label="Clear"
                  variant="ghost"
                  size="sm"
                  onClick={() => {
                    dispatch({type: 'clearSelfCheck', rule: rule.rule});
                    onClose();
                  }}
                />
              )}
              <Button
                label="Cancel"
                variant="ghost"
                size="sm"
                onClick={onClose}
              />
              <Button
                label={existing === undefined ? 'Mark satisfied' : 'Save'}
                variant="primary"
                size="sm"
                onClick={() => {
                  dispatch({
                    type: 'confirmSelfCheck',
                    rule: rule.rule,
                    reason,
                    note: note.trim() === '' ? undefined : note.trim(),
                  });
                  onClose();
                }}
              />
            </Stack>
          </Stack>
        </Stack>
      </Dialog>
      {reporting && (
        <ReportRuleDialog
          program={program}
          rule={rule}
          onClose={() => setReporting(false)}
        />
      )}
    </>
  );
}
