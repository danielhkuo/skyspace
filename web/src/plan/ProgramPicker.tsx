import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {TextInput} from '@astryxdesign/core/TextInput';
import {Token} from '@astryxdesign/core/Token';
import {useState} from 'react';

import type {Program, ProgramId, ProgramKind} from '../domain';

type ProgramPickerProps = {
  label: string;
  isOptional?: boolean;
  placeholder: string;
  /** Which kinds this picker offers: majors, or minors and certificates. */
  kinds: ProgramKind[];
  available: Program[];
  chosen: ProgramId[];
  onChange: (next: ProgramId[]) => void;
  /** Fewer than this many is refused, with the reason on the remove button. */
  minimum?: number;
  minimumReason?: string;
};

/** Search-and-token picker for majors or minors (`Plan 7 Settings and Onboarding`). */
export function ProgramPicker({
  label,
  isOptional,
  placeholder,
  kinds,
  available,
  chosen,
  onChange,
  minimum = 0,
  minimumReason,
}: ProgramPickerProps) {
  const atMinimum = chosen.length <= minimum;
  const [query, setQuery] = useState('');
  const pool = available.filter(p => kinds.includes(p.kind));
  const q = query.trim().toLowerCase();
  const matches =
    q === ''
      ? []
      : pool
          .filter(
            p =>
              !chosen.includes(p.id) &&
              `${p.name} ${p.credential} ${p.slug}`.toLowerCase().includes(q),
          )
          .slice(0, 6);
  const pick = (id: ProgramId): void => {
    onChange([...chosen, id]);
    setQuery('');
  };
  return (
    <Stack width="100%" gap={1} align="start">
      <TextInput
        label={label}
        isOptional={isOptional}
        size="sm"
        value={query}
        onChange={setQuery}
        placeholder={placeholder}
        startIcon="search"
        width="100%"
        onEnter={() => {
          const first = matches[0];
          if (first !== undefined) {
            pick(first.id);
          }
        }}
      />
      {matches.length > 0 && (
        <Stack direction="horizontal" gap={1} wrap="wrap">
          {matches.map(p => (
            <Token
              key={p.id}
              label={`${p.name} (${p.credential})`}
              size="sm"
              onClick={() => pick(p.id)}
            />
          ))}
        </Stack>
      )}
      {q !== '' && matches.length === 0 && (
        <Text type="supporting">
          No {kinds.includes('major') ? 'major' : 'minor'} matches. The demo
          knows {pool.length} program{pool.length === 1 ? '' : 's'}; Rice has
          351.
        </Text>
      )}
      <Stack direction="horizontal" gap={1} wrap="wrap">
        {chosen
          .map(id => pool.find(p => p.id === id))
          .filter((p): p is Program => p !== undefined)
          .map(p => (
            <Token
              key={p.id}
              label={`${p.name} (${p.credential})`}
              size="sm"
              color="blue"
              description={atMinimum ? minimumReason : undefined}
              onRemove={
                atMinimum
                  ? undefined
                  : () => onChange(chosen.filter(id => id !== p.id))
              }
            />
          ))}
      </Stack>
    </Stack>
  );
}
