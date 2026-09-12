import {Heading} from '@astryxdesign/core/Heading';
import {Text} from '@astryxdesign/core/Text';
import {VStack} from '@astryxdesign/core/VStack';

/**
 * Placeholder shell. Real pages are built from Astryx templates:
 * run `npx astryx build "<idea>"` and `npx astryx docs layout` first.
 */
export function App() {
  return (
    <VStack>
      <Heading level={1}>Skyspace</Heading>
      <Text>
        Course discovery and degree planning for Rice University students.
      </Text>
    </VStack>
  );
}
