import {useCallback, useEffect, useState} from 'react';

import {dataSource} from '../datasource';
import {NoPlanError, UnauthenticatedError} from '../datasource/types';
import type {Plan, PlanBundle, TermCode} from '../domain';

export type CatalogData = {
  term: {code: TermCode; label: string};
  subjects: string[];
  partsOfTerm: {code: string; label: string}[];
  /**
   * For prerequisites, exclusions and "fills in your plan". A guest has no
   * plan, so no bundle: the catalog still searches, without those.
   */
  bundle: PlanBundle | undefined;
};

export type CatalogState = {
  /** `undefined` while the load is in flight. */
  data: CatalogData | undefined;
  failed: boolean;
  retry: () => void;
  /** After "Add to plan" saved: keep the page's copy of the plan current. */
  setPlan: (plan: Plan) => void;
};

/** One load per mount. */
export function useCatalogData(): CatalogState {
  const [data, setData] = useState<CatalogData | undefined>(undefined);
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const retry = useCallback(() => {
    setFailed(false);
    setAttempt(n => n + 1);
  }, []);
  const setPlan = useCallback((plan: Plan) => {
    setData(prev =>
      prev?.bundle === undefined
        ? prev
        : {...prev, bundle: {...prev.bundle, plan}},
    );
  }, []);
  useEffect(() => {
    let live = true;
    void (async () => {
      const term = await dataSource.currentTerm();
      const [subjects, partsOfTerm, bundle] = await Promise.all([
        dataSource.listSubjects(term.code),
        dataSource.listPartsOfTerm(term.code),
        dataSource.loadBundle().catch((error: unknown) => {
          // Signed out, or signed in with no plan yet: the catalog still works.
          if (
            error instanceof UnauthenticatedError ||
            error instanceof NoPlanError
          ) {
            return undefined;
          }
          throw error;
        }),
      ]);
      if (live) {
        setData({term, subjects, partsOfTerm, bundle});
      }
    })().catch(() => {
      if (live) {
        setFailed(true);
      }
    });
    return () => {
      live = false;
    };
  }, [attempt]);
  return {data, failed, retry, setPlan};
}
