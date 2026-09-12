import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';

type PlaceholderPageProps = {
  title: string;
};

/** Stands in for a page that is not built yet, so navigation works end to end. */
export function PlaceholderPage({title}: PlaceholderPageProps) {
  return (
    <Stack padding={4} gap={1} align="start">
      <Text as="p" type="label" weight="semibold">
        {title}
      </Text>
      <Text type="supporting">Not built yet.</Text>
    </Stack>
  );
}
