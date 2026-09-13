import {Card} from '@astryxdesign/core/Card';
import {Stack} from '@astryxdesign/core/Stack';
import {StatusDot} from '@astryxdesign/core/StatusDot';
import {Text} from '@astryxdesign/core/Text';
import type {CSSProperties} from 'react';

import {
  formatCourseCode,
  formatMinute,
  seatStatus,
  timedMeetings,
  type Crn,
  type Day,
  type Section,
  type TermSchedule,
} from '../domain';
import {seatsDot} from '../catalog/labels';
import {DAY_NAME, hueOf, timeSpan} from './labels';
import type {Hue} from '../plan/paint';

type WeekGridProps = {
  schedule: TermSchedule;
  sectionsByCrn: ReadonlyMap<Crn, Section>;
  /** CRN → the days it clashes on, so only the clashing meetings are outlined. */
  conflicts: ReadonlyMap<Crn, {crn: Crn; days: string[]}[]>;
};

/** One hour of the day, in pixels: the artboard's scale. A 50-minute class is 50 px. */
const HOUR_PX = 60;
const GUTTER_WIDTH = 56;
const HEAD_HEIGHT = 32;
const DEFAULT_START = 8 * 60;
const DEFAULT_END = 21 * 60;
const WEEKDAYS: Day[] = ['M', 'T', 'W', 'R', 'F'];

type Block = {
  key: string;
  crn: Crn;
  code: string;
  section: string;
  start: number;
  end: number;
  hue: Hue;
  seats: Section['seats'];
  conflict: boolean;
  /** Set by the layout pass: which of `columns` this block sits in. */
  column: number;
  columns: number;
};

const LINE = 'var(--border-width) solid var(--color-border)';
const dayHead: CSSProperties = {
  borderInlineStart: LINE,
  borderBlockEnd: LINE,
};
const dayBody = (hours: number): CSSProperties => ({
  position: 'relative',
  height: hours * HOUR_PX,
  borderInlineStart: LINE,
  backgroundImage: `repeating-linear-gradient(to bottom, var(--color-border) 0 1px, transparent 1px ${HOUR_PX}px)`,
});
const blockBase: CSSProperties = {
  position: 'absolute',
  borderRadius: 'var(--radius-inner)',
  overflow: 'hidden',
};
/** Colour, the hue edge and the conflict outline are paint; position is per block. */
function blockStyle(block: Block, gridStart: number): CSSProperties {
  const inset = 2;
  const width = `calc((100% - ${inset * 2}px) / ${block.columns})`;
  return {
    ...blockBase,
    top: ((block.start - gridStart) / 60) * HOUR_PX,
    height: Math.max(((block.end - block.start) / 60) * HOUR_PX, 20),
    left: `calc(${inset}px + ${width} * ${block.column})`,
    width,
    boxShadow: block.conflict
      ? `inset 0 0 0 2px var(--color-border-red), inset 3px 0 0 0 var(--color-border-${block.hue})`
      : `inset 3px 0 0 0 var(--color-border-${block.hue})`,
  };
}

/** Overlapping blocks share the column side by side, the way the artboard draws a conflict. */
function layoutDay(blocks: Block[]): Block[] {
  const sorted = [...blocks].sort((a, b) => a.start - b.start || a.end - b.end);
  let cluster: Block[] = [];
  let clusterEnd = -1;
  const out: Block[] = [];
  const flush = (): void => {
    const columnEnds: number[] = [];
    for (const b of cluster) {
      let col = columnEnds.findIndex(end => end <= b.start);
      if (col === -1) {
        col = columnEnds.length;
        columnEnds.push(b.end);
      } else {
        columnEnds[col] = b.end;
      }
      b.column = col;
    }
    for (const b of cluster) {
      b.columns = columnEnds.length;
      out.push(b);
    }
    cluster = [];
  };
  for (const b of sorted) {
    if (cluster.length > 0 && b.start >= clusterEnd) {
      flush();
    }
    cluster.push(b);
    clusterEnd = Math.max(clusterEnd, b.end);
  }
  if (cluster.length > 0) {
    flush();
  }
  return out;
}

/** Monday to Friday, with weekend columns only when a visible meeting uses them. */
export function WeekGrid({schedule, sectionsByCrn, conflicts}: WeekGridProps) {
  const clashDays = (crn: Crn): Set<string> =>
    new Set((conflicts.get(crn) ?? []).flatMap(c => c.days));
  const byDay = new Map<Day, Block[]>();
  let earliest = DEFAULT_START;
  let latest = DEFAULT_END;
  for (const candidate of schedule.candidates) {
    if (!candidate.visible) {
      continue;
    }
    for (const crn of candidate.sections) {
      const section = sectionsByCrn.get(crn);
      if (section === undefined) {
        continue;
      }
      timedMeetings(section.listing).forEach((time, i) => {
        earliest = Math.min(earliest, Math.floor(time.start / 60) * 60);
        latest = Math.max(latest, Math.ceil(time.end / 60) * 60);
        for (const day of time.days) {
          const list = byDay.get(day) ?? [];
          list.push({
            key: `${crn}-${i}-${day}`,
            crn,
            code: formatCourseCode(candidate.course),
            section: section.listing.section,
            start: time.start,
            end: time.end,
            hue: hueOf(candidate.colour),
            seats: section.seats,
            conflict: clashDays(crn).has(day),
            column: 0,
            columns: 1,
          });
          byDay.set(day, list);
        }
      });
    }
  }
  const days: Day[] = [
    ...WEEKDAYS,
    ...(['S', 'U'] as Day[]).filter(d => (byDay.get(d)?.length ?? 0) > 0),
  ];
  const hours: number[] = [];
  for (let m = earliest; m < latest; m += 60) {
    hours.push(m);
  }

  return (
    <Stack direction="horizontal" width="100%" gap={0} align="start">
      <Stack width={GUTTER_WIDTH} gap={0} style={{flexShrink: 0}}>
        <Stack width="100%" height={HEAD_HEIGHT} />
        {hours.map(m => (
          <Stack
            key={m}
            width="100%"
            height={HOUR_PX}
            hAlign="end"
            vAlign="start"
            paddingInlineEnd={1}
          >
            <Text type="supporting" hasTabularNumbers>
              {formatMinute(m)}
            </Text>
          </Stack>
        ))}
      </Stack>
      {days.map(day => (
        <Stack key={day} gap={0} style={{flex: 1, minWidth: 0}}>
          <Stack
            width="100%"
            height={HEAD_HEIGHT}
            hAlign="center"
            vAlign="center"
            style={dayHead}
          >
            <Text type="label" weight="medium">
              {DAY_NAME[day]}
            </Text>
          </Stack>
          <Stack width="100%" style={dayBody(hours.length)}>
            {layoutDay(byDay.get(day) ?? []).map(block => (
              <Card
                key={block.key}
                variant={block.hue}
                padding={1}
                style={blockStyle(block, earliest)}
              >
                <Stack gap={0} align="stretch" style={{minWidth: 0}}>
                  <Text size="sm" weight="semibold" hasTabularNumbers>
                    {block.code}
                  </Text>
                  <Text type="supporting" hasTabularNumbers>
                    {block.section} ·{' '}
                    {timeSpan({days: [], start: block.start, end: block.end})}
                  </Text>
                  {block.conflict && <Text type="supporting">⚠ conflicts</Text>}
                </Stack>
                {block.seats !== undefined &&
                  seatStatus(block.seats) !== 'open' && (
                    <StatusDot
                      {...seatsDot(block.seats)}
                      style={{
                        position: 'absolute',
                        top: 8,
                        right: 8,
                        boxShadow:
                          '0 0 0 2px var(--color-background-card), 0 1px 3px rgba(0,0,0,0.3)',
                      }}
                    />
                  )}
              </Card>
            ))}
          </Stack>
        </Stack>
      ))}
    </Stack>
  );
}
