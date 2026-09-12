import {Button} from '@astryxdesign/core/Button';
import {Dialog} from '@astryxdesign/core/Dialog';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {TextArea} from '@astryxdesign/core/TextArea';
import {TextInput} from '@astryxdesign/core/TextInput';
import {useState} from 'react';

import {dataSource} from '../datasource';
import type {Program, RuleReport} from '../domain';
import {islandHead, rowDivider} from './paint';

type ReportRuleDialogProps = {
  program: Program;
  rule: RuleReport;
  onClose: () => void;
};

/** "Report this rule": a person reviews every report. The demo keeps it in this browser. */
export function ReportRuleDialog({
  program,
  rule,
  onClose,
}: ReportRuleDialogProps) {
  const [what, setWhat] = useState('');
  const [email, setEmail] = useState('');
  const [sent, setSent] = useState(false);

  const send = (): void => {
    void dataSource
      .reportRule({
        program: program.id,
        rule: rule.rule,
        label: rule.label,
        sourceUrl: rule.source.url,
        text: what.trim(),
        email: email.trim() === '' ? undefined : email.trim(),
      })
      .then(() => setSent(true));
  };

  return (
    <Dialog
      isOpen
      onOpenChange={open => {
        if (!open) {
          onClose();
        }
      }}
      width={360}
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
          <Text as="h3" size="sm" weight="semibold">
            Report this rule
          </Text>
          <Text type="supporting">A person reviews every report.</Text>
        </Stack>
        {sent ? (
          <Stack
            width="100%"
            padding={2}
            gap={1.5}
            align="start"
            style={rowDivider}
          >
            <Text size="sm">
              Thanks.{' '}
              {dataSource.kind === 'demo'
                ? 'In this demo the report stays in your browser; nothing is sent.'
                : 'We will look at it.'}
            </Text>
            <Button
              label="Done"
              variant="primary"
              size="sm"
              onClick={onClose}
            />
          </Stack>
        ) : (
          <>
            <Stack
              width="100%"
              padding={2}
              gap={1.5}
              align="start"
              style={rowDivider}
            >
              <TextArea
                label="What's wrong"
                size="sm"
                value={what}
                onChange={setWhat}
                placeholder="The GA lists four courses, not three."
                width="100%"
                rows={2}
              />
              <TextInput
                label="Email"
                isOptional
                type="email"
                size="sm"
                value={email}
                onChange={setEmail}
                placeholder="netid@rice.edu"
                width="100%"
              />
            </Stack>
            <Stack
              direction="horizontal"
              width="100%"
              padding={2}
              gap={1.5}
              hAlign="end"
              style={rowDivider}
            >
              <Button
                label="Cancel"
                variant="ghost"
                size="sm"
                onClick={onClose}
              />
              <Button
                label="Send report"
                variant="primary"
                size="sm"
                isDisabled={what.trim() === ''}
                onClick={send}
              />
            </Stack>
          </>
        )}
      </Stack>
    </Dialog>
  );
}
