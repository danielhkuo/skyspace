/**
 * "Add to schedule" from the catalog: the term's current schedule (or a
 * fresh one), the same reducer the page runs, one save. The hook also says
 * where a section already is, so the pane can show it.
 */
import {useCallback, useEffect, useState} from 'react';

import {dataSource} from '../datasource';
import {
  findCandidate,
  type Section,
  type TermCode,
  type TermSchedule,
} from '../domain';
import {nextScheduleName} from './labels';
import {newSchedule, reduceSchedule} from './reduce';

export type AddToSchedule = {
  /** The current schedule's name when the section's course is listed there; a no-op until loaded. */
  scheduledIn: (section: Section) => string | undefined;
  add: ((section: Section) => void) | undefined;
};

export function useAddToSchedule(term: TermCode | undefined): AddToSchedule {
  const [current, setCurrent] = useState<TermSchedule | undefined>(undefined);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    if (term === undefined) {
      return undefined;
    }
    let live = true;
    void dataSource.loadSchedules(term).then(({schedules, current: id}) => {
      if (live) {
        setCurrent(schedules.find(s => s.id === id) ?? schedules[0]);
        setReady(true);
      }
    });
    return () => {
      live = false;
    };
  }, [term]);

  const scheduledIn = useCallback(
    (section: Section): string | undefined =>
      current !== undefined &&
      findCandidate(current, section.listing.code) !== undefined
        ? current.name
        : undefined,
    [current],
  );

  const add = useCallback(
    (section: Section) => {
      if (term === undefined) {
        return;
      }
      void (async () => {
        // Reload first: the schedule page may have saved since this page loaded.
        const {schedules, current: id} = await dataSource.loadSchedules(term);
        const base =
          schedules.find(s => s.id === id) ??
          schedules[0] ??
          newSchedule(term, nextScheduleName([]));
        const next = reduceSchedule(base, {
          type: 'addCandidate',
          course: section.listing.code,
          crn: section.listing.crn,
        });
        await dataSource.saveSchedule(next);
        await dataSource.setCurrentSchedule(term, next.id);
        setCurrent(next);
      })();
    },
    [term],
  );

  return {scheduledIn, add: ready ? add : undefined};
}
