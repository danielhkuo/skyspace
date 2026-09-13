import {Button} from '@astryxdesign/core/Button';
import {Dialog} from '@astryxdesign/core/Dialog';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {TextArea} from '@astryxdesign/core/TextArea';
import {useState} from 'react';

import {dataSource} from '../datasource';
import {RateLimitedError} from '../datasource/types';
import type {CatalogYear, Program, RequirementReport} from '../domain';
import {islandHead, rowDivider} from './paint';

type ReportRequirementDialogProps = {
  program: Program;
  requirement: RequirementReport;
  /** The plan's year: which edition of the requirement is being reported. */
  catalogYear: CatalogYear;
  onClose: () => void;
};

/**
 * "Report this requirement": a person reviews every report. The wire body
 * takes free text only, so the requirement's label and source URL are folded
 * into the message. No contact is collected: the store keeps none on
 * purpose. The demo keeps the report in this browser.
 */
export function ReportRequirementDialog({
  program,
  requirement,
  catalogYear,
  onClose,
}: ReportRequirementDialogProps) {
  const [what, setWhat] = useState('');
  const [sent, setSent] = useState(false);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);

  const send = (): void => {
    setSending(true);
    setError(undefined);
    void dataSource
      .reportRequirement({
        program: program.id,
        requirement: requirement.requirement,
        catalogYear,
        message: `${requirement.label} (${requirement.source.url}): ${what.trim()}`,
      })
      .then(() => setSent(true))
      .catch((e: unknown) => {
        setError(
          e instanceof RateLimitedError
            ? e.retryAfterSeconds === undefined
              ? 'Too many reports for now. Try again later.'
              : `Too many reports for now. Try again in ${e.retryAfterSeconds} seconds.`
            : 'The report could not be sent. Try again.',
        );
      })
      .finally(() => setSending(false));
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
            Report this requirement
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
              {error !== undefined && (
                <Text
                  size="sm"
                  role="alert"
                  style={{color: 'var(--color-text-red)'}}
                >
                  {error}
                </Text>
              )}
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
                isLoading={sending}
                onClick={send}
              />
            </Stack>
          </>
        )}
      </Stack>
    </Dialog>
  );
}
