import {Card} from '@astryxdesign/core/Card';
import {Icon} from '@astryxdesign/core/Icon';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';

import type {Plan, Warning} from '../domain';
import {termName, warningSubject, warningTerm, warningText} from './labels';
import {HollowMark} from './marks';
import {islandHead, violetInk} from './paint';

type WarningsPanelProps = {
  plan: Plan;
  warnings: Warning[];
};

/** Sits above the board. Stays put during a drag; the board's own tints do the talking then. */
export function WarningsPanel({plan, warnings}: WarningsPanelProps) {
  return (
    <Card padding={0} width="100%" variant="yellow">
      <Stack width="100%" gap={0}>
        <Stack
          direction="horizontal"
          width="100%"
          paddingInline={2}
          paddingBlock={1.5}
          gap={1.5}
          vAlign="center"
          style={islandHead}
        >
          <Icon icon="warning" size="sm" color="warning" label="" />
          <Text size="sm" weight="semibold">
            {warnings.length === 0
              ? 'No warnings'
              : `${warnings.length} warning${warnings.length === 1 ? '' : 's'}`}
          </Text>
        </Stack>
        {warnings.map((warning, i) => {
          const term = warningTerm(warning);
          const self = warning.kind === 'selfCheck';
          return (
            <Stack
              key={i}
              direction="horizontal"
              width="100%"
              paddingInline={2}
              paddingBlock={1}
              gap={1.5}
              vAlign="start"
              style={i === 0 ? undefined : islandHead}
            >
              {self ? (
                <Icon
                  icon={HollowMark}
                  size="sm"
                  label="Self-check"
                  style={violetInk}
                />
              ) : (
                <Icon
                  icon="warning"
                  size="sm"
                  color="warning"
                  label="Warning"
                />
              )}
              <Text size="sm" weight="medium" textWrap="nowrap">
                {warningSubject(warning)}
              </Text>
              <Text type="supporting" textWrap="nowrap">
                {self
                  ? 'check yourself'
                  : term === undefined
                    ? ''
                    : termName(plan, term)}
              </Text>
              <Text size="sm">{warningText(warning, plan)}</Text>
            </Stack>
          );
        })}
      </Stack>
    </Card>
  );
}
