/**
 * The term's schedules and the one on screen. Every edit reduces, sets state
 * and saves the changed document at once: localStorage is synchronous in the
 * demo, and an API source would queue the same way the plan saver does.
 */
import {useCallback, useEffect, useMemo, useState} from 'react';

import {dataSource} from '../datasource';
import {
  courseKey,
  type Crn,
  type ScheduleId,
  type ScheduledMeeting,
  type Section,
  type TermCode,
  type TermSchedule,
} from '../domain';
import {engine} from '../engine';
import {nextScheduleName} from './labels';
import {newSchedule, reduceSchedule, type ScheduleAction} from './reduce';

type Loaded = {schedules: TermSchedule[]; currentId: ScheduleId};

export type ScheduleState = {
  /** `undefined` while loading. */
  schedules: TermSchedule[] | undefined;
  current: TermSchedule | undefined;
  failed: boolean;
  retry: () => void;
  /** Every section of every candidate's course, for the pickers and the grid. */
  sectionsByCourse: ReadonlyMap<string, Section[]>;
  sectionsByCrn: ReadonlyMap<Crn, Section>;
  /** CRN → the courses it clashes with, by day: `Map<'12423', [{code:'COMP 222', days:['R']}]>`. */
  conflicts: ReadonlyMap<Crn, {crn: Crn; days: string[]}[]>;
  dispatch: (action: ScheduleAction) => void;
  switchTo: (id: ScheduleId) => void;
  create: () => void;
  deleteCurrent: () => void;
};

export function useSchedule(term: TermCode | undefined): ScheduleState {
  const [loaded, setLoaded] = useState<Loaded | undefined>(undefined);
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    if (term === undefined) {
      return undefined;
    }
    let live = true;
    void dataSource
      .loadSchedules(term)
      .then(async ({schedules, current}) => {
        if (!live) {
          return;
        }
        if (schedules.length === 0) {
          const fresh = await dataSource.saveSchedule(
            newSchedule(term, nextScheduleName([])),
          );
          if (live) {
            setLoaded({schedules: [fresh], currentId: fresh.id});
          }
          return;
        }
        const first = schedules[0];
        setLoaded({
          schedules,
          currentId: current ?? (first as TermSchedule).id,
        });
      })
      .catch(() => {
        if (live) {
          setFailed(true);
        }
      });
    return () => {
      live = false;
    };
  }, [term, attempt]);

  const current = loaded?.schedules.find(s => s.id === loaded.currentId);

  // The pickers need every section of each course; the grid needs the picked ones. One map serves both.
  const courseKeys = (current?.candidates ?? [])
    .map(c => courseKey(c.course))
    .sort()
    .join('|');
  const [sectionsByCourse, setSectionsByCourse] = useState<
    Map<string, Section[]>
  >(new Map());
  useEffect(() => {
    if (term === undefined || current === undefined) {
      return undefined;
    }
    let live = true;
    const missing = current.candidates.filter(
      c => !sectionsByCourse.has(courseKey(c.course)),
    );
    if (missing.length === 0) {
      return undefined;
    }
    void Promise.all(
      missing.map(async c => {
        const rows = await dataSource.courseSections(term, c.course);
        return [courseKey(c.course), rows] as const;
      }),
    ).then(found => {
      if (live) {
        setSectionsByCourse(prev => new Map([...prev, ...found]));
      }
    });
    return () => {
      live = false;
    };
    // `courseKeys` is the candidate set; the map only ever grows.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [term, courseKeys]);

  const sectionsByCrn = useMemo(() => {
    const map = new Map<Crn, Section>();
    for (const rows of sectionsByCourse.values()) {
      for (const s of rows) {
        map.set(s.listing.crn, s);
      }
    }
    return map;
  }, [sectionsByCourse]);

  const conflicts = useMemo(() => {
    const out = new Map<Crn, {crn: Crn; days: string[]}[]>();
    if (current === undefined) {
      return out;
    }
    const meetings: ScheduledMeeting[] = [];
    for (const candidate of current.candidates) {
      if (!candidate.visible) {
        continue;
      }
      for (const crn of candidate.sections) {
        const section = sectionsByCrn.get(crn);
        for (const meeting of section?.listing.meetings ?? []) {
          meetings.push({
            owner: {kind: 'section', value: crn},
            meeting,
            ...(section?.listing.partOfTerm === undefined
              ? {}
              : {partOfTerm: section.listing.partOfTerm}),
          });
        }
      }
    }
    for (const c of engine.scheduleConflicts(meetings)) {
      if (c.a.kind !== 'section' || c.b.kind !== 'section') {
        continue;
      }
      const add = (from: Crn, to: Crn): void => {
        const list = out.get(from) ?? [];
        const hit = list.find(x => x.crn === to);
        if (hit === undefined) {
          list.push({crn: to, days: [...c.days]});
        } else {
          for (const d of c.days) {
            if (!hit.days.includes(d)) {
              hit.days.push(d);
            }
          }
        }
        out.set(from, list);
      };
      add(c.a.value, c.b.value);
      add(c.b.value, c.a.value);
    }
    return out;
  }, [current, sectionsByCrn]);

  /**
   * State first, then the write. The store may mint its own id for a new
   * schedule; when it does, the document is re-keyed in place. A save that
   * keeps the id leaves state alone: a later edit may already be on screen.
   */
  const save = useCallback(
    (next: Loaded, changed: TermSchedule): Promise<TermSchedule> => {
      setLoaded(next);
      return dataSource.saveSchedule(changed).then(persisted => {
        if (persisted.id !== changed.id) {
          setLoaded(prev =>
            prev === undefined
              ? prev
              : {
                  schedules: prev.schedules.map(s =>
                    s.id === changed.id ? persisted : s,
                  ),
                  currentId:
                    prev.currentId === changed.id
                      ? persisted.id
                      : prev.currentId,
                },
          );
        }
        return persisted;
      });
    },
    [],
  );

  const dispatch = useCallback(
    (action: ScheduleAction) => {
      if (loaded === undefined || current === undefined) {
        return;
      }
      const changed = reduceSchedule(current, action);
      void save(
        {
          ...loaded,
          schedules: loaded.schedules.map(s =>
            s.id === changed.id ? changed : s,
          ),
        },
        changed,
      );
    },
    [loaded, current, save],
  );

  const switchTo = useCallback(
    (id: ScheduleId) => {
      if (loaded === undefined || term === undefined) {
        return;
      }
      setLoaded({...loaded, currentId: id});
      void dataSource.setCurrentSchedule(term, id);
    },
    [loaded, term],
  );

  const create = useCallback(() => {
    if (loaded === undefined || term === undefined) {
      return;
    }
    const fresh = newSchedule(
      term,
      nextScheduleName(loaded.schedules.map(s => s.name)),
    );
    void save(
      {schedules: [...loaded.schedules, fresh], currentId: fresh.id},
      fresh,
    ).then(persisted => dataSource.setCurrentSchedule(term, persisted.id));
  }, [loaded, term, save]);

  const deleteCurrent = useCallback(() => {
    if (loaded === undefined || current === undefined || term === undefined) {
      return;
    }
    const rest = loaded.schedules.filter(s => s.id !== current.id);
    void dataSource.deleteSchedule(current.id);
    const next = rest[0];
    if (next === undefined) {
      // The page always shows a schedule; an empty one replaces the last.
      const fresh = newSchedule(term, nextScheduleName([]));
      void save({schedules: [fresh], currentId: fresh.id}, fresh).then(
        persisted => dataSource.setCurrentSchedule(term, persisted.id),
      );
      return;
    }
    setLoaded({schedules: rest, currentId: next.id});
    void dataSource.setCurrentSchedule(term, next.id);
  }, [loaded, current, term, save]);

  const retry = useCallback(() => {
    setFailed(false);
    setAttempt(n => n + 1);
  }, []);

  return {
    schedules: loaded?.schedules,
    current,
    failed,
    retry,
    sectionsByCourse,
    sectionsByCrn,
    conflicts,
    dispatch,
    switchTo,
    create,
    deleteCurrent,
  };
}
