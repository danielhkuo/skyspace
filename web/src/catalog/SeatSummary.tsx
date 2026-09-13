import {Stack} from '@astryxdesign/core/Stack';
import {StatusDot} from '@astryxdesign/core/StatusDot';
import {Text} from '@astryxdesign/core/Text';

import {formatAsOf, seatStatus, seatsOpen, type Seats} from '../domain';
import {SEAT_DOT} from './labels';

type SeatSummaryProps = {
  seats: Seats | undefined;
  /** One line for a table cell; two lines with the timestamp otherwise. */
  layout: 'cell' | 'line';
};

/** "5 open · as of 7:09 PM", "Full · waitlist 0 of 0", or "not polled". Never zero for unknown. */
export function SeatSummary({seats, layout}: SeatSummaryProps) {
  if (seats === undefined) {
    return (
      <Stack direction="horizontal" gap={1} vAlign="center">
        <StatusDot variant="neutral" label="Seats not polled" />
        <Text size="sm" color="secondary" textWrap="nowrap">
          not polled
        </Text>
      </Stack>
    );
  }
  const status = seatStatus(seats);
  const dot = SEAT_DOT[status];
  const open = seatsOpen(seats);
  const headline =
    status === 'full' || status === 'waitlistOpen' ? 'Full' : `${open} open`;
  const detail =
    status === 'full' || status === 'waitlistOpen'
      ? `waitlist ${seats.waitlistCount} of ${seats.waitlistCapacity}`
      : `as of ${formatAsOf(seats.asOf)}`;

  if (layout === 'line') {
    return (
      <Stack direction="horizontal" gap={1.5} vAlign="center" wrap="wrap">
        <StatusDot variant={dot.variant} label={dot.label} />
        <Text size="sm" textWrap="nowrap">
          {headline}
        </Text>
        <Text type="supporting" textWrap="nowrap">
          {detail}
        </Text>
      </Stack>
    );
  }
  return (
    <Stack gap={0} align="start">
      <Stack direction="horizontal" gap={1} vAlign="center">
        <StatusDot variant={dot.variant} label={dot.label} />
        <Text size="sm" textWrap="nowrap">
          {headline}
        </Text>
      </Stack>
      <Text type="supporting" size="xsm" textWrap="nowrap">
        {detail}
      </Text>
    </Stack>
  );
}
