import {BottomSheet} from '@astryxdesign/core/BottomSheet';
import {Button} from '@astryxdesign/core/Button';
import {Divider} from '@astryxdesign/core/Divider';
import {DropdownMenu} from '@astryxdesign/core/DropdownMenu';
import {EmptyState} from '@astryxdesign/core/EmptyState';
import {Icon} from '@astryxdesign/core/Icon';
import {Link} from '@astryxdesign/core/Link';
import {Section} from '@astryxdesign/core/Section';
import {Skeleton} from '@astryxdesign/core/Skeleton';
import {Stack, StackItem} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {useCallback, useMemo, useState} from 'react';
import {useSearchParams} from 'react-router';

import {
  creditRangeMin,
  sameCourse,
  type Crn,
  type Section as CourseSection,
} from '../domain';
import {FavoritesFooter} from './FavoritesFooter';
import {FilterRail, SearchBox, type QueryPatch} from './FilterRail';
import {DESKTOP_WIDTH, useViewportWidth} from '../shell/useViewportWidth';
import {
  activeFilterCount,
  applyQuery,
  parseQuery,
  partsOfTermIn,
  serializeQuery,
  SORT_LABEL,
  subjectsIn,
  type SortKey,
} from './query';
import {ResultsTable} from './ResultsTable';
import {SectionRows} from './SectionRows';
import {placedEntry} from './fills';
import {classHref, sectionIdentity} from './labels';
import {SectionDetailBody} from './SectionDetailBody';
import {useAddToPlan} from './useAddToPlan';
import {useCatalogData} from './useCatalogData';
import {useFavorites} from './useFavorites';

const RAIL_WIDTH = 240;
const PANE_WIDTH = 420;
const SORTS: SortKey[] = ['relevance', 'code', 'credits', 'openSeats'];

/** Find sections in a term: rail, results, pane. The whole search is in the URL. */
export function CatalogPage() {
  const {data, setPlan} = useCatalogData();
  const addToPlan = useAddToPlan(data?.bundle, setPlan);
  const favorites = useFavorites();
  const [params, setParams] = useSearchParams();
  const desktop = useViewportWidth() >= DESKTOP_WIDTH;
  const [filtersOpen, setFiltersOpen] = useState(false);
  const query = useMemo(() => parseQuery(params), [params]);

  const patch = useCallback(
    (change: QueryPatch, mode: 'replace' | 'push' = 'replace') => {
      const next = serializeQuery({...parseQuery(params), ...change});
      void setParams(next, {replace: mode === 'replace'});
    },
    [params, setParams],
  );

  const sections = useMemo(() => data?.sections ?? [], [data]);
  const result = useMemo(() => applyQuery(sections, query), [sections, query]);
  const subjects = useMemo(() => subjectsIn(sections), [sections]);
  const partsOfTerm = useMemo(() => partsOfTermIn(sections), [sections]);

  const selected: CourseSection | undefined =
    query.crn === undefined
      ? undefined
      : sections.find(s => s.listing.crn === query.crn);

  const select = (crn: Crn): void => patch({crn}, 'push');

  const rail = (
    <FilterRail
      query={query}
      onChange={patch}
      subjects={subjects}
      partsOfTerm={partsOfTerm}
      hiddenUnscheduled={result.hiddenUnscheduled}
      showSearch={desktop}
      size={desktop ? 'sm' : 'md'}
    />
  );

  const countsRow = (
    <Stack
      direction="horizontal"
      width="100%"
      vAlign="center"
      hAlign="between"
      paddingInline={3}
      paddingBlock={1.5}
    >
      <Stack direction="horizontal" gap={1} vAlign="center">
        {data === undefined ? (
          <Skeleton width={120} height={20} />
        ) : (
          <>
            <Text weight="medium" size="sm">
              {result.rows.length} section{result.rows.length === 1 ? '' : 's'}
            </Text>
            <Text type="supporting">
              · {result.courseCount} course{result.courseCount === 1 ? '' : 's'}
            </Text>
          </>
        )}
      </Stack>
      <Stack direction="horizontal" gap={1} vAlign="center">
        <Text type="supporting">Sort</Text>
        <DropdownMenu
          hasChevron
          button={{label: SORT_LABEL[query.sort], variant: 'ghost', size: 'sm'}}
          items={SORTS.map(sort => ({
            id: sort,
            label: SORT_LABEL[sort],
            onClick: () => patch({sort}),
          }))}
        />
      </Stack>
    </Stack>
  );

  const results =
    data === undefined ? (
      <Stack width="100%" gap={1.5} padding={2}>
        {[0, 1, 2, 3, 4, 5, 6, 7].map(i => (
          <Skeleton key={i} width="100%" height={20} index={i} />
        ))}
      </Stack>
    ) : result.rows.length === 0 ? (
      <EmptyState
        title="No sections match"
        description={
          result.hiddenUnscheduled > 0
            ? `${result.hiddenUnscheduled} unscheduled section${result.hiddenUnscheduled === 1 ? ' is' : 's are'} hidden. Show them, or remove a filter.`
            : 'Try removing a filter or searching for a code like COMP 140.'
        }
        actions={
          <Link
            href="#"
            size="sm"
            onClick={e => {
              e.preventDefault();
              patch({showUnscheduled: true});
            }}
          >
            Show unscheduled sections
          </Link>
        }
        isCompact
      />
    ) : desktop ? (
      <ResultsTable
        rows={result.rows}
        selected={query.crn}
        onSelect={select}
        isFavorite={favorites.isFavorite}
        onToggleFavorite={favorites.toggle}
      />
    ) : (
      <SectionRows
        rows={result.rows}
        selected={query.crn}
        onSelect={select}
        isFavorite={favorites.isFavorite}
        onToggleFavorite={favorites.toggle}
      />
    );

  const detail =
    selected === undefined || data === undefined ? undefined : (
      <Stack width="100%" gap={3} padding={3} align="start">
        <Stack
          direction="horizontal"
          width="100%"
          hAlign="between"
          vAlign="start"
          gap={2}
        >
          <Stack gap={0.5}>
            <Text type="supporting" hasTabularNumbers>
              {sectionIdentity(selected)}
            </Text>
            <Text as="h2" size="xl" weight="semibold">
              {selected.listing.title}
            </Text>
          </Stack>
          <Link href={classHref(selected)} size="sm">
            Open as page
          </Link>
        </Stack>
        <SectionDetailBody
          section={selected}
          prereq={
            data.bundle.prerequisites.find(p =>
              sameCourse(p.course, selected.listing.code),
            )?.fact
          }
          isFavorite={favorites.isFavorite(selected.listing.code)}
          onToggleFavorite={() => favorites.toggle(selected.listing.code)}
          onAddToPlan={
            addToPlan.open === undefined
              ? undefined
              : () =>
                  addToPlan.open?.(
                    selected.listing.code,
                    creditRangeMin(selected.listing.credits),
                  )
          }
          placedIn={placedEntry(data.bundle, selected.listing.code)?.term}
          headings="pane"
          size={desktop ? 'sm' : 'md'}
        />
      </Stack>
    );

  if (!desktop) {
    const filters = activeFilterCount(query);
    return (
      <Stack width="100%" height="100%" gap={0}>
        <Section
          variant="section"
          dividers={['end']}
          paddingInline={2}
          paddingBlock={1.5}
          width="100%"
        >
          <Stack direction="horizontal" width="100%" gap={1.5} vAlign="center">
            <StackItem size="fill">
              <SearchBox query={query} onChange={patch} size="md" />
            </StackItem>
            <Button
              label={filters === 0 ? 'Filters' : `Filters (${filters})`}
              variant="secondary"
              size="md"
              icon={<Icon icon="funnel" size="sm" />}
              onClick={() => setFiltersOpen(true)}
            />
          </Stack>
        </Section>
        {countsRow}
        <Divider />
        <StackItem size="fill" isScrollable>
          {results}
        </StackItem>
        <FavoritesFooter
          favorites={favorites.favorites ?? []}
          onRemove={favorites.remove}
        />
        <BottomSheet
          isOpen={filtersOpen}
          onOpenChange={setFiltersOpen}
          label="Filters"
          height="tall"
        >
          {rail}
        </BottomSheet>
        <BottomSheet
          isOpen={detail !== undefined}
          onOpenChange={open => {
            if (!open) {
              patch({crn: undefined});
            }
          }}
          label="Section details"
          height="tall"
        >
          <Stack
            direction="horizontal"
            width="100%"
            paddingInline={2}
            paddingBlock={1}
          >
            <Button
              label="Results"
              variant="ghost"
              size="md"
              icon={<Icon icon="chevronLeft" size="sm" />}
              onClick={() => patch({crn: undefined})}
            />
          </Stack>
          {detail}
        </BottomSheet>
        {addToPlan.dialog}
      </Stack>
    );
  }

  return (
    <Stack
      direction="horizontal"
      width="100%"
      height="100%"
      gap={0}
      align="stretch"
    >
      <Section
        variant="section"
        dividers={['end']}
        padding={0}
        width={RAIL_WIDTH}
        height="100%"
      >
        <Stack width="100%" height="100%" isScrollable>
          {rail}
        </Stack>
      </Section>

      <StackItem size="fill">
        <Stack width="100%" height="100%" gap={0}>
          {countsRow}
          <Divider />
          <StackItem size="fill" isScrollable>
            {results}
          </StackItem>
          <FavoritesFooter
            favorites={favorites.favorites ?? []}
            onRemove={favorites.remove}
          />
        </Stack>
      </StackItem>

      <Section
        variant="section"
        dividers={['start']}
        padding={0}
        width={PANE_WIDTH}
        height="100%"
      >
        <Stack width="100%" height="100%" isScrollable>
          {detail ?? (
            <Stack width="100%" height="100%" vAlign="center" padding={3}>
              <EmptyState
                title="Pick a section"
                description="Its seats, prerequisites and description open here."
                isCompact
              />
            </Stack>
          )}
        </Stack>
      </Section>
      {addToPlan.dialog}
    </Stack>
  );
}
