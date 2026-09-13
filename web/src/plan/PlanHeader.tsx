import {Badge} from '@astryxdesign/core/Badge';
import {DropdownMenu} from '@astryxdesign/core/DropdownMenu';
import {useNavigate} from 'react-router';
import {useSyncExternalStore} from 'react';

import {getSaveStatus, subscribeSaveStatus} from '../datasource/planSaver';
import {Button} from '@astryxdesign/core/Button';
import {Icon} from '@astryxdesign/core/Icon';
import {LayoutHeader} from '@astryxdesign/core/Layout';
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
  onOpenSettings: () => void;
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
  onOpenSettings,
}: PlanHeaderProps) {
  const navigate = useNavigate();
  const {progress} = report;
  return (
    <LayoutHeader hasDivider padding={3}>
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
            <DropdownMenu
              button={{label: plan.name, variant: 'ghost', size: 'md'}}
              hasChevron
              items={[
                {
                  id: 'current',
                  label: plan.name,
                  description: 'Current plan',
                  isDisabled: true,
                },
                {type: 'divider'},
                {
                  id: 'new',
                  label: 'Start a new plan…',
                  description: 'Programs, timeline, incoming credit',
                  onClick: () => void navigate('/plan/new'),
                },
              ]}
            />
            <Button
              label="Settings"
              variant="ghost"
              size="sm"
              onClick={onOpenSettings}
            />
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
    </LayoutHeader>
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
