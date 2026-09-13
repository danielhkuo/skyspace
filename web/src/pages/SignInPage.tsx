import {Button} from '@astryxdesign/core/Button';
import {CheckboxInput} from '@astryxdesign/core/CheckboxInput';
import {Divider} from '@astryxdesign/core/Divider';
import {Link} from '@astryxdesign/core/Link';
import {Section} from '@astryxdesign/core/Section';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {TextInput} from '@astryxdesign/core/TextInput';
import {useEffect, useState} from 'react';
import {useNavigate} from 'react-router';

import {dataSource} from '../datasource';
import {InvalidCodeError, RateLimitedError} from '../datasource/types';
import type {CourseCode, TermCode, TermSchedule} from '../domain';
import {rowDivider} from '../plan/paint';
import {useSession} from '../shell/useSession';

type Step = 'email' | 'code' | 'claim';

/** How long the resend link waits when the store names no wait of its own. */
const RESEND_WAIT_SECONDS = 48;

/**
 * Sign in with a Rice email and a mailed code, then claim what this browser
 * already holds (`Sign In and Claim`). No password field exists anywhere.
 * The code lives in component state and nowhere else. The demo sends no
 * mail: any six digits are accepted, and the page says so.
 */
export function SignInPage() {
  const {requestSignInCode, verifySignInCode} = useSession();
  const navigate = useNavigate();
  const [step, setStep] = useState<Step>('email');
  const [email, setEmail] = useState('');
  const [code, setCode] = useState('');
  const [error, setError] = useState<string | undefined>(undefined);
  const [codeError, setCodeError] = useState<string | undefined>(undefined);
  const [claimError, setClaimError] = useState<string | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const [favorites, setFavorites] = useState<CourseCode[]>([]);
  // Read at mount, while still signed out: after verify, loadSchedules
  // answers for the account, and the guest's own work would be lost.
  const [schedulesHeld, setSchedulesHeld] = useState<
    {schedules: TermSchedule[]; term: string; code: TermCode} | undefined
  >(undefined);
  const [keepFavorites, setKeepFavorites] = useState(true);
  const [resendIn, setResendIn] = useState(0);

  useEffect(() => {
    void dataSource.loadFavorites().then(setFavorites);
    void dataSource.currentTerm().then(async term => {
      const {schedules} = await dataSource.loadSchedules(term.code);
      setSchedulesHeld({schedules, term: term.label, code: term.code});
    });
  }, []);
  useEffect(() => {
    if (resendIn <= 0) {
      return undefined;
    }
    const t = setTimeout(() => setResendIn(n => n - 1), 1000);
    return () => clearTimeout(t);
  }, [resendIn]);

  const address = email.trim();
  const validEmail = /^[^\s@]+@rice\.edu$/i.test(address);

  /**
   * Ask for a code and open the code step. Being rate limited still opens
   * it: an earlier code may be in the inbox, and the countdown says when
   * another can be asked for.
   */
  const requestCode = (): void => {
    if (!validEmail) {
      setError('Enter a rice.edu address.');
      return;
    }
    setBusy(true);
    void requestSignInCode(address)
      .then(() => {
        setResendIn(RESEND_WAIT_SECONDS);
        setStep('code');
      })
      .catch((e: unknown) => {
        if (e instanceof RateLimitedError) {
          setResendIn(e.retryAfterSeconds ?? RESEND_WAIT_SECONDS);
          setStep('code');
        } else {
          setError('The code could not be sent. Try again.');
        }
      })
      .finally(() => setBusy(false));
  };

  const resendCode = (): void => {
    setBusy(true);
    setCodeError(undefined);
    void requestSignInCode(address)
      .then(() => setResendIn(RESEND_WAIT_SECONDS))
      .catch((e: unknown) => {
        if (e instanceof RateLimitedError) {
          setResendIn(e.retryAfterSeconds ?? RESEND_WAIT_SECONDS);
        } else {
          setCodeError('The code could not be sent. Try again.');
        }
      })
      .finally(() => setBusy(false));
  };

  /** The code is checked before the claim step, so a wrong one stays here. */
  const verifyCode = (): void => {
    if (code.length !== 6) {
      return;
    }
    setBusy(true);
    void verifySignInCode(address, code)
      .then(() => {
        setCode('');
        setStep('claim');
      })
      .catch((e: unknown) => {
        setCodeError(
          e instanceof InvalidCodeError
            ? e.message
            : 'The code could not be checked. Try again.',
        );
      })
      .finally(() => setBusy(false));
  };

  /** Move what this browser built onto the account, then leave. */
  const claim = (): void => {
    setBusy(true);
    setClaimError(undefined);
    void (async () => {
      await dataSource.claimGuestData({
        schedules: schedulesHeld?.schedules ?? [],
        collections: keepFavorites
          ? [{name: 'Favorites', courses: favorites}]
          : [],
      });
      void navigate('/catalog');
    })()
      .catch(() => {
        setClaimError(
          'Your favorites and schedules could not be moved. They are still in this browser; try again.',
        );
      })
      .finally(() => setBusy(false));
  };

  const favoritesCount = favorites.length;

  return (
    <Stack
      width="100%"
      height="100%"
      align="center"
      vAlign="center"
      padding={4}
    >
      <Section variant="section" padding={0} width={480} maxWidth="100%">
        <Stack width="100%" gap={4} padding={6} align="start">
          {step === 'email' && (
            <>
              <Text type="supporting">Step 1</Text>
              <Text weight="semibold" size="lg">
                Skyspace
              </Text>
              <Text as="h1" size="xl" weight="semibold">
                Sign in with your Rice email
              </Text>
              <TextInput
                label="Rice email"
                isLabelHidden
                type="email"
                size="md"
                value={email}
                onChange={v => {
                  setEmail(v);
                  setError(undefined);
                }}
                placeholder="netid@rice.edu"
                description="Any rice.edu address works, including aliases. We never ask for your NetID password."
                width="100%"
                status={
                  error === undefined
                    ? undefined
                    : {type: 'error', message: error}
                }
                onEnter={requestCode}
              />
              <Button
                label="Email me a code"
                variant="primary"
                size="md"
                width="100%"
                isLoading={busy}
                onClick={requestCode}
              />
              <Divider />
              <Text type="supporting">
                Or continue without an account: the catalog and plan work in
                this browser.
              </Text>
            </>
          )}

          {step === 'code' && (
            <>
              <Text type="supporting">Step 2</Text>
              <Text as="h1" size="xl" weight="semibold">
                Enter the six-digit code we sent to {address}
              </Text>
              {dataSource.kind === 'demo' && (
                <Text type="supporting">
                  Demo: no mail is sent. Any six digits work.
                </Text>
              )}
              <TextInput
                label="Six-digit code"
                isLabelHidden
                size="md"
                value={code}
                onChange={v => {
                  setCode(v.replace(/\D/g, '').slice(0, 6));
                  setCodeError(undefined);
                }}
                placeholder="000000"
                width="100%"
                hasAutoFocus
                status={
                  codeError === undefined
                    ? undefined
                    : {type: 'error', message: codeError}
                }
                onEnter={verifyCode}
              />
              <Button
                label="Continue"
                variant="primary"
                size="md"
                width="100%"
                isDisabled={code.length !== 6}
                isLoading={busy}
                onClick={verifyCode}
              />
              <Stack gap={1.5} align="start">
                {resendIn > 0 ? (
                  <Text type="supporting" hasTabularNumbers>
                    Resend in{' '}
                    {`${Math.floor(resendIn / 60)}:${String(resendIn % 60).padStart(2, '0')}`}
                  </Text>
                ) : (
                  <Link
                    href="#"
                    size="sm"
                    onClick={e => {
                      e.preventDefault();
                      if (!busy) {
                        resendCode();
                      }
                    }}
                  >
                    Send another code
                  </Link>
                )}
                <Link
                  href="#"
                  size="sm"
                  onClick={e => {
                    e.preventDefault();
                    setStep('email');
                    setCode('');
                    setCodeError(undefined);
                  }}
                >
                  Use a different email
                </Link>
              </Stack>
            </>
          )}

          {step === 'claim' && (
            <>
              <Text type="supporting">Step 3 · claim</Text>
              <Text as="h1" size="xl" weight="semibold">
                You built these before signing in
              </Text>
              <Section variant="section" padding={0} width="100%">
                <Stack width="100%" gap={0}>
                  <Stack
                    direction="horizontal"
                    width="100%"
                    padding={2}
                    gap={1.5}
                    vAlign="start"
                  >
                    <CheckboxInput
                      label="Keep these favorites"
                      isLabelHidden
                      size="sm"
                      value={keepFavorites}
                      onChange={setKeepFavorites}
                    />
                    <Stack gap={0} align="start">
                      <Text size="sm" weight="medium">
                        Favorites
                      </Text>
                      <Text type="supporting">
                        {favoritesCount} course{favoritesCount === 1 ? '' : 's'}
                      </Text>
                    </Stack>
                  </Stack>
                  <Stack
                    direction="horizontal"
                    width="100%"
                    padding={2}
                    gap={1.5}
                    vAlign="start"
                    style={rowDivider}
                  >
                    <Stack gap={0} align="start">
                      <Text size="sm" weight="medium">
                        Schedules
                      </Text>
                      <Text type="supporting">
                        {schedulesHeld === undefined
                          ? 'Counting…'
                          : `${schedulesHeld.schedules.length} for ${schedulesHeld.term}, kept as they are`}
                      </Text>
                    </Stack>
                  </Stack>
                  <Stack
                    direction="horizontal"
                    width="100%"
                    padding={2}
                    gap={1.5}
                    vAlign="start"
                    style={rowDivider}
                  >
                    <Stack gap={0} align="start">
                      <Text size="sm" weight="medium">
                        Plan
                      </Text>
                      <Text type="supporting">
                        Your plan in this browser stays as it is.
                      </Text>
                    </Stack>
                  </Stack>
                </Stack>
              </Section>
              {claimError !== undefined && (
                <Text
                  size="sm"
                  role="alert"
                  style={{color: 'var(--color-text-red)'}}
                >
                  {claimError}
                </Text>
              )}
              <Button
                label="Keep them on my account"
                variant="primary"
                size="md"
                width="100%"
                isLoading={busy}
                onClick={claim}
              />
              <Text type="supporting">
                Plans aren&apos;t created until you finish onboarding.
              </Text>
            </>
          )}
        </Stack>
      </Section>
    </Stack>
  );
}
