import {Dialog} from '@astryxdesign/core/Dialog';
import {Item} from '@astryxdesign/core/Item';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {TextInput} from '@astryxdesign/core/TextInput';
import {useEffect, useState} from 'react';

import {
  creditRangeMin,
  formatCourseCode,
  formatCreditRange,
  type CourseInfo,
  type Credits,
  type PlanBundle,
  type CourseCode,
} from '../domain';
import {dataSource} from '../datasource';
import {termName} from './labels';
import {duplicateOf} from './useBoardDrag';

const RESULT_LIMIT = 12;

type AddCourseDialogProps = {
  isOpen: boolean;
  /** Where the course lands; shown in the title. */
  termLabel: string;
  onClose: () => void;
  onPick: (course: CourseCode, credits: Credits) => void;
  bundle: PlanBundle;
};

/** The same search as the catalog, dropping its pick into one term. */
export function AddCourseDialog({
  isOpen,
  termLabel,
  onClose,
  onPick,
  bundle,
}: AddCourseDialogProps) {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<CourseInfo[]>([]);

  useEffect(() => {
    let cancelled = false;
    void dataSource.searchCourses(query, RESULT_LIMIT).then(found => {
      if (!cancelled) {
        setResults(found);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [query]);

  const close = (): void => {
    setQuery('');
    onClose();
  };

  const pick = (info: CourseInfo): void => {
    if (duplicateOf(bundle, info.code) !== undefined) {
      return;
    }
    onPick(info.code, creditRangeMin(info.credits));
    close();
  };

  return (
    <Dialog
      isOpen={isOpen}
      onOpenChange={open => {
        if (!open) {
          close();
        }
      }}
      width={480}
      padding={3}
    >
      <Stack gap={2} width="100%">
        <Text as="p" size="lg" weight="semibold">
          Add a course to {termLabel}
        </Text>
        <TextInput
          label="Search courses"
          isLabelHidden
          placeholder="Course code or title"
          value={query}
          onChange={setQuery}
          startIcon="search"
          hasClear
          hasAutoFocus
          width="100%"
          onEnter={() => {
            const first = results[0];
            if (first !== undefined) {
              pick(first);
            }
          }}
        />
        <Stack gap={0} width="100%">
          {results.map(info => (
            <Item
              key={formatCourseCode(info.code)}
              label={formatCourseCode(info.code)}
              description={info.title}
              density="compact"
              layout="inline"
              endContent={
                <Text size="sm" color="secondary" hasTabularNumbers>
                  {(() => {
                    const already = duplicateOf(bundle, info.code);
                    return already === undefined
                      ? formatCreditRange(info.credits)
                      : `already in ${already === 'incoming' ? 'incoming credit' : termName(bundle.plan, already)}`;
                  })()}
                </Text>
              }
              isDisabled={duplicateOf(bundle, info.code) !== undefined}
              onClick={() => pick(info)}
            />
          ))}
          {query.trim() !== '' && results.length === 0 && (
            <Text type="supporting">Nothing in the demo catalog matches.</Text>
          )}
          {query.trim() === '' && (
            <Text type="supporting">
              Type a code like{' '}
              <Text type="inherit" weight="medium">
                comp 4
              </Text>{' '}
              or a word from a title.
            </Text>
          )}
        </Stack>
      </Stack>
    </Dialog>
  );
}
