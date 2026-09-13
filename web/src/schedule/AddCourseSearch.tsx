import {Item} from '@astryxdesign/core/Item';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {TextInput} from '@astryxdesign/core/TextInput';
import {useEffect, useState} from 'react';

import {dataSource} from '../datasource';
import {
  courseKey,
  DEFAULT_QUERY,
  formatCourseCode,
  formatCreditRange,
  type CourseCode,
  type Crn,
  type Section,
  type TermCode,
  type TermSchedule,
} from '../domain';

const COURSE_LIMIT = 8;

type AddCourseSearchProps = {
  term: TermCode;
  schedule: TermSchedule;
  onAdd: (course: CourseCode, crn: Crn) => void;
};

type Hit = {course: CourseCode; title: string; first: Section; count: number};

/** The catalog's own search, grouped by course; a pick adds the course with its first section picked. */
export function AddCourseSearch({term, schedule, onAdd}: AddCourseSearchProps) {
  const [query, setQuery] = useState('');
  const [found, setFound] = useState<{query: string; hits: Hit[]}>({
    query: '',
    hits: [],
  });
  // Results belong to the query that fetched them; a cleared box shows none without a state write.
  const hits = found.query === query ? found.hits : [];

  useEffect(() => {
    if (query.trim() === '') {
      return undefined;
    }
    let live = true;
    void dataSource
      .searchSections(term, {
        ...DEFAULT_QUERY,
        q: query,
        scheduledOnly: false,
        limit: 40,
      })
      .then(page => {
        if (!live) {
          return;
        }
        const byCourse = new Map<string, Hit>();
        for (const row of page.rows) {
          const key = courseKey(row.listing.code);
          const hit = byCourse.get(key);
          if (hit === undefined) {
            byCourse.set(key, {
              course: row.listing.code,
              title: row.listing.title,
              first: row,
              count: 1,
            });
          } else {
            hit.count += 1;
          }
        }
        setFound({query, hits: [...byCourse.values()].slice(0, COURSE_LIMIT)});
      });
    return () => {
      live = false;
    };
  }, [term, query]);

  const pick = (hit: Hit): void => {
    onAdd(hit.course, hit.first.listing.crn);
    setQuery('');
  };
  const already = (course: CourseCode): boolean =>
    schedule.candidates.some(c => courseKey(c.course) === courseKey(course));

  return (
    <Stack width="100%" gap={1.5} paddingBlock={3} align="start">
      <TextInput
        label="Add course"
        isLabelHidden
        size="sm"
        value={query}
        onChange={setQuery}
        placeholder="Add course"
        startIcon="search"
        hasClear
        width="100%"
        onEnter={() => {
          const first = hits.find(h => !already(h.course));
          if (first !== undefined) {
            pick(first);
          }
        }}
      />
      {hits.length > 0 && (
        <Stack gap={0} width="100%">
          {hits.map(hit => (
            <Item
              key={courseKey(hit.course)}
              label={formatCourseCode(hit.course)}
              description={hit.title}
              density="compact"
              layout="inline"
              endContent={
                <Text type="supporting" hasTabularNumbers textWrap="nowrap">
                  {already(hit.course)
                    ? 'already listed'
                    : `${formatCreditRange(hit.first.listing.credits)} · ${hit.count} section${hit.count === 1 ? '' : 's'}`}
                </Text>
              }
              isDisabled={already(hit.course)}
              onClick={() => pick(hit)}
            />
          ))}
        </Stack>
      )}
      {query.trim() !== '' && hits.length === 0 && (
        <Text type="supporting">Nothing this term matches.</Text>
      )}
      <Text type="supporting">
        Unlimited candidates. Hide a course to test a combination.
      </Text>
    </Stack>
  );
}
