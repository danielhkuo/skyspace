import {Card} from '@astryxdesign/core/Card';
import {Link} from '@astryxdesign/core/Link';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {Token} from '@astryxdesign/core/Token';

import type {FillsSummary} from './fills';

type FillsCardProps = {fills: FillsSummary};

/** "In CS + Stats minor, this course is in Fall 2024 and fills …", or what it could fill. */
export function FillsCard({fills}: FillsCardProps) {
  const {planName, placedIn} = fills;
  const entries = placedIn === undefined ? fills.candidates : fills.fills;
  const lead =
    placedIn === undefined
      ? entries.length === 0
        ? ', this course fills no requirement.'
        : ', this course could fill'
      : entries.length === 0
        ? `, this course is in ${placedIn} and fills no requirement.`
        : `, this course is in ${placedIn} and fills`;
  return (
    <Card variant="muted" padding={2} width="100%">
      <Stack width="100%" gap={1} align="start">
        <Text type="label" weight="semibold">
          Fills in your plan
        </Text>
        <Stack direction="horizontal" gap={1} wrap="wrap" vAlign="center">
          <Text size="sm">
            In{' '}
            <Text type="inherit" weight="semibold">
              {planName}
            </Text>
            {lead}
          </Text>
          {entries.map((e, i) => (
            <Stack
              key={`${e.program}-${e.path}`}
              direction="horizontal"
              gap={1}
              vAlign="center"
            >
              {i > 0 && <Text size="sm">and</Text>}
              <Token label={e.path} size="sm" description={e.program} />
            </Stack>
          ))}
          <Link href="/plan" size="sm">
            {placedIn === undefined ? 'open plan' : 'change'}
          </Link>
        </Stack>
      </Stack>
    </Card>
  );
}
