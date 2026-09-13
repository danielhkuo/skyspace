/**
 * Identifier newtypes, mirroring `skyspace-core`. All are UUID strings on the
 * wire. The brand stops a `TermId` from being passed where a `RequirementId` is wanted.
 */
declare const brand: unique symbol;
type Branded<T, Name extends string> = T & {readonly [brand]: Name};

export type PlanId = Branded<string, 'PlanId'>;
export type TermId = Branded<string, 'TermId'>;
export type EntryId = Branded<string, 'EntryId'>;
export type RequirementId = Branded<string, 'RequirementId'>;
export type ProgramId = Branded<string, 'ProgramId'>;
export type CollectionId = Branded<string, 'CollectionId'>;

/** The browser mints ids for unsaved cards (`04-planning.md`); the store mints the rest. */
export function newEntryId(): EntryId {
  return crypto.randomUUID() as EntryId;
}

export function newTermId(): TermId {
  return crypto.randomUUID() as TermId;
}
