import {useCallback, useEffect, useState} from 'react';

import {dataSource} from '../datasource';
import type {Session} from '../datasource/types';

const EVENT = 'skyspace:session';

/** Tell every mounted `useSession` that the session changed. */
export function announceSessionChange(): void {
  window.dispatchEvent(new Event(EVENT));
}

export type SessionState = {
  /** `undefined` while loading, `null` when signed out. */
  session: Session | null | undefined;
  /** Mails a code; rejects with `RateLimitedError` when asked too often. */
  requestSignInCode: (email: string) => Promise<void>;
  /** Trades the code for a session; rejects with `InvalidCodeError`. */
  verifySignInCode: (email: string, code: string) => Promise<Session>;
  signOut: () => Promise<void>;
};

export function useSession(): SessionState {
  const [session, setSession] = useState<Session | null | undefined>(undefined);
  useEffect(() => {
    let live = true;
    const load = (): void => {
      void dataSource.session().then(s => {
        if (live) {
          setSession(s ?? null);
        }
      });
    };
    load();
    window.addEventListener(EVENT, load);
    return () => {
      live = false;
      window.removeEventListener(EVENT, load);
    };
  }, []);
  const requestSignInCode = useCallback(
    (email: string) => dataSource.requestSignInCode(email),
    [],
  );
  const verifySignInCode = useCallback(async (email: string, code: string) => {
    const s = await dataSource.verifySignInCode(email, code);
    announceSessionChange();
    return s;
  }, []);
  const signOut = useCallback(async () => {
    await dataSource.signOut();
    announceSessionChange();
  }, []);
  return {session, requestSignInCode, verifySignInCode, signOut};
}
