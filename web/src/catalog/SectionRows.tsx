import {Icon} from '@astryxdesign/core/Icon';
import {IconButton} from '@astryxdesign/core/IconButton';
import {Stack, StackItem} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import type {CSSProperties} from 'react';

import {
  courseKey,
  formatCourseCode,
  formatCreditRange,
  formatInstructors,
  formatMeetings,
  isDistributionGroup,
  type CourseCode,
  type Crn,
  type Section,
} from '../domain';
import {rowDivider} from '../plan/paint';
import {StarFilledMark, StarMark} from './marks';
import {SeatSummary} from './SeatSummary';

type SectionRowsProps = {
  rows: Section[];
  selected: Crn | undefined;
  onSelect: (crn: Crn) => void;
  isFavorite: (code: CourseCode) => boolean;
  onToggleFavorite: (code: CourseCode) => void;
};

const selectedRow: CSSProperties = {
  ...rowDivider,
  background: 'var(--color-accent-muted)',
  boxShadow: 'inset 2px 0 0 0 var(--color-accent)',
  cursor: 'pointer',
};
const plainRow: CSSProperties = {...rowDivider, cursor: 'pointer'};

/** The tablet results: one stacked row per section, 44 px targets, no table. */
export function SectionRows({
  rows,
  selected,
  onSelect,
  isFavorite,
  onToggleFavorite,
}: SectionRowsProps) {
  return (
    <Stack width="100%" gap={0}>
      {rows.map((section, i) => {
        const {listing, detail} = section;
        const previous = rows[i - 1];
        const repeat =
          previous !== undefined &&
          courseKey(previous.listing.code) === courseKey(listing.code);
        const isSelected = listing.crn === selected;
        const starred = isFavorite(listing.code);
        const meets = formatMeetings(listing);
        const tags = (detail?.attributes ?? []).filter(isDistributionGroup);
        const byline = [formatInstructors(listing), ...tags]
          .filter(s => s !== '')
          .join(' · ');
        return (
          <Stack
            key={listing.crn}
            direction="horizontal"
            width="100%"
            paddingInline={2}
            paddingBlock={1.5}
            gap={1.5}
            vAlign="start"
            style={isSelected ? selectedRow : plainRow}
            onClick={() => onSelect(listing.crn)}
            tabIndex={0}
            onKeyDown={e => {
              if (e.key === 'Enter' && e.target === e.currentTarget) {
                onSelect(listing.crn);
              }
            }}
          >
            <StackItem size="fill">
              <Stack width="100%" gap={0.5} align="start">
                <Stack
                  direction="horizontal"
                  gap={1.5}
                  vAlign="end"
                  wrap="wrap"
                >
                  <Text
                    weight="semibold"
                    size="sm"
                    color={repeat ? 'secondary' : undefined}
                    hasTabularNumbers
                    textWrap="nowrap"
                  >
                    {formatCourseCode(listing.code)} · {listing.section}
                  </Text>
                  <Text size="sm">{listing.title}</Text>
                </Stack>
                {byline !== '' && <Text type="supporting">{byline}</Text>}
                <Stack
                  direction="horizontal"
                  gap={1.5}
                  vAlign="center"
                  wrap="wrap"
                >
                  {meets === '' ? (
                    <Text size="sm" color="secondary" textWrap="nowrap">
                      no meeting time
                    </Text>
                  ) : (
                    <Text size="sm" hasTabularNumbers textWrap="nowrap">
                      {meets}
                    </Text>
                  )}
                  <Text type="supporting" hasTabularNumbers textWrap="nowrap">
                    {formatCreditRange(listing.credits)} cr
                  </Text>
                  <SeatSummary seats={section.seats} layout="line" />
                </Stack>
              </Stack>
            </StackItem>
            <IconButton
              label={
                starred
                  ? `Remove ${formatCourseCode(listing.code)} from favorites`
                  : `Add ${formatCourseCode(listing.code)} to favorites`
              }
              variant="ghost"
              size="md"
              icon={
                <Icon icon={starred ? StarFilledMark : StarMark} size="sm" />
              }
              onClick={e => {
                e.stopPropagation();
                onToggleFavorite(listing.code);
              }}
            />
          </Stack>
        );
      })}
    </Stack>
  );
}
