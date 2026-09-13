import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import type {CSSProperties} from 'react';

import {
  formatInstructors,
  formatMeetings,
  type Crn,
  type Section,
} from '../domain';
import {rowDivider} from '../plan/paint';
import {SeatSummary} from './SeatSummary';

type OtherSectionsProps = {
  /** Every section of the course, the open one included. */
  sections: Section[];
  current: Crn;
  onSelect: (crn: Crn) => void;
};

const row: CSSProperties = {...rowDivider, cursor: 'pointer'};
const currentRow: CSSProperties = {
  ...rowDivider,
  boxShadow: 'inset 2px 0 0 0 var(--color-accent)',
  background: 'var(--color-accent-muted)',
};

/** The course's sections as compact rows inside the pane; clicking one opens it in place. */
export function OtherSections({
  sections,
  current,
  onSelect,
}: OtherSectionsProps) {
  return (
    <Stack width="100%" gap={0}>
      {sections.map(section => {
        const {listing} = section;
        const isCurrent = listing.crn === current;
        const meets = formatMeetings(listing);
        return (
          <Stack
            key={listing.crn}
            direction="horizontal"
            width="100%"
            paddingInline={1.5}
            paddingBlock={1}
            gap={1.5}
            vAlign="center"
            hAlign="between"
            style={isCurrent ? currentRow : row}
            onClick={isCurrent ? undefined : () => onSelect(listing.crn)}
            tabIndex={isCurrent ? undefined : 0}
            onKeyDown={e => {
              if (e.key === 'Enter' && e.target === e.currentTarget) {
                onSelect(listing.crn);
              }
            }}
          >
            <Stack gap={0} align="start">
              <Stack direction="horizontal" gap={1} vAlign="end" wrap="wrap">
                <Text size="sm" weight="semibold" hasTabularNumbers>
                  {listing.section}
                </Text>
                <Text size="sm" hasTabularNumbers>
                  {meets === '' ? 'no meeting time' : meets}
                </Text>
              </Stack>
              <Text type="supporting" maxLines={1}>
                {formatInstructors(listing) || 'No instructor listed'}
              </Text>
            </Stack>
            <SeatSummary seats={section.seats} layout="cell" />
          </Stack>
        );
      })}
    </Stack>
  );
}
