import {Card} from '@astryxdesign/core/Card';
import {Collapsible} from '@astryxdesign/core/Collapsible';
import {Link} from '@astryxdesign/core/Link';
import {Icon} from '@astryxdesign/core/Icon';
import {Stack} from '@astryxdesign/core/Stack';
import {
  Table,
  TableBody,
  TableCell,
  TableHeader,
  TableHeaderCell,
  TableRow,
} from '@astryxdesign/core/Table';
import {Text} from '@astryxdesign/core/Text';
import type {CSSProperties} from 'react';

import type {EntryId, Plan, Warning} from '../domain';
import {
  isRecordedClaim,
  termName,
  warningEntry,
  warningSubject,
  warningTerm,
  warningText,
} from './labels';
import {HollowMark} from './marks';
import {islandHead, violetInk} from './paint';

type WarningsPanelProps = {
  plan: Plan;
  warnings: Warning[];
  /** Opens the card's Edit course dialog, the one place a warning gets resolved. */
  onEditCourse?: (entry: EntryId) => void;
};

const col = (px: number): CSSProperties => ({width: px, maxWidth: px});
const COLS = {mark: col(36), subject: col(120), where: col(120)};
const fixedLayout: CSSProperties = {tableLayout: 'fixed', width: '100%'};

function WarningRows({plan, warnings, onEditCourse}: WarningsPanelProps) {
  return (
    <Table
      density="compact"
      dividers="rows"
      textOverflow="wrap"
      style={fixedLayout}
    >
      <TableHeader>
        <TableRow isHeaderRow>
          <TableHeaderCell style={COLS.mark}>
            <Text type="supporting" size="xsm">
              {' '}
            </Text>
          </TableHeaderCell>
          <TableHeaderCell style={COLS.subject}>Course</TableHeaderCell>
          <TableHeaderCell style={COLS.where}>Where</TableHeaderCell>
          <TableHeaderCell>What it means</TableHeaderCell>
        </TableRow>
      </TableHeader>
      <TableBody>
        {warnings.map((warning, i) => {
          const term = warningTerm(warning);
          const self = warning.kind === 'selfCheck';
          const entry = warningEntry(warning);
          return (
            <TableRow key={i}>
              <TableCell>
                {self || isRecordedClaim(warning) ? (
                  <Icon
                    icon={HollowMark}
                    size="sm"
                    label={self ? 'Self-check' : 'On your say-so'}
                    style={violetInk}
                  />
                ) : (
                  <Icon
                    icon="warning"
                    size="sm"
                    color="warning"
                    label="Warning"
                  />
                )}
              </TableCell>
              <TableCell>
                <Text size="sm" weight="medium" textWrap="nowrap">
                  {warningSubject(warning, plan)}
                </Text>
              </TableCell>
              <TableCell>
                <Text type="supporting" textWrap="nowrap">
                  {self
                    ? 'check yourself'
                    : term === undefined
                      ? ''
                      : termName(plan, term)}
                </Text>
              </TableCell>
              <TableCell>
                <Text size="sm">
                  {warningText(warning, plan)}
                  {entry !== undefined && onEditCourse !== undefined && (
                    <>
                      {' '}
                      <Link
                        href="#"
                        size="sm"
                        onClick={e => {
                          e.preventDefault();
                          onEditCourse(entry);
                        }}
                      >
                        Edit course
                      </Link>
                    </>
                  )}
                </Text>
              </TableCell>
            </TableRow>
          );
        })}
      </TableBody>
    </Table>
  );
}

/**
 * Sits above the board: what, where, and what Rice will do about it, one row
 * each. Pins the student has already explained sit in a collapsed group with
 * a violet mark, so a resolved warning stays findable without shouting.
 */
export function WarningsPanel({
  plan,
  warnings,
  onEditCourse,
}: WarningsPanelProps) {
  const recorded = warnings.filter(isRecordedClaim);
  const active = warnings.filter(w => !isRecordedClaim(w));
  return (
    <Card padding={0} width="100%" variant="yellow">
      <Stack width="100%" gap={0}>
        <Stack
          direction="horizontal"
          width="100%"
          paddingInline={2}
          paddingBlock={1.5}
          gap={1.5}
          vAlign="center"
          style={islandHead}
        >
          <Icon icon="warning" size="sm" color="warning" label="" />
          <Text size="sm" weight="semibold">
            {active.length === 0
              ? 'No warnings'
              : `${active.length} warning${active.length === 1 ? '' : 's'}`}
          </Text>
        </Stack>
        {active.length > 0 && (
          <WarningRows
            plan={plan}
            warnings={active}
            onEditCourse={onEditCourse}
          />
        )}
        {recorded.length > 0 && (
          <Stack
            width="100%"
            paddingInline={2}
            paddingBlock={1}
            style={islandHead}
          >
            <Collapsible
              trigger={
                <Text size="sm" style={violetInk}>
                  {recorded.length} on your say-so
                </Text>
              }
              defaultIsOpen={false}
            >
              <WarningRows
                plan={plan}
                warnings={recorded}
                onEditCourse={onEditCourse}
              />
            </Collapsible>
          </Stack>
        )}
      </Stack>
    </Card>
  );
}
