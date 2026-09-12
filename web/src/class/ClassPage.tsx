import {BreadcrumbItem, Breadcrumbs} from '@astryxdesign/core/Breadcrumbs';
import {Card} from '@astryxdesign/core/Card';
import {EmptyState} from '@astryxdesign/core/EmptyState';
import {Link} from '@astryxdesign/core/Link';
import {Skeleton} from '@astryxdesign/core/Skeleton';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {Token} from '@astryxdesign/core/Token';
import {useMemo} from 'react';
import {useNavigate, useParams} from 'react-router';

import {ResultsTable} from '../catalog/ResultsTable';
import {SectionDetailBody} from '../catalog/SectionDetailBody';
import {classHref, sectionIdentity} from '../catalog/labels';
import {useAddToPlan} from '../catalog/useAddToPlan';
import {useCatalogData} from '../catalog/useCatalogData';
import {useFavorites} from '../catalog/useFavorites';
import {creditRangeMin, formatCourseCode, sameCourse} from '../domain';
import {engine} from '../engine';
import {summarizeFills, type FillEntry} from '../catalog/fills';

const PAGE_WIDTH = 960;

/** One section as its own page: `/class/:term/:crn`. Everything Rice publishes, then what the student does next. */
export function ClassPage() {
  const {term, crn} = useParams();
  const {data, setPlan} = useCatalogData();
  const addToPlan = useAddToPlan(data?.bundle, setPlan);
  const favorites = useFavorites();
  const navigate = useNavigate();

  const section = data?.sections.find(
    s => s.listing.crn === crn && s.listing.term === term,
  );
  const report = useMemo(
    () => (data === undefined ? undefined : engine.evaluate(data.bundle)),
    [data],
  );
  const fills = useMemo(
    () =>
      data === undefined || report === undefined || section === undefined
        ? undefined
        : summarizeFills(data.bundle, report, section.listing.code),
    [data, report, section],
  );

  if (data === undefined) {
    return (
      <Stack width="100%" align="center" paddingBlock={4}>
        <Stack width={PAGE_WIDTH} maxWidth="100%" gap={1.5} paddingInline={6}>
          {[0, 1, 2, 3, 4].map(i => (
            <Skeleton key={i} width="100%" height={20} index={i} />
          ))}
        </Stack>
      </Stack>
    );
  }

  if (section === undefined || fills === undefined) {
    return (
      <Stack width="100%" align="center" paddingBlock={6}>
        <EmptyState
          title={`No section with CRN ${crn ?? ''} in ${data.term.label}`}
          description="It may have been cancelled, or the link points at another term."
          actions={
            <Link href="/catalog" size="sm">
              Back to the catalog
            </Link>
          }
        />
      </Stack>
    );
  }

  const code = section.listing.code;
  const siblings = data.sections.filter(s => sameCourse(s.listing.code, code));
  const prereq = data.bundle.prerequisites.find(p =>
    sameCourse(p.course, code),
  )?.fact;

  return (
    <Stack width="100%" height="100%" align="center" isScrollable>
      <Stack
        width={PAGE_WIDTH}
        maxWidth="100%"
        gap={5}
        paddingInline={6}
        paddingBlock={4}
        align="start"
      >
        <Breadcrumbs variant="supporting" separator="›">
          <BreadcrumbItem href="/catalog">Catalog</BreadcrumbItem>
          <BreadcrumbItem isCurrent>{formatCourseCode(code)}</BreadcrumbItem>
        </Breadcrumbs>

        <Stack gap={1} align="start">
          <Text type="supporting" hasTabularNumbers>
            {sectionIdentity(section)}
          </Text>
          <Text as="h1" size="2xl" weight="semibold">
            {section.listing.title}
          </Text>
        </Stack>

        <SectionDetailBody
          section={section}
          prereq={prereq}
          isFavorite={favorites.isFavorite(code)}
          onToggleFavorite={() => favorites.toggle(code)}
          onAddToPlan={
            addToPlan.open === undefined
              ? undefined
              : () =>
                  addToPlan.open?.(
                    code,
                    creditRangeMin(section.listing.credits),
                  )
          }
          placedIn={fills.placedIn}
          headings="page"
          size="sm"
        >
          <Stack width="100%" gap={2} align="start">
            <Text as="h2" size="lg" weight="semibold">
              All sections
            </Text>
            <ResultsTable
              rows={siblings}
              selected={section.listing.crn}
              onSelect={next => {
                const target = siblings.find(s => s.listing.crn === next);
                if (target !== undefined) {
                  void navigate(classHref(target));
                }
              }}
              isFavorite={favorites.isFavorite}
              onToggleFavorite={favorites.toggle}
            />
          </Stack>

          <Card variant="muted" padding={3} width="100%">
            <Stack width="100%" gap={1} align="start">
              <Text type="label" weight="semibold">
                Fills in your plan
              </Text>
              <FillsLine
                planName={fills.planName}
                placedIn={fills.placedIn}
                entries={
                  fills.placedIn === undefined ? fills.candidates : fills.fills
                }
              />
            </Stack>
          </Card>
        </SectionDetailBody>
      </Stack>
      {addToPlan.dialog}
    </Stack>
  );
}

type FillsLineProps = {
  planName: string;
  placedIn: string | undefined;
  entries: FillEntry[];
};

function FillsLine({planName, placedIn, entries}: FillsLineProps) {
  const tokens = entries.map((e, i) => (
    <Stack
      key={`${e.program}-${e.path}`}
      direction="horizontal"
      gap={1}
      vAlign="center"
    >
      {i > 0 && <Text size="sm">and</Text>}
      <Token label={e.path} size="sm" description={e.program} />
    </Stack>
  ));
  return (
    <Stack direction="horizontal" gap={1} wrap="wrap" vAlign="center">
      <Text size="sm">
        In{' '}
        <Text type="inherit" weight="semibold">
          {planName}
        </Text>
        {placedIn === undefined
          ? entries.length === 0
            ? ', this course fills no requirement.'
            : ', this course could fill'
          : entries.length === 0
            ? `, this course is in ${placedIn} and fills no requirement.`
            : `, this course is in ${placedIn} and fills`}
      </Text>
      {tokens}
      <Link href="/plan" size="sm">
        {placedIn === undefined ? 'open plan' : 'change'}
      </Link>
    </Stack>
  );
}
