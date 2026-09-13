import {Button} from '@astryxdesign/core/Button';
import {CheckboxInput} from '@astryxdesign/core/CheckboxInput';
import {Collapsible} from '@astryxdesign/core/Collapsible';
import {Divider} from '@astryxdesign/core/Divider';
import {Icon} from '@astryxdesign/core/Icon';
import {IconButton} from '@astryxdesign/core/IconButton';

import {Stack, StackItem} from '@astryxdesign/core/Stack';
import {Switch} from '@astryxdesign/core/Switch';
import {Text} from '@astryxdesign/core/Text';
import {TextInput} from '@astryxdesign/core/TextInput';
import {ToggleButton, ToggleButtonGroup} from '@astryxdesign/core/ToggleButton';
import {Token} from '@astryxdesign/core/Token';
import {useState} from 'react';

import {
  ATTRIBUTE_LABEL,
  DAYS,
  formatMinute,
  type Attribute,
  type MinuteOfDay,
} from '../domain';
import {
  activeFilterCount,
  DEFAULT_QUERY,
  LEVELS,
  parseTimeText,
  type CatalogQuery,
} from './query';

export type QueryPatch = Partial<CatalogQuery>;

type FilterRailProps = {
  query: CatalogQuery;
  onChange: (patch: QueryPatch) => void;
  subjects: string[];
  /** `code` is what the query matches; `label` is what the rail shows. */
  partsOfTerm: {code: string; label: string}[];
  hiddenUnscheduled: number;
  /** The tablet sheet has its own search box above the results. */
  showSearch: boolean;
  size: 'sm' | 'md';
};

const DISTRIBUTION: Attribute[] = ['GRP1', 'GRP2', 'GRP3', 'AD'];

function toggleIn<T>(list: T[], item: T): T[] {
  return list.includes(item) ? list.filter(x => x !== item) : [...list, item];
}

/** Every filter from `features/catalog.md`, in the artboard's order. State lives in the URL; this only edits it. */
export function FilterRail({
  query,
  onChange,
  subjects,
  partsOfTerm,
  hiddenUnscheduled,
  showSearch,
  size,
}: FilterRailProps) {
  const [subjectFind, setSubjectFind] = useState('');
  const subjectMatches =
    subjectFind.trim() === ''
      ? []
      : subjects
          .filter(
            s =>
              s.startsWith(subjectFind.trim().toUpperCase()) &&
              !query.subject.includes(s),
          )
          .slice(0, 8);

  return (
    <Stack width="100%" gap={3} padding={3} align="stretch">
      <Stack
        direction="horizontal"
        width="100%"
        hAlign={showSearch ? 'between' : 'end'}
        vAlign="center"
      >
        {showSearch && (
          <Text as="h2" weight="semibold" size="lg">
            Filters
          </Text>
        )}
        <Button
          label="Clear all"
          variant="ghost"
          size="sm"
          isDisabled={activeFilterCount(query) === 0 && query.q === ''}
          onClick={() => onChange({...DEFAULT_QUERY})}
        />
      </Stack>
      {showSearch && (
        <>
          <SearchBox query={query} onChange={onChange} size={size} />
          <Divider />
        </>
      )}

      <Collapsible trigger="Subject" defaultIsOpen>
        <Stack gap={1.5} paddingBlock={1.5} align="start" width="100%">
          <TextInput
            label="Find a subject"
            isLabelHidden
            size="sm"
            value={subjectFind}
            onChange={setSubjectFind}
            placeholder="Find a subject"
            width="100%"
            onEnter={() => {
              const first = subjectMatches[0];
              if (first !== undefined) {
                onChange({subject: [...query.subject, first]});
                setSubjectFind('');
              }
            }}
          />
          {subjectMatches.length > 0 && (
            <Stack direction="horizontal" gap={1} wrap="wrap">
              {subjectMatches.map(s => (
                <Token
                  key={s}
                  label={s}
                  size="sm"
                  onClick={() => {
                    onChange({subject: [...query.subject, s]});
                    setSubjectFind('');
                  }}
                />
              ))}
            </Stack>
          )}
          {query.subject.length > 0 && (
            <Stack direction="horizontal" gap={1} wrap="wrap">
              {query.subject.map(s => (
                <Token
                  key={s}
                  label={s}
                  size="sm"
                  color="cyan"
                  onRemove={() =>
                    onChange({subject: query.subject.filter(x => x !== s)})
                  }
                />
              ))}
            </Stack>
          )}
        </Stack>
      </Collapsible>

      <Collapsible trigger="Distribution" defaultIsOpen>
        <Stack gap={1} paddingBlock={1.5} width="100%">
          {DISTRIBUTION.map(a => (
            <CheckboxInput
              key={a}
              label={ATTRIBUTE_LABEL[a].replace('Distribution ', '')}
              size="sm"
              value={query.attr.includes(a)}
              onChange={() => onChange({attr: toggleIn(query.attr, a)})}
              width="100%"
            />
          ))}
        </Stack>
      </Collapsible>

      <Collapsible trigger="Level" defaultIsOpen>
        <Stack gap={1} paddingBlock={1.5} align="start" width="100%">
          <ToggleButtonGroup
            type="multiple"
            size="sm"
            value={query.level.map(String)}
            onChange={selected =>
              onChange({
                level: LEVELS.filter(x => selected.includes(String(x))),
              })
            }
            label="Level"
          >
            {LEVELS.map(level => (
              <ToggleButton
                key={level}
                value={String(level)}
                label={level === 500 ? '500+' : String(level)}
              />
            ))}
          </ToggleButtonGroup>
        </Stack>
      </Collapsible>

      <Collapsible trigger="Days" defaultIsOpen>
        <Stack gap={1} paddingBlock={1.5} align="start" width="100%">
          <ToggleButtonGroup
            type="multiple"
            size="sm"
            value={query.days}
            onChange={selected =>
              onChange({
                days: DAYS.filter(x => selected.includes(x)),
              })
            }
            label="Days"
          >
            {DAYS.map(d => (
              <ToggleButton key={d} value={d} label={d} />
            ))}
          </ToggleButtonGroup>
        </Stack>
      </Collapsible>

      <Collapsible
        trigger="Time"
        defaultIsOpen={
          query.startsAfter !== undefined || query.endsBefore !== undefined
        }
      >
        <Stack gap={1.5} paddingBlock={1.5} width="100%">
          <TimeField
            key={`after-${query.startsAfter ?? ''}`}
            label="Starts after"
            placeholder="9:00 AM"
            value={query.startsAfter}
            onCommit={m => onChange({startsAfter: m})}
          />
          <TimeField
            key={`before-${query.endsBefore ?? ''}`}
            label="Ends before"
            placeholder="5:00 PM"
            value={query.endsBefore}
            onCommit={m => onChange({endsBefore: m})}
          />
        </Stack>
      </Collapsible>

      <Collapsible
        trigger="Credits"
        defaultIsOpen={
          query.creditsMin !== undefined || query.creditsMax !== undefined
        }
      >
        <Stack gap={1} paddingBlock={1.5} align="start" width="100%">
          <ToggleButtonGroup
            size="sm"
            value={
              query.creditsMin !== undefined &&
              query.creditsMin === query.creditsMax &&
              [100, 200, 300, 400].includes(query.creditsMin)
                ? String(query.creditsMin / 100)
                : null
            }
            onChange={val => {
              if (val === null) {
                onChange({creditsMin: undefined, creditsMax: undefined});
              } else {
                onChange({
                  creditsMin: Number(val) * 100,
                  creditsMax: Number(val) * 100,
                });
              }
            }}
            label="Credits"
          >
            {[1, 2, 3, 4].map(hours => (
              <ToggleButton
                key={hours}
                value={String(hours)}
                label={String(hours)}
              />
            ))}
          </ToggleButtonGroup>
        </Stack>
      </Collapsible>

      <Collapsible
        trigger="Part of term"
        defaultIsOpen={query.partOfTerm.length > 0}
      >
        <Stack gap={1} paddingBlock={1.5} width="100%">
          {partsOfTerm.map(pot => (
            <CheckboxInput
              key={pot.code}
              label={pot.label}
              size="sm"
              value={query.partOfTerm.includes(pot.code)}
              onChange={() =>
                onChange({partOfTerm: toggleIn(query.partOfTerm, pot.code)})
              }
              width="100%"
            />
          ))}
        </Stack>
      </Collapsible>

      <Divider />

      <Stack gap={2} width="100%" align="start">
        <Switch
          label="Open seats only"
          size="sm"
          value={query.openSeatsOnly}
          onChange={checked => onChange({openSeatsOnly: checked})}
          labelPosition="start"
          labelSpacing="spread"
          width="100%"
        />
        <Switch
          label="Hide unscheduled sections"
          size="sm"
          value={query.scheduledOnly}
          onChange={checked => onChange({scheduledOnly: checked})}
          labelPosition="start"
          labelSpacing="spread"
          width="100%"
        />
        {hiddenUnscheduled > 0 && (
          <Text type="supporting">
            Hidden: {hiddenUnscheduled} section
            {hiddenUnscheduled === 1 ? '' : 's'} with no meeting time.
          </Text>
        )}
      </Stack>
    </Stack>
  );
}

type SearchBoxProps = {
  query: CatalogQuery;
  onChange: (patch: QueryPatch) => void;
  size: 'sm' | 'md';
};

/** Search as you type, plus a copy of the shareable URL. */
export function SearchBox({query, onChange, size}: SearchBoxProps) {
  const [copied, setCopied] = useState(false);
  return (
    <Stack direction="horizontal" width="100%" gap={1} vAlign="start">
      <StackItem size="fill">
        <TextInput
          label="Search courses"
          isLabelHidden
          size={size}
          value={query.q}
          onChange={q => onChange({q})}
          placeholder="Course, title, instructor"
          startIcon="search"
          hasClear
          width="100%"
        />
      </StackItem>
      <IconButton
        label={copied ? 'Link copied' : 'Copy link to this search'}
        tooltip={copied ? 'Copied' : 'Copy link to this search'}
        variant="ghost"
        size={size}
        icon={<Icon icon={copied ? 'check' : 'copy'} size="sm" />}
        onClick={() => {
          void navigator.clipboard?.writeText(window.location.href).then(() => {
            setCopied(true);
            window.setTimeout(() => setCopied(false), 1500);
          });
        }}
      />
    </Stack>
  );
}

type TimeFieldProps = {
  label: string;
  placeholder: string;
  value: MinuteOfDay | undefined;
  onCommit: (minute: MinuteOfDay | undefined) => void;
};

/** Free text, committed on Enter or blur; an unreadable time is flagged, not applied. */
function TimeField({label, placeholder, value, onCommit}: TimeFieldProps) {
  const [text, setText] = useState(
    value === undefined ? '' : formatMinute(value),
  );
  const [bad, setBad] = useState(false);
  const commit = (): void => {
    if (text.trim() === '') {
      setBad(false);
      if (value !== undefined) {
        onCommit(undefined);
      }
      return;
    }
    const parsed = parseTimeText(text);
    setBad(parsed === undefined);
    if (parsed !== undefined && parsed !== value) {
      onCommit(parsed);
    }
  };
  return (
    <TextInput
      label={label}
      size="sm"
      value={text}
      onChange={t => {
        setText(t);
        setBad(false);
      }}
      onEnter={commit}
      onBlur={commit}
      placeholder={placeholder}
      width="100%"
      status={
        bad ? {type: 'error', message: 'Try 9:00 AM or 15:00'} : undefined
      }
    />
  );
}
