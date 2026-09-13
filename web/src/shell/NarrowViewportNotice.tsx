import {Center} from '@astryxdesign/core/Center';
import {Link} from '@astryxdesign/core/Link';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';

/** Shown instead of the app below 768 px. No dismiss, no degraded mode. */
export function NarrowViewportNotice() {
  return (
    <Center height="100dvh" padding={6}>
      <Stack gap={3} maxWidth={360} align="start">
        <Text weight="semibold" size="lg">
          Skyspace
        </Text>
        <Text>
          Skyspace needs a wider screen. It is built for laptops and tablets,
          and we would rather say so than ship a cramped version.
        </Text>
        <Link href="https://courses.rice.edu/" isExternalLink>
          Rice&apos;s course schedule
        </Link>
      </Stack>
    </Center>
  );
}
