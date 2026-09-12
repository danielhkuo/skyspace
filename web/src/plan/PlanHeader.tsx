import {Badge} from '@astryxdesign/core/Badge';
import {useSyncExternalStore} from 'react';

import {getSaveStatus, subscribeSaveStatus} from '../datasource/planSaver';
import {Button} from '@astryxdesign/core/Button';
import {Icon} from '@astryxdesign/core/Icon';
import {Section} from '@astryxdesign/core/Section';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import type {ReactNode} from 'react';

import {
  formatCatalogYear,
  formatCredits,
  shortTermLabel,
  type Plan,
  type Program,
  type Report,
} from '../domain';
import {amberInk} from './paint';

type PlanHeaderProps = {
  plan: Plan;
  programs: Program[];
  report: Report;
  warningCount: number;
  onToggleWarnings: () => void;
  /** Tablet: the Saved and Requirements sheet buttons. */
  extraActions?: ReactNode;
};

function summaryLine(plan: Plan, programs: Program[]): string {
  const names = plan.programs
    .map(id => programs.find(p => p.id === id))
    .filter((p): p is Program => p !== undefined && p.kind !== 'university')
    .map(p => p.name);
  const last = plan.terms[plan.terms.length - 1];
  const parts = [
    ...names,
    `Catalog year ${formatCatalogYear(plan.catalogYear)}`,
    `matriculated ${shortTermLabel(plan.matriculation)}`,
  ];
  if (last !== undefined) {
    parts.push(`graduating ${shortTermLabel(last.position)}`);
  }
  return parts.join(' · ');
}

export function PlanHeader({
  plan,
  programs,
  report,
  warningCount,
  onToggleWarnings,
  extraActions,
}: PlanHeaderProps) {
  const {progress} = report;
  return (
    <Section
      variant="section"
      dividers={['bottom']}
      paddingInline={3}
      paddingBlock={1.5}
      width="100%"
    >
      <Stack
        direction="horizontal"
        width="100%"
        vAlign="center"
        hAlign="between"
        gap={3}
        wrap="wrap"
      >
        <Stack gap={0.5} align="start">
          <Stack direction="horizontal" gap={1} vAlign="center">
            <Text as="p" size="lg" weight="semibold" textWrap="nowrap">
              {plan.name}
            </Text>
            <Button
              label="Switch plan"
              variant="ghost"
              size="sm"
              isIconOnly
              icon={<Icon icon="chevronDown" size="sm" />}
            />
            <Button label="Settings" variant="ghost" size="sm" />
          </Stack>
          <Text type="supporting" maxLines={1}>
            {summaryLine(plan, programs)} · <SaveStatusText />
          </Text>
        </Stack>
        <Stack direction="horizontal" gap={2} vAlign="center">
          <Stack direction="horizontal" gap={1} vAlign="center">
            <Text weight="medium" size="sm" hasTabularNumbers textWrap="nowrap">
              {formatCredits(progress.creditsMet)} of{' '}
              {formatCredits(progress.creditsRequired)} credit hours
            </Text>
            {progress.creditsUnknown > 0 && (
              <Text size="sm" textWrap="nowrap" style={amberInk}>
                · at least
              </Text>
            )}
          </Stack>
          {extraActions}
          <Button label="Export PDF" variant="secondary" size="sm" />
          <Button
            label="Warnings"
            variant={warningCount > 0 ? 'primary' : 'secondary'}
            size="sm"
            icon={<Icon icon="warning" size="sm" />}
            endContent={
              <Badge
                variant={warningCount > 0 ? 'warning' : 'neutral'}
                label={warningCount}
              />
            }
            onClick={onToggleWarnings}
          />
        </Stack>
      </Stack>
    </Section>
  );
}

const STATUS_TEXT = {
  saved: 'Saved',
  pending: 'Unsaved changes',
  saving: 'Saving…',
};

function SaveStatusText() {
  const status = useSyncExternalStore(
    subscribeSaveStatus,
    getSaveStatus,
    getSaveStatus,
  );
  return <>{STATUS_TEXT[status]}</>;
}
