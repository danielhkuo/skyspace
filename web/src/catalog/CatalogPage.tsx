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
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type CSSProperties,
} from 'react';
import {useSearchParams} from 'react-router';

import {dataSource} from '../datasource';
import {
  creditRangeMin,
  sameCourse,
  type CatalogQuery,
  type Crn,
  type Section as CourseSection,
  type SectionPage,
  type TermCode,
} from '../domain';
import {engine} from '../engine';
import {FavoritesFooter} from './FavoritesFooter';
import {FilterRail, SearchBox} from './FilterRail';
import {DESKTOP_WIDTH, useViewportWidth} from '../shell/useViewportWidth';
import {
  activeFilterCount,
  parseCatalogUrl,
  serializeCatalogUrl,
  SORT_LABEL,
} from './query';
import type {SortKey} from '../domain';
import {ResultsTable} from './ResultsTable';
import {SectionRows} from './SectionRows';
import {FillsCard} from './FillsCard';
import {summarizeFills} from './fills';
import {sectionIdentity} from './labels';
import {OtherSections} from './OtherSections';
import {SectionDetailBody} from './SectionDetailBody';
import {useAddToPlan} from './useAddToPlan';
import {useCatalogData} from './useCatalogData';
import {useFavorites} from './useFavorites';

const RAIL_WIDTH = 240;
const PANE_WIDTH = 480;
/** The pane scrolls down only; anything wider than it is a bug, not a scrollbar. */
const clipX: CSSProperties = {overflowX: 'hidden'};
/** Rail and pane keep their width; the results column is what gives. */
const fixedColumn: CSSProperties = {flexShrink: 0};
const givingColumn: CSSProperties = {minWidth: 0};
const SORTS: SortKey[] = ['relevance', 'courseNumber', 'credits', 'openSeats'];

type UrlPatch = Partial<CatalogQuery> & {
  crn?: Crn | undefined;
  term?: TermCode;
};

type Results = {key: string; rows: CourseSection[]; page: SectionPage};

/** Find sections in a term: rail, results, pane. The whole search is in the URL. */
export function CatalogPage() {
  const {data, setPlan} = useCatalogData();
  const addToPlan = useAddToPlan(data?.bundle, setPlan);
  const favorites = useFavorites();
  const [params, setParams] = useSearchParams();
  const desktop = useViewportWidth() >= DESKTOP_WIDTH;
  const [filtersOpen, setFiltersOpen] = useState(false);
  const {
    query,
    crn,
    term: urlTerm,
  } = useMemo(() => parseCatalogUrl(params), [params]);
  const queryKey = serializeCatalogUrl({query}).toString();

  const patch = useCallback(
    (change: UrlPatch, mode: 'replace' | 'push' = 'replace') => {
      const current = parseCatalogUrl(params);
      const {crn: nextCrn, term: nextTerm, ...queryChange} = change;
      const crnAfter = 'crn' in change ? nextCrn : current.crn;
      const next = serializeCatalogUrl({
        query: {...current.query, ...queryChange, offset: 0},
        // The term rides with the CRN and only with it.
        term: crnAfter === undefined ? undefined : (nextTerm ?? current.term),
        crn: crnAfter,
      });
      void setParams(next, {replace: mode === 'replace'});
    },
    [params, setParams],
  );

  // One page at a time, like the API. A new query starts over; "Load more" appends.
  const [results, setResults] = useState<Results | undefined>(undefined);
  const [loadingMore, setLoadingMore] = useState(false);
  // A shared link names its term; without one the catalog shows the current term.
  const term = urlTerm ?? data?.term.code;
  useEffect(() => {
    if (term === undefined) {
      return;
    }
    let live = true;
    void dataSource.searchSections(term, {...query, offset: 0}).then(page => {
      if (live) {
        setResults({key: queryKey, rows: page.rows, page});
      }
    });
    return () => {
      live = false;
    };
  }, [term, query, queryKey]);
  const loadMore = (): void => {
    if (term === undefined || results === undefined || loadingMore) {
      return;
    }
    setLoadingMore(true);
    void dataSource
      .searchSections(term, {...query, offset: results.rows.length})
      .then(page => {
        setResults(prev =>
          prev === undefined || prev.key !== queryKey
            ? prev
            : {...prev, rows: [...prev.rows, ...page.rows], page},
        );
      })
      .finally(() => setLoadingMore(false));
  };
  const result = results?.key === queryKey ? results : undefined;
  const rows = result?.rows ?? [];
  const subjects = data?.subjects ?? [];
  const partsOfTerm = data?.partsOfTerm ?? [];

  // The pane's section comes from its own fetch, so it opens even when the results do not hold it.
  const [pane, setPane] = useState<
    {crn: Crn; section: CourseSection; siblings: CourseSection[]} | undefined
  >(undefined);
  useEffect(() => {
    if (term === undefined || crn === undefined) {
      return;
    }
    let live = true;
    void dataSource.getSection(term, crn).then(async section => {
      if (section === undefined) {
        if (live) {
          setPane(undefined);
        }
        return;
      }
      const siblings = await dataSource.courseSections(
        term,
        section.listing.code,
      );
      if (live) {
        setPane({crn, section, siblings});
      }
    });
    return () => {
      live = false;
    };
  }, [term, crn]);
  const selected =
    crn !== undefined && pane?.crn === crn ? pane.section : undefined;
  const siblings = selected === undefined ? [] : (pane?.siblings ?? []);

  const select = (next: Crn): void =>
    patch({crn: next, term: data?.term.code}, 'push');

  const report = useMemo(
    () => (data === undefined ? undefined : engine.evaluate(data.bundle)),
    [data],
  );
  const fills = useMemo(
    () =>
      data === undefined || report === undefined || selected === undefined
        ? undefined
        : summarizeFills(data.bundle, report, selected.listing.code),
    [data, report, selected],
  );

  const rail = (
    <FilterRail
      query={query}
      onChange={patch}
      subjects={subjects}
      partsOfTerm={partsOfTerm}
      hiddenUnscheduled={result?.page.unscheduledHidden ?? 0}
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
        {result === undefined ? (
          <Skeleton width={120} height={20} />
        ) : (
          <>
            <Text weight="medium" size="sm">
              {result.page.total} section{result.page.total === 1 ? '' : 's'}
            </Text>
            <Text type="supporting">
              · {result.page.courseCount} course
              {result.page.courseCount === 1 ? '' : 's'}
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

  const resultsView =
    result === undefined ? (
      <Stack width="100%" gap={1.5} padding={2}>
        {[0, 1, 2, 3, 4, 5, 6, 7].map(i => (
          <Skeleton key={i} width="100%" height={20} index={i} />
        ))}
      </Stack>
    ) : rows.length === 0 ? (
      <EmptyState
        title="No sections match"
        description={
          result.page.unscheduledHidden > 0
            ? `${result.page.unscheduledHidden} unscheduled section${result.page.unscheduledHidden === 1 ? ' is' : 's are'} hidden. Show them, or remove a filter.`
            : 'Try removing a filter or searching for a code like COMP 140.'
        }
        actions={
          <Link
            href="#"
            size="sm"
            onClick={e => {
              e.preventDefault();
              patch({scheduledOnly: false});
            }}
          >
            Show unscheduled sections
          </Link>
        }
        isCompact
      />
    ) : desktop ? (
      <ResultsTable
        rows={rows}
        selected={crn}
        onSelect={select}
        isFavorite={favorites.isFavorite}
        onToggleFavorite={favorites.toggle}
      />
    ) : (
      <SectionRows
        rows={rows}
        selected={crn}
        onSelect={select}
        isFavorite={favorites.isFavorite}
        onToggleFavorite={favorites.toggle}
      />
    );

  const loadMoreRow =
    result !== undefined && result.page.hasMore ? (
      <Stack width="100%" padding={2} align="center">
        <Button
          label={`Load more (${result.page.total - rows.length} left)`}
          variant="secondary"
          size="sm"
          isLoading={loadingMore}
          onClick={loadMore}
        />
      </Stack>
    ) : null;

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
          placedIn={fills?.placedIn}
          size={desktop ? 'sm' : 'md'}
        >
          {siblings.length > 1 && (
            <Stack width="100%" gap={1} align="start">
              <Text as="h3" type="label" weight="semibold">
                All sections
              </Text>
              <OtherSections
                sections={siblings}
                current={selected.listing.crn}
                onSelect={select}
              />
            </Stack>
          )}
          {fills !== undefined && <FillsCard fills={fills} />}
        </SectionDetailBody>
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
          {resultsView}
          {loadMoreRow}
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
        style={fixedColumn}
      >
        <Stack width="100%" height="100%" isScrollable>
          {rail}
        </Stack>
      </Section>

      <StackItem size="fill" style={givingColumn}>
        <Stack width="100%" height="100%" gap={0}>
          {countsRow}
          <Divider />
          <StackItem size="fill" isScrollable>
            {resultsView}
            {loadMoreRow}
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
        style={fixedColumn}
      >
        <Stack width="100%" height="100%" isScrollable style={clipX}>
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
