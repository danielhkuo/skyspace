import {Button} from '@astryxdesign/core/Button';
import {Card} from '@astryxdesign/core/Card';
import {Icon} from '@astryxdesign/core/Icon';
import {Stack, StackItem} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';

type LoadErrorCardProps = {
  what: string;
  onRetry: () => void;
};

/** "Server unreachable" (`States` 6): the page says what is safe and offers one retry. */
export function LoadErrorCard({what, onRetry}: LoadErrorCardProps) {
  return (
    <Stack width="100%" padding={4} align="center">
      <Card padding={3} width={448} maxWidth="100%">
        <Stack width="100%" gap={2} align="start">
          <Stack direction="horizontal" width="100%" gap={1.5} vAlign="start">
            <Icon icon="error" size="sm" color="error" label="Error" />
            <StackItem size="fill">
              <Text size="sm">
                Skyspace couldn&apos;t load {what}. Anything you saved is still
                in this browser.
              </Text>
            </StackItem>
          </Stack>
          <Button
            label="Retry"
            variant="secondary"
            size="sm"
            onClick={onRetry}
          />
        </Stack>
      </Card>
    </Stack>
  );
}
