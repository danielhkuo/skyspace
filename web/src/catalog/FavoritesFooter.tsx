import {Button} from '@astryxdesign/core/Button';
import {Icon} from '@astryxdesign/core/Icon';
import {Section} from '@astryxdesign/core/Section';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {Token} from '@astryxdesign/core/Token';
import {useState} from 'react';
import {useNavigate} from 'react-router';

import {courseKey, formatCourseCode, type CourseCode} from '../domain';
import {catalogSearchHref} from './labels';

type FavoritesFooterProps = {
  favorites: CourseCode[];
  onRemove: (code: CourseCode) => void;
};

/** Under the results: how many courses are starred, and the list on demand. One flat list, no collections. */
export function FavoritesFooter({favorites, onRemove}: FavoritesFooterProps) {
  const [open, setOpen] = useState(false);
  const navigate = useNavigate();
  const n = favorites.length;
  return (
    <Section
      variant="muted"
      dividers={['start']}
      paddingInline={3}
      paddingBlock={1.5}
      width="100%"
    >
      <Stack width="100%" gap={1.5}>
        <Stack
          direction="horizontal"
          width="100%"
          vAlign="center"
          hAlign="between"
        >
          <Stack direction="horizontal" gap={1} vAlign="center">
            <Text weight="medium" size="sm">
              Favorites
            </Text>
            <Text type="supporting">
              · {n} course{n === 1 ? '' : 's'} · saved in this browser
            </Text>
          </Stack>
          <Button
            label={open ? 'Collapse' : 'Expand'}
            variant="ghost"
            size="sm"
            isDisabled={n === 0}
            endContent={
              open ? undefined : <Icon icon="chevronDown" size="sm" />
            }
            onClick={() => setOpen(v => !v)}
          />
        </Stack>
        {open && n > 0 && (
          <Stack direction="horizontal" gap={1} wrap="wrap">
            {favorites.map(code => (
              <Token
                key={courseKey(code)}
                label={formatCourseCode(code)}
                size="sm"
                onClick={() => void navigate(catalogSearchHref(code))}
                onRemove={e => {
                  e.stopPropagation();
                  onRemove(code);
                }}
              />
            ))}
          </Stack>
        )}
      </Stack>
    </Section>
  );
}
