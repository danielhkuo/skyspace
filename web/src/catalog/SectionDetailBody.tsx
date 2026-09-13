import {Banner} from '@astryxdesign/core/Banner';
import {Button} from '@astryxdesign/core/Button';
import {Card} from '@astryxdesign/core/Card';
import {Divider} from '@astryxdesign/core/Divider';
import {Icon} from '@astryxdesign/core/Icon';
import {IconButton} from '@astryxdesign/core/IconButton';
import {Link} from '@astryxdesign/core/Link';
import {Stack} from '@astryxdesign/core/Stack';
import {StatusDot} from '@astryxdesign/core/StatusDot';
import {Text} from '@astryxdesign/core/Text';
import {Token} from '@astryxdesign/core/Token';
import type {ReactNode} from 'react';

import {dataSource} from '../datasource';

import {
  ATTRIBUTE_LABEL,
  FINAL_EXAM_LABEL,
  formatAsOf,
  formatCreditRange,
  formatDateSpan,
  type PrereqFact,
  type Section,
} from '../domain';
import {StarFilledMark, StarMark} from './marks';
import {PrereqTokens} from './PrereqTokens';
import {seatsDot, seatsHeadline} from './labels';

type SectionDetailBodyProps = {
  section: Section;
  prereq: PrereqFact | undefined;
  isFavorite: boolean;
  onToggleFavorite: () => void;
  onAddToPlan: (() => void) | undefined;
  /** "Fall 2024" when the course is already on the board. */
  placedIn: string | undefined;
  /** Rendered after the description: the course's other sections and the fills card. */
  children?: ReactNode;
  /** Control size; the tablet artboard uses `md`. */
  size: 'sm' | 'md';
};

const RICE_COURSE =
  'https://courses.rice.edu/courses/!SWKSCAT.cat?p_action=COURSE';

/** Everything below a section's title in the pane. */
export function SectionDetailBody({
  section,
  prereq,
  isFavorite,
  onToggleFavorite,
  onAddToPlan,
  placedIn,
  children,
  size,
}: SectionDetailBodyProps) {
  const {listing, detail, seats} = section;
  const heading = (text: string): ReactNode => (
    <Text as="h3" type="label" weight="semibold">
      {text}
    </Text>
  );
  const dates = listing.meetings.find(m => m.dates !== undefined)?.dates;
  const distribution = (detail?.attributes ?? []).filter(a => a !== 'AD');
  const attributeTokens = [
    ...distribution.map(a => `${a} ${ATTRIBUTE_LABEL[a]}`),
    ...(detail?.attributes.includes('AD') ? ['AD Analyzing Diversity'] : []),
  ];

  return (
    <Stack width="100%" gap={3} align="start">
      <Stack direction="horizontal" width="100%" gap={1} wrap="wrap">
        <Token
          label={`${formatCreditRange(listing.credits)} credits`}
          size="sm"
        />
        {attributeTokens.map(label => (
          <Token key={label} label={label} size="sm" />
        ))}
        {detail?.gradeMode !== undefined && (
          <Token label={detail.gradeMode} size="sm" />
        )}
        {detail?.methodOfInstruction !== undefined && (
          <Token label={detail.methodOfInstruction} size="sm" />
        )}
        {detail?.courseType !== undefined && (
          <Token label={detail.courseType} size="sm" />
        )}
        {listing.partOfTerm !== undefined && (
          <Token
            label={
              dates === undefined
                ? listing.partOfTerm
                : `${listing.partOfTerm} · ${formatDateSpan(dates)}`
            }
            size="sm"
          />
        )}
        <Token label={FINAL_EXAM_LABEL[listing.finalExam]} size="sm" />
      </Stack>

      <Stack direction="horizontal" width="100%" gap={1} vAlign="center">
        <Button
          label="Add to schedule"
          variant="primary"
          size={size}
          isDisabled
          tooltip="The Schedule page is not built yet"
        />
        <Button
          label="Add to plan"
          variant="secondary"
          size={size}
          isDisabled={onAddToPlan === undefined}
          onClick={onAddToPlan}
        />
        <IconButton
          label={isFavorite ? 'Remove from favorites' : 'Add to favorites'}
          variant="ghost"
          size={size}
          icon={
            <Icon icon={isFavorite ? StarFilledMark : StarMark} size="sm" />
          }
          onClick={onToggleFavorite}
        />
      </Stack>
      {placedIn !== undefined && (
        <Text type="supporting">
          In your plan · {placedIn} ·{' '}
          <Link href="/plan" type="inherit">
            open
          </Link>
        </Text>
      )}

      <Card padding={3} width="100%">
        <Stack width="100%" gap={1} align="start">
          {seats === undefined ? (
            <>
              <Stack direction="horizontal" gap={1} vAlign="center">
                <StatusDot variant="neutral" label="Seats not polled" />
                <Text size="sm" color="secondary">
                  Seats are not polled for this section
                </Text>
              </Stack>
              <Text type="supporting">no numbers, no timestamp</Text>
            </>
          ) : (
            <>
              <Stack direction="horizontal" gap={1} vAlign="center">
                <StatusDot {...seatsDot(seats)} />
                <Text weight="medium" hasTabularNumbers>
                  {seatsHeadline(seats)}
                </Text>
              </Stack>
              {(detail?.reserved ?? []).map(r => (
                <Text key={r.label} size="sm">
                  Reserved: {r.capacity} for {r.label}, {r.available} available
                </Text>
              ))}
              <Text type="supporting">
                as of {formatAsOf(seats.asOf)} from Rice
              </Text>
            </>
          )}
        </Stack>
      </Card>

      <Stack width="100%" gap={1} align="start">
        {heading('Prerequisites')}
        <PrereqTokens fact={prereq} fallbackText={detail?.prerequisitesText} />
        <Text type="supporting">Rice checks these at registration.</Text>
      </Stack>

      {detail?.restrictions !== undefined && (
        <Stack width="100%" gap={1} align="start">
          {heading('Restrictions')}
          <Card variant="muted" padding={2} width="100%">
            <Text size="sm" color="secondary">
              {detail.restrictions}
            </Text>
          </Card>
        </Stack>
      )}

      {detail?.mutuallyExclusive !== undefined && (
        <Banner
          status="warning"
          container="card"
          collapsible={false}
          title={detail.mutuallyExclusive}
        />
      )}

      <Stack width="100%" gap={1} align="start">
        {heading('Description')}
        {detail !== undefined && detail.notes.length > 0 && (
          <Stack direction="horizontal" gap={1} wrap="wrap">
            {detail.notes.map(note => (
              <Token key={note} label={note.replace(/\.$/, '')} size="sm" />
            ))}
          </Stack>
        )}
        <Text size="sm" color="secondary">
          {detail?.description ??
            'Rice publishes no description for this section.'}
        </Text>
      </Stack>

      {children}

      <Divider />

      <Stack width="100%" gap={1} align="start">
        {dataSource.kind === 'demo' ? (
          <Text type="supporting">
            Demo data: this CRN is invented, so there is no Rice page for it. A
            real term and CRN together name a section; Rice reuses CRNs.
          </Text>
        ) : (
          <Link
            href={`${RICE_COURSE}&p_term=${listing.term}&p_crn=${listing.crn}`}
            size="sm"
            isExternalLink
          >
            Rice course page
          </Link>
        )}
        <Link href="https://esther.rice.edu/" size="sm" isExternalLink>
          Syllabus in Esther (sign-in required)
        </Link>
        <Link href="https://esther.rice.edu/" size="sm" isExternalLink>
          Evaluations in Esther (sign-in required)
        </Link>
      </Stack>
    </Stack>
  );
}
