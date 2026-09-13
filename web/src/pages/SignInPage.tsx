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
import {rowDivider} from '../plan/paint';
import {useSession} from '../shell/useSession';

type Step = 'email' | 'code' | 'claim';

/**
 * Sign in with a Rice email and a mailed code, then claim what this browser
 * already holds (`Sign In and Claim`). No password field exists anywhere.
 * The demo sends no mail: any six digits are accepted, and the page says so.
 */
export function SignInPage() {
  const {signIn} = useSession();
  const navigate = useNavigate();
  const [step, setStep] = useState<Step>('email');
  const [email, setEmail] = useState('');
  const [code, setCode] = useState('');
  const [error, setError] = useState<string | undefined>(undefined);
  const [favoritesCount, setFavoritesCount] = useState(0);
  const [schedulesHeld, setSchedulesHeld] = useState<
    {count: number; term: string} | undefined
  >(undefined);
  const [keepFavorites, setKeepFavorites] = useState(true);
  const [resendIn, setResendIn] = useState(0);

  useEffect(() => {
    void dataSource.loadFavorites().then(f => setFavoritesCount(f.length));
    void dataSource.currentTerm().then(async term => {
      const {schedules} = await dataSource.loadSchedules(term.code);
      setSchedulesHeld({count: schedules.length, term: term.label});
    });
  }, []);
  useEffect(() => {
    if (resendIn <= 0) {
      return undefined;
    }
    const t = setTimeout(() => setResendIn(n => n - 1), 1000);
    return () => clearTimeout(t);
  }, [resendIn]);

  const validEmail = /^[^\s@]+@rice\.edu$/i.test(email.trim());

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
                onEnter={() => {
                  if (validEmail) {
                    setStep('code');
                    setResendIn(48);
                  } else {
                    setError('Enter a rice.edu address.');
                  }
                }}
              />
              <Button
                label="Email me a code"
                variant="primary"
                size="md"
                width="100%"
                onClick={() => {
                  if (validEmail) {
                    setStep('code');
                    setResendIn(48);
                  } else {
                    setError('Enter a rice.edu address.');
                  }
                }}
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
                Enter the six-digit code we sent to {email.trim()}
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
                onChange={v => setCode(v.replace(/\D/g, '').slice(0, 6))}
                placeholder="000000"
                width="100%"
                hasAutoFocus
                onEnter={() => {
                  if (code.length === 6) {
                    setStep('claim');
                  }
                }}
              />
              <Button
                label="Continue"
                variant="primary"
                size="md"
                width="100%"
                isDisabled={code.length !== 6}
                onClick={() => setStep('claim')}
              />
              <Stack gap={1.5} align="start">
                <Text type="supporting" hasTabularNumbers>
                  {resendIn > 0
                    ? `Resend in 0:${String(resendIn).padStart(2, '0')}`
                    : 'You can request another code.'}
                </Text>
                <Link
                  href="#"
                  size="sm"
                  onClick={e => {
                    e.preventDefault();
                    setStep('email');
                    setCode('');
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
                          : `${schedulesHeld.count} for ${schedulesHeld.term}, kept as they are`}
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
              <Button
                label="Keep them on my account"
                variant="primary"
                size="md"
                width="100%"
                onClick={() => {
                  void (async () => {
                    if (!keepFavorites) {
                      await dataSource.saveFavorites([]);
                    }
                    await signIn(email.trim());
                    void navigate('/catalog');
                  })();
                }}
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
