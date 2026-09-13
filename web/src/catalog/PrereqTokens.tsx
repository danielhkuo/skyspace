import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {Token} from '@astryxdesign/core/Token';
import type {ReactNode} from 'react';
import {useNavigate} from 'react-router';

import {formatCourseCode, type PrereqExpr, type PrereqFact} from '../domain';
import {catalogSearchHref} from './labels';

type PrereqTokensProps = {
  /** From the course record; `undefined` when we hold no record. */
  fact: PrereqFact | undefined;
  /** The section page's simplified text, shown when there is no parsed record. */
  fallbackText: string | undefined;
};

/**
 * Rice's AND/OR expression as tokens, one level of parentheses for a group
 * nested inside a group, exactly as the artboard lays it out.
 */
export function PrereqTokens({fact, fallbackText}: PrereqTokensProps) {
  const navigate = useNavigate();

  if (fact === undefined || fact.kind === 'unknown') {
    return fallbackText === undefined || fallbackText === '' ? (
      <Text size="sm">No prerequisites listed by Rice</Text>
    ) : (
      <Text size="sm">{fallbackText}</Text>
    );
  }
  if (fact.kind === 'noneRequired') {
    return <Text size="sm">No prerequisites listed by Rice</Text>;
  }

  const parts: ReactNode[] = [];
  let seq = 0;
  const word = (text: string): void => {
    seq += 1;
    parts.push(
      <Text key={`w${seq}`} type="supporting">
        {text}
      </Text>,
    );
  };
  const walk = (expr: PrereqExpr, nested: boolean): void => {
    switch (expr.kind) {
      case 'course':
        seq += 1;
        parts.push(
          <Token
            key={`c${seq}`}
            label={formatCourseCode(expr.value)}
            size="md"
            onClick={() => void navigate(catalogSearchHref(expr.value))}
          />,
        );
        return;
      case 'unparsed':
        seq += 1;
        parts.push(
          <Text key={`u${seq}`} size="sm">
            {expr.value}
          </Text>,
        );
        return;
      case 'all':
      case 'any': {
        const joiner = expr.kind === 'all' ? 'AND' : 'OR';
        if (nested) {
          word('(');
        }
        expr.value.forEach((child, i) => {
          if (i > 0) {
            word(joiner);
          }
          walk(child, true);
        });
        if (nested) {
          word(')');
        }
      }
    }
  };
  walk(fact.value.expr, false);

  return (
    <Stack gap={1}>
      <Stack direction="horizontal" gap={1} wrap="wrap" vAlign="center">
        {parts}
      </Stack>
      {fact.value.corequisite !== undefined && (
        <Text size="sm">
          Corequisite: {formatCourseCode(fact.value.corequisite)}
        </Text>
      )}
    </Stack>
  );
}
