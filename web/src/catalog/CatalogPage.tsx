import {BottomSheet} from '@astryxdesign/core/BottomSheet';
import {Button} from '@astryxdesign/core/Button';
import {Divider} from '@astryxdesign/core/Divider';
import {DropdownMenu} from '@astryxdesign/core/DropdownMenu';
import {EmptyState} from '@astryxdesign/core/EmptyState';
import {Icon} from '@astryxdesign/core/Icon';
import {Link} from '@astryxdesign/core/Link';
import {Section} from '@astryxdesign/core/Section';
import {Layout, LayoutContent, LayoutPanel} from '@astryxdesign/core/Layout';
import {useResizable, ResizeHandle} from '@astryxdesign/core/Resizable';
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
import {LoadErrorCard} from '../shell/LoadErrorCard';
import {useSession} from '../shell/useSession';
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
import {useAddToSchedule} from '../schedule/useAddToSchedule';
import {useCatalogData} from './useCatalogData';
import {useFavorites} from './useFavorites';

const clipX: CSSProperties = {overflowX: 'hidden'};
const SORTS: SortKey[] = ['relevance', 'courseNumber', 'credits', 'openSeats'];

type UrlPatch = Partial<CatalogQuery> & {
  crn?: Crn | undefined;
  term?: TermCode;
};

type Results = {key: string; rows: CourseSection[]; page: SectionPage};

/** Find sections in a term: rail, results, pane. The whole search is in the URL. */
export function CatalogPage() {
  const {data, failed, retry, setPlan} = useCatalogData();
  const addToPlan = useAddToPlan(data?.bundle, setPlan);
  const addToSchedule = useAddToSchedule(data?.term.code);
  const favorites = useFavorites();
  const {session} = useSession();
  const [params, setParams] = useSearchParams();
  const desktop = useViewportWidth() >= DESKTOP_WIDTH;

  const filtersResizable = useResizable({
    defaultSize: 320,
    minSize: 320,
    maxSize: 320,
    collapsible: true,
  });

  const detailResizable = useResizable({
    defaultSize: 480,
    minSize: 400,
    maxSize: 640,
  });

  const [filtersOpenMobile, setFiltersOpenMobile] = useState(false);
  const {
    query: rawQuery,
    crn,
    term: urlTerm,
  } = useMemo(() => parseCatalogUrl(params), [params]);
  const queryKey = serializeCatalogUrl({query: rawQuery}).toString();
  // Stabilize the query object reference so that changing the CRN (which does not affect queryKey)
  // does not cause the search effect to fire again and issue redundant network requests.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const query = useMemo(() => rawQuery, [queryKey]);

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

  // No bundle (a guest): no report, no "fills in your plan", no "Add to plan".
  const bundle = data?.bundle;
  const report = useMemo(
    () => (bundle === undefined ? undefined : engine.evaluate(bundle)),
    [bundle],
  );
  const fills = useMemo(
    () =>
      bundle === undefined || report === undefined || selected === undefined
        ? undefined
        : summarizeFills(bundle, report, selected.listing.code),
    [bundle, report, selected],
  );

  const rail = (
    <FilterRail
      query={query}
      onChange={patch}
      subjects={subjects}
      partsOfTerm={partsOfTerm}
      hiddenUnscheduled={result?.page.unscheduledHidden ?? 0}
      showSearch={false}
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
      gap={2}
    >
      <Stack direction="horizontal" gap={2} vAlign="center">
        {desktop && (
          <Stack direction="horizontal" gap={1} vAlign="center">
            <Button
              label={
                activeFilterCount(query) === 0
                  ? 'Filters'
                  : `Filters (${activeFilterCount(query)})`
              }
              variant={filtersResizable.isCollapsed ? 'secondary' : 'secondary'}
              size="sm"
              icon={<Icon icon="funnel" size="sm" />}
              onClick={() => {
                if (filtersResizable.isCollapsed) {
                  filtersResizable.expand();
                } else {
                  filtersResizable.collapse();
                }
              }}
            />
            <div style={{width: 240}}>
              <SearchBox query={query} onChange={patch} size="sm" />
            </div>
          </Stack>
        )}
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
    crn === undefined || data === undefined ? undefined : selected ===
      undefined ? (
      <Stack width="100%" gap={3} padding={3} align="start">
        <Stack gap={1} width="100%">
          <Skeleton width="30%" height={20} />
          <Skeleton width="80%" height={32} />
        </Stack>
        <Skeleton width="100%" height={200} />
      </Stack>
    ) : (
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
            data.bundle?.prerequisites.find(p =>
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
          onAddToSchedule={
            addToSchedule.add === undefined ||
            selected.listing.term !== data.term.code
              ? undefined
              : () => addToSchedule.add?.(selected)
          }
          placedIn={fills?.placedIn}
          scheduledIn={addToSchedule.scheduledIn(selected)}
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

  if (failed) {
    return <LoadErrorCard what="the catalog" onRetry={retry} />;
  }

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
              onClick={() => setFiltersOpenMobile(true)}
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
          signedIn={session !== null && session !== undefined}
          favorites={favorites.favorites ?? []}
          onRemove={favorites.remove}
        />
        <BottomSheet
          isOpen={filtersOpenMobile}
          onOpenChange={setFiltersOpenMobile}
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
    <>
      <Layout
        start={
          <>
            <LayoutPanel
              resizable={filtersResizable.props}
              hasDivider
              padding={0}
            >
              {!filtersResizable.isCollapsed && rail}
            </LayoutPanel>
          </>
        }
        content={
          <LayoutContent isScrollable={false} padding={0}>
            <Stack width="100%" height="100%" gap={0}>
              {countsRow}
              <Divider />
              <StackItem size="fill" isScrollable>
                {resultsView}
                {loadMoreRow}
              </StackItem>
              <FavoritesFooter
                signedIn={session !== null && session !== undefined}
                favorites={favorites.favorites ?? []}
                onRemove={favorites.remove}
              />
            </Stack>
          </LayoutContent>
        }
        end={
          detail !== undefined ? (
            <>
              <ResizeHandle
                direction="horizontal"
                hasDivider
                resizable={detailResizable.props}
                label="Resize details"
                isReversed
              />
              <LayoutPanel
                resizable={detailResizable.props}
                hasDivider={false}
                padding={0}
                style={clipX}
              >
                {detail}
              </LayoutPanel>
            </>
          ) : undefined
        }
      />
      {addToPlan.dialog}
    </>
  );
}
