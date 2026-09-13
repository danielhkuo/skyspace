/**
 * Helpers shared by the adapters. The wire spells a core-type optional as
 * `?: T | null` (ts-rs pessimism) while the domain says `?: T`; these treat
 * null and undefined alike and never write an `undefined` key, so a
 * round trip reproduces the input key for key.
 */

/** `{key: value}` when present, `{}` otherwise, for spreading into a literal. */
export function opt<K extends string, T>(
  key: K,
  value: T | null | undefined,
): Partial<Record<K, T>> {
  if (value === null || value === undefined) {
    return {};
  }
  return {[key]: value} as Partial<Record<K, T>>;
}

/** `opt` with a conversion applied to a present value. */
export function optMap<K extends string, T, U>(
  key: K,
  value: T | null | undefined,
  convert: (value: T) => U,
): Partial<Record<K, U>> {
  if (value === null || value === undefined) {
    return {};
  }
  return {[key]: convert(value)} as Partial<Record<K, U>>;
}

/** For a `Record` rename table: an unknown wire value is a bug, never a silent passthrough. */
export function lookup<K extends string, V>(
  table: Record<K, V>,
  value: string,
  what: string,
): V {
  if (Object.prototype.hasOwnProperty.call(table, value)) {
    return table[value as K];
  }
  throw new Error(`unknown wire value for ${what}: ${JSON.stringify(value)}`);
}

/** Inverts a one-to-one rename table so the outbound direction cannot drift from the inbound one. */
export function invert<K extends string, V extends string>(
  table: Record<K, V>,
): Record<V, K> {
  const out = {} as Record<V, K>;
  for (const key of Object.keys(table) as K[]) {
    out[table[key]] = key;
  }
  return out;
}

/**
 * The `default` arm of a `switch` over a wire union: `never` keeps the
 * switch exhaustive at compile time, the throw catches a new server variant
 * at run time.
 */
export function unknownWire(what: string, value: never): never {
  throw new Error(
    `unknown wire value for ${what}: ${JSON.stringify(value as unknown)}`,
  );
}
