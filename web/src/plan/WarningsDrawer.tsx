import {Icon} from '@astryxdesign/core/Icon';
import {Section} from '@astryxdesign/core/Section';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';

import type {Plan, Warning} from '../domain';
import {termName, warningSubject, warningTerm, warningText} from './labels';
import {HollowMark} from './marks';
import {violetInk} from './paint';

type WarningsDrawerProps = {
  plan: Plan;
  warnings: Warning[];
  /** While a card is in the air the drawer is one line. */
  collapsed: boolean;
};

export function WarningsDrawer({
  plan,
  warnings,
  collapsed,
}: WarningsDrawerProps) {
  if (collapsed) {
    return (
      <Section
        variant="muted"
        dividers={['top']}
        paddingInline={3}
        paddingBlock={2}
        width="100%"
      >
        <Stack direction="horizontal" width="100%" gap={1.5} vAlign="center">
          <Icon icon="info" size="sm" color="secondary" label="Note" />
          <Text size="sm">
            Drop anywhere. Skyspace warns after, never blocks.
          </Text>
        </Stack>
      </Section>
    );
  }
  return (
    <Section
      variant="muted"
      dividers={['top']}
      paddingInline={3}
      paddingBlock={2}
      width="100%"
    >
      <Stack width="100%" gap={1.5}>
        <Stack
          direction="horizontal"
          width="100%"
          hAlign="between"
          vAlign="center"
        >
          <Text type="label" weight="semibold">
            Warnings
          </Text>
          <Text type="supporting">
            {warnings.length} · the plan is yours; Skyspace warns and never
            blocks
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
              gap={1.5}
              vAlign="start"
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
                  ? 'self-check'
                  : term === undefined
                    ? ''
                    : termName(plan, term)}
              </Text>
              <Text size="sm">{warningText(warning, plan)}</Text>
            </Stack>
          );
        })}
        {warnings.length === 0 && (
          <Text type="supporting">Nothing to warn about.</Text>
        )}
      </Stack>
    </Section>
  );
}
