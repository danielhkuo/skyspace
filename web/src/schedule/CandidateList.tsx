import {Card} from '@astryxdesign/core/Card';
import {Divider} from '@astryxdesign/core/Divider';
import {DropdownMenu} from '@astryxdesign/core/DropdownMenu';
import {Icon} from '@astryxdesign/core/Icon';
import {IconButton} from '@astryxdesign/core/IconButton';
import {MoreMenu} from '@astryxdesign/core/MoreMenu';
import {Stack, StackItem} from '@astryxdesign/core/Stack';
import {StatusDot} from '@astryxdesign/core/StatusDot';
import {Text} from '@astryxdesign/core/Text';
import type {CSSProperties} from 'react';

import {
  courseKey,
  formatAsOf,
  formatCourseCode,
  formatCredits,
  creditRangeMin,
  seatStatus,
  type Candidate,
  type Crn,
  type Section,
  type TermSchedule,
} from '../domain';
import {seatsDot} from '../catalog/labels';
import {
  COLOURS,
  DAY_NAME,
  HUE_LABEL,
  hueOf,
  seatsLine,
  sectionLabel,
} from './labels';
import {EyeMark} from './marks';
import type {ScheduleAction} from './reduce';

type CandidateListProps = {
  schedule: TermSchedule;
  sectionsByCourse: ReadonlyMap<string, Section[]>;
  sectionsByCrn: ReadonlyMap<Crn, Section>;
  conflicts: ReadonlyMap<Crn, {crn: Crn; days: string[]}[]>;
  dispatch: (action: ScheduleAction) => void;
};

const swatch = (hue: string): CSSProperties => ({
  width: 12,
  height: 12,
  flexShrink: 0,
  borderRadius: 'var(--radius-inner)',
  background: `var(--color-border-${hue})`,
});
const dimmed: CSSProperties = {opacity: 0.55};

/** The candidates, one block each: title row, section picker, seats, conflicts. */
export function CandidateList({
  schedule,
  sectionsByCourse,
  sectionsByCrn,
  conflicts,
  dispatch,
}: CandidateListProps) {
  return (
    <Stack width="100%" gap={0}>
      {schedule.candidates.map((candidate, i) => (
        <Stack key={courseKey(candidate.course)} width="100%" gap={0}>
          {i > 0 && <Divider />}
          <CandidateRow
            candidate={candidate}
            sections={sectionsByCourse.get(courseKey(candidate.course))}
            sectionsByCrn={sectionsByCrn}
            conflicts={conflicts}
            dispatch={dispatch}
          />
        </Stack>
      ))}
    </Stack>
  );
}

type CandidateRowProps = {
  candidate: Candidate;
  /** `undefined` until the course's sections have loaded. */
  sections: Section[] | undefined;
  sectionsByCrn: ReadonlyMap<Crn, Section>;
  conflicts: ReadonlyMap<Crn, {crn: Crn; days: string[]}[]>;
  dispatch: (action: ScheduleAction) => void;
};

function CandidateRow({
  candidate,
  sections,
  sectionsByCrn,
  conflicts,
  dispatch,
}: CandidateRowProps) {
  const code = formatCourseCode(candidate.course);
  const hue = hueOf(candidate.colour);
  const picked = candidate.sections
    .map(crn => sectionsByCrn.get(crn))
    .filter((s): s is Section => s !== undefined);
  const first = picked[0] ?? sections?.[0];
  const title = first?.listing.title ?? '';
  const credits =
    first === undefined ? undefined : creditRangeMin(first.listing.credits);
  const clashes = candidate.visible
    ? candidate.sections.flatMap(crn => conflicts.get(crn) ?? [])
    : [];

  return (
    <Stack
      width="100%"
      gap={1}
      paddingBlock={2}
      align="start"
      style={candidate.visible ? undefined : dimmed}
    >
      <Stack direction="horizontal" width="100%" gap={1.5} vAlign="center">
        <Stack style={swatch(hue)} />
        <Text
          weight="semibold"
          size="sm"
          hasTabularNumbers
          textWrap="nowrap"
          color={candidate.visible ? undefined : 'secondary'}
        >
          {code}
        </Text>
        <StackItem size="fill" style={{minWidth: 0}}>
          <Text type="supporting">{title}</Text>
        </StackItem>
        <Text type="supporting" hasTabularNumbers textWrap="nowrap">
          {credits === undefined ? '' : `${formatCredits(credits)} cr`}
        </Text>
        <IconButton
          label={candidate.visible ? `Hide ${code}` : `Show ${code}`}
          variant="ghost"
          size="sm"
          icon={
            candidate.visible ? (
              <Icon icon={EyeMark} size="sm" />
            ) : (
              <Icon icon="eyeSlash" size="sm" />
            )
          }
          onClick={() =>
            dispatch({type: 'toggleVisible', course: candidate.course})
          }
        />
        <MoreMenu
          label={`More for ${code}`}
          size="sm"
          alignment="end"
          items={[
            {
              type: 'section',
              title: 'Colour',
              items: COLOURS.map((h, colour) => ({
                id: h,
                label: HUE_LABEL[h],
                icon:
                  colour === candidate.colour % COLOURS.length ? (
                    <Icon icon="check" size="sm" />
                  ) : undefined,
                onClick: () =>
                  dispatch({
                    type: 'setColour',
                    course: candidate.course,
                    colour,
                  }),
              })),
            },
            {type: 'divider'},
            {
              id: 'remove',
              label: 'Remove from schedule',
              variant: 'destructive',
              onClick: () =>
                dispatch({type: 'removeCandidate', course: candidate.course}),
            },
          ]}
        />
      </Stack>

      <Stack
        direction="horizontal"
        width="100%"
        gap={1.5}
        vAlign="center"
        wrap="wrap"
      >
        <SectionPicker
          candidate={candidate}
          sections={sections}
          onPick={crn =>
            dispatch({type: 'pickSection', course: candidate.course, crn})
          }
        />
        {picked[0] !== undefined && <SeatsRow section={picked[0]} />}
      </Stack>

      {clashes.map(clash => {
        const other = sectionsByCrn.get(clash.crn);
        const name =
          other === undefined
            ? `CRN ${clash.crn}`
            : formatCourseCode(other.listing.code);
        const days = clash.days.map(d => DAY_NAME[d as keyof typeof DAY_NAME]);
        return (
          <Stack
            key={clash.crn}
            direction="horizontal"
            width="100%"
            gap={1}
            vAlign="center"
          >
            <Icon icon="warning" size="xsm" color="warning" label="Warning" />
            <Text type="supporting">
              Conflicts with {name} on {days.join(', ')}
            </Text>
          </Stack>
        );
      })}
    </Stack>
  );
}

function SeatsRow({section}: {section: Section}) {
  const {seats} = section;
  if (seats === undefined) {
    return (
      <Stack direction="horizontal" gap={1.5} vAlign="center">
        <StatusDot variant="neutral" label="Seats not polled" />
        <Text type="supporting" textWrap="nowrap">
          seats not polled
        </Text>
      </Stack>
    );
  }
  const dot = seatsDot(seats);
  const line = seatsLine(seats);
  return (
    <Stack direction="horizontal" gap={1.5} vAlign="center">
      <StatusDot {...dot} />
      <Text type="supporting" textWrap="nowrap">
        {seatStatus(seats) === 'nearlyFull'
          ? `${line} · as of ${formatAsOf(seats.asOf)}`
          : line}
      </Text>
    </Stack>
  );
}

type SectionPickerProps = {
  candidate: Candidate;
  sections: Section[] | undefined;
  onPick: (crn: Crn) => void;
};

/** "002 · TR 4:00–5:15 PM ▾": every section of the course, seats beside each. */
function SectionPicker({candidate, sections, onPick}: SectionPickerProps) {
  const pickedCrn = candidate.sections[0];
  const pickedSection = sections?.find(s => s.listing.crn === pickedCrn);
  const label =
    sections === undefined
      ? 'Loading sections…'
      : sections.length === 0
        ? 'Not offered this term'
        : pickedSection === undefined
          ? 'Pick a section'
          : sectionLabel(pickedSection);
  return (
    <DropdownMenu
      button={{
        label,
        variant: 'secondary',
        size: 'sm',
        isDisabled: sections === undefined || sections.length === 0,
      }}
      hasChevron
      menuWidth={320}
      items={(sections ?? []).map(s => ({
        id: s.listing.crn,
        label: sectionLabel(s),
        description: `CRN ${s.listing.crn} · ${seatsLine(s.seats)}`,
        icon:
          s.listing.crn === pickedCrn ? (
            <Icon icon="check" size="sm" />
          ) : undefined,
        onClick: () => onPick(s.listing.crn),
      }))}
    />
  );
}

/** Kept for the tablet sheet: the same list inside a card. */
export function CandidateListCard(props: CandidateListProps) {
  return (
    <Card padding={0} width="100%">
      <CandidateList {...props} />
    </Card>
  );
}
