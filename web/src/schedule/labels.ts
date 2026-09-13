/** Copy the schedule page shares between the list, the grid and the header. */
import {
  formatMinute,
  seatsOpen,
  seatStatus,
  timedMeetings,
  type Day,
  type MeetingTime,
  type Seats,
  type Section,
} from '../domain';
import type {Hue} from '../plan/paint';

export const DAY_NAME: Record<Day, string> = {
  M: 'Monday',
  T: 'Tuesday',
  W: 'Wednesday',
  R: 'Thursday',
  F: 'Friday',
  S: 'Saturday',
  U: 'Sunday',
};

/** The ten hues a stored colour index maps onto; the server never reads the number. */
export const COLOURS: Hue[] = [
  'blue',
  'teal',
  'green',
  'orange',
  'pink',
  'purple',
  'red',
  'cyan',
  'yellow',
  'gray',
];

export function hueOf(colour: number): Hue {
  return (
    COLOURS[((colour % COLOURS.length) + COLOURS.length) % COLOURS.length] ??
    'gray'
  );
}

export const HUE_LABEL: Record<Hue, string> = {
  blue: 'Blue',
  teal: 'Teal',
  green: 'Green',
  orange: 'Orange',
  pink: 'Pink',
  purple: 'Purple',
  red: 'Red',
  cyan: 'Cyan',
  yellow: 'Yellow',
  gray: 'Gray',
};

/** `4:00–5:15 PM`, meridiem once when both ends share it. */
export function timeSpan(time: MeetingTime): string {
  const sameHalf = time.start < 720 === time.end < 720;
  return `${formatMinute(time.start, !sameHalf)}–${formatMinute(time.end)}`;
}

/** `TR 4:00–5:15 PM`: days first, the way a picker reads. */
export function meetingLabel(time: MeetingTime): string {
  return `${time.days.join('')} ${timeSpan(time)}`;
}

/** `002 · TR 4:00–5:15 PM · R 4:00 PM`, or `002 · no meeting time`. */
export function sectionLabel(section: Section): string {
  const times = timedMeetings(section.listing);
  const meets =
    times.length === 0
      ? 'no meeting time'
      : times.map(meetingLabel).join(' · ');
  return `${section.listing.section} · ${meets}`;
}

/** The short seat line under a picked section: `2 open · as of 7:09 PM`, `Full · waitlist 4 of 10`. */
export function seatsLine(seats: Seats | undefined): string {
  if (seats === undefined) {
    return 'seats not polled';
  }
  switch (seatStatus(seats)) {
    case 'open':
    case 'nearlyFull':
      return `${seatsOpen(seats)} open`;
    case 'waitlistOpen':
      return `Full · waitlist ${seats.waitlistCount} of ${seats.waitlistCapacity}`;
    case 'full':
      return 'Full';
  }
}

/** "Schedule A", "Schedule B", … past the names already taken. */
export function nextScheduleName(taken: string[]): string {
  const have = new Set(taken);
  for (let i = 0; i < 26; i += 1) {
    const name = `Schedule ${String.fromCharCode(65 + i)}`;
    if (!have.has(name)) {
      return name;
    }
  }
  return `Schedule ${taken.length + 1}`;
}
