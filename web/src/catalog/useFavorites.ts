import {useCallback, useEffect, useState} from 'react';

import {dataSource} from '../datasource';
import {courseKey, type CourseCode} from '../domain';

export type FavoritesState = {
  /** `undefined` until the first load answers. */
  favorites: CourseCode[] | undefined;
  isFavorite: (code: CourseCode) => boolean;
  toggle: (code: CourseCode) => void;
  remove: (code: CourseCode) => void;
};

/**
 * The student's starred courses, one flat list shared by the catalog, the
 * class page and the plan's tray. Saved through the data source on every
 * change; the first load is the only read.
 */
export function useFavorites(): FavoritesState {
  const [initial, setInitial] = useState<CourseCode[] | undefined>(undefined);
  const [favorites, setFavorites] = useState<CourseCode[] | undefined>(
    undefined,
  );

  useEffect(() => {
    let live = true;
    void dataSource.loadFavorites().then(loaded => {
      if (live) {
        setInitial(loaded);
        setFavorites(loaded);
      }
    });
    return () => {
      live = false;
    };
  }, []);

  useEffect(() => {
    if (favorites !== undefined && favorites !== initial) {
      void dataSource.saveFavorites(favorites);
    }
  }, [favorites, initial]);

  const isFavorite = useCallback(
    (code: CourseCode) =>
      favorites?.some(f => courseKey(f) === courseKey(code)) ?? false,
    [favorites],
  );

  const toggle = useCallback((code: CourseCode) => {
    setFavorites(prev => {
      if (prev === undefined) {
        return prev;
      }
      const key = courseKey(code);
      return prev.some(f => courseKey(f) === key)
        ? prev.filter(f => courseKey(f) !== key)
        : [...prev, code];
    });
  }, []);

  const remove = useCallback((code: CourseCode) => {
    const key = courseKey(code);
    setFavorites(prev => prev?.filter(f => courseKey(f) !== key));
  }, []);

  return {favorites, isFavorite, toggle, remove};
}
