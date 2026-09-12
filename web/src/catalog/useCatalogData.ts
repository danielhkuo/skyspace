import {useCallback, useEffect, useState} from 'react';

import {dataSource} from '../datasource';
import type {Plan, PlanBundle, Section, TermCode} from '../domain';

export type CatalogData = {
  term: {code: TermCode; label: string};
  sections: Section[];
  /** For prerequisites, exclusions and "fills in your plan". */
  bundle: PlanBundle;
};

export type CatalogState = {
  /** `undefined` while the load is in flight. */
  data: CatalogData | undefined;
  /** After "Add to plan" saved: keep the page's copy of the plan current. */
  setPlan: (plan: Plan) => void;
};

/** One load per mount. */
export function useCatalogData(): CatalogState {
  const [data, setData] = useState<CatalogData | undefined>(undefined);
  const setPlan = useCallback((plan: Plan) => {
    setData(prev =>
      prev === undefined ? prev : {...prev, bundle: {...prev.bundle, plan}},
    );
  }, []);
  useEffect(() => {
    let live = true;
    void (async () => {
      const term = await dataSource.currentTerm();
      const [sections, bundle] = await Promise.all([
        dataSource.listSections(term.code),
        dataSource.loadBundle(),
      ]);
      if (live) {
        setData({term, sections, bundle});
      }
    })();
    return () => {
      live = false;
    };
  }, []);
  return {data, setPlan};
}
