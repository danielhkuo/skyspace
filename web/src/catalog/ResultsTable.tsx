import {Icon} from '@astryxdesign/core/Icon';
import {IconButton} from '@astryxdesign/core/IconButton';
import {
  Table,
  TableBody,
  TableCell,
  TableHeader,
  TableHeaderCell,
  TableRow,
} from '@astryxdesign/core/Table';
import {Text} from '@astryxdesign/core/Text';
import {VisuallyHidden} from '@astryxdesign/core/VisuallyHidden';
import type {CSSProperties} from 'react';

import {
  courseKey,
  formatCourseCode,
  formatCreditRange,
  formatInstructors,
  formatMeetings,
  type CourseCode,
  type Crn,
  type Section,
} from '../domain';
import {StarFilledMark, StarMark} from './marks';
import {SeatSummary} from './SeatSummary';

type ResultsTableProps = {
  rows: Section[];
  selected: Crn | undefined;
  onSelect: (crn: Crn) => void;
  isFavorite: (code: CourseCode) => boolean;
  onToggleFavorite: (code: CourseCode) => void;
};

/**
 * Fixed layout, so the header row owns the column widths. Structural widths
 * are the one place raw px belongs (`web/AGENTS.md`); each clears the widest
 * thing it holds: the seat timestamp, Rice's "1 TO 4", the star button.
 * No tags column: the pane shows the distribution group, and the title
 * needs the room at 1440 once the rail and pane take theirs.
 */
const col = (px: number): CSSProperties => ({width: px, maxWidth: px});
const COLS = {
  code: col(84),
  sec: col(44),
  inst: col(96),
  meets: col(120),
  cr: col(50),
  seats: col(104),
  star: col(44),
};
const fixedLayout: CSSProperties = {
  tableLayout: 'fixed',
  width: '100%',
  minWidth: 700,
};
const clickable: CSSProperties = {cursor: 'pointer'};
/** The wash goes on the cells: the table paints cell backgrounds above the row's. */
const selectedCell: CSSProperties = {
  background: 'var(--color-accent-muted)',
};
const selectedFirstCell: CSSProperties = {
  ...selectedCell,
  boxShadow: 'inset 2px 0 0 0 var(--color-accent)',
};

/** One row per section. Click or Enter opens it in the pane; the star never opens it. */
export function ResultsTable({
  rows,
  selected,
  onSelect,
  isFavorite,
  onToggleFavorite,
}: ResultsTableProps) {
  return (
    <Table
      density="compact"
      dividers="rows"
      hasHover
      textOverflow="wrap"
      style={fixedLayout}
    >
      <TableHeader>
        <TableRow isHeaderRow>
          <TableHeaderCell style={COLS.code}>Code</TableHeaderCell>
          <TableHeaderCell style={COLS.sec}>Sec</TableHeaderCell>
          <TableHeaderCell>Title</TableHeaderCell>
          <TableHeaderCell style={COLS.inst}>Instructor</TableHeaderCell>
          <TableHeaderCell style={COLS.meets}>Meets</TableHeaderCell>
          <TableHeaderCell style={COLS.cr}>Cr</TableHeaderCell>
          <TableHeaderCell style={COLS.seats}>Seats</TableHeaderCell>
          <TableHeaderCell style={COLS.star}>
            <VisuallyHidden>Favorite</VisuallyHidden>
          </TableHeaderCell>
        </TableRow>
      </TableHeader>
      <TableBody>
        {rows.map((section, i) => {
          const {listing} = section;
          const previous = rows[i - 1];
          const repeat =
            previous !== undefined &&
            courseKey(previous.listing.code) === courseKey(listing.code);
          const isSelected = listing.crn === selected;
          const meets = formatMeetings(listing);
          const starred = isFavorite(listing.code);
          const cell = isSelected ? selectedCell : undefined;
          return (
            <TableRow
              key={listing.crn}
              style={clickable}
              onClick={() => onSelect(listing.crn)}
              tabIndex={0}
              aria-selected={isSelected}
              onKeyDown={e => {
                if (e.key === 'Enter' && e.target === e.currentTarget) {
                  onSelect(listing.crn);
                }
              }}
            >
              <TableCell style={isSelected ? selectedFirstCell : undefined}>
                <Text
                  weight="semibold"
                  size="sm"
                  color={repeat ? 'secondary' : undefined}
                  hasTabularNumbers
                  textWrap="nowrap"
                >
                  {formatCourseCode(listing.code)}
                </Text>
              </TableCell>
              <TableCell style={cell}>
                <Text size="sm" hasTabularNumbers>
                  {listing.section}
                </Text>
              </TableCell>
              <TableCell style={cell}>
                <Text size="sm">{listing.title}</Text>
              </TableCell>
              <TableCell style={cell}>
                <Text size="sm" color="secondary">
                  {formatInstructors(listing)}
                </Text>
              </TableCell>
              <TableCell style={cell}>
                {meets === '' ? (
                  <Text size="sm" color="secondary">
                    no meeting time
                  </Text>
                ) : (
                  <Text size="sm" hasTabularNumbers>
                    {meets}
                  </Text>
                )}
              </TableCell>
              <TableCell style={cell}>
                <Text size="sm" hasTabularNumbers>
                  {formatCreditRange(listing.credits)}
                </Text>
              </TableCell>
              <TableCell style={cell}>
                <SeatSummary seats={section.seats} layout="cell" />
              </TableCell>
              <TableCell style={cell}>
                <IconButton
                  label={
                    starred
                      ? `Remove ${formatCourseCode(listing.code)} from favorites`
                      : `Add ${formatCourseCode(listing.code)} to favorites`
                  }
                  variant="ghost"
                  size="sm"
                  icon={
                    <Icon
                      icon={starred ? StarFilledMark : StarMark}
                      size="sm"
                    />
                  }
                  onClick={e => {
                    e.stopPropagation();
                    onToggleFavorite(listing.code);
                  }}
                />
              </TableCell>
            </TableRow>
          );
        })}
      </TableBody>
    </Table>
  );
}
