import {AlertDialog} from '@astryxdesign/core/AlertDialog';
import {Button} from '@astryxdesign/core/Button';
import {Divider} from '@astryxdesign/core/Divider';
import {Icon} from '@astryxdesign/core/Icon';
import {Link} from '@astryxdesign/core/Link';
import {Section} from '@astryxdesign/core/Section';
import {Stack, StackItem} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {useState} from 'react';
import {useNavigate} from 'react-router';

import {dataSource} from '../datasource';
import {announceSessionChange, useSession} from '../shell/useSession';

const NEVER = [
  'Your NetID password',
  'Your transcript or grades',
  'Anything from Esther',
];

/** Account on the left, privacy on the right (`Account and Privacy`). */
export function AccountPage() {
  const {session, signOut} = useSession();
  const navigate = useNavigate();
  const [confirming, setConfirming] = useState(false);
  const [deleting, setDeleting] = useState(false);

  return (
    <Stack
      direction="horizontal"
      width="100%"
      height="100%"
      gap={0}
      align="stretch"
    >
      <Section
        variant="section"
        dividers={['end']}
        padding={0}
        width="50%"
        height="100%"
      >
        <Stack width="100%" height="100%" isScrollable>
          <Stack width="100%" maxWidth={640} gap={4} padding={5} align="start">
            <Text as="h1" size="xl" weight="semibold">
              Account
            </Text>
            <Stack
              direction="horizontal"
              width="100%"
              gap={2}
              vAlign="center"
              hAlign="between"
            >
              <Text weight="medium">
                {session === undefined
                  ? '…'
                  : session === null
                    ? 'Not signed in'
                    : session.email}
              </Text>
              {session ? (
                <Button
                  label="Sign out"
                  variant="secondary"
                  size="sm"
                  onClick={() => {
                    void signOut().then(() => navigate('/catalog'));
                  }}
                />
              ) : (
                <Button
                  label="Sign in"
                  variant="secondary"
                  size="sm"
                  onClick={() => void navigate('/sign-in')}
                />
              )}
            </Stack>
            <Divider />
            <Stack width="100%" gap={2} align="start">
              <Text as="h2" size="lg" weight="semibold">
                Data
              </Text>
              <Stack width="100%" gap={0.5} align="start">
                <Button
                  label="Delete account"
                  variant="destructive"
                  size="sm"
                  onClick={() => setConfirming(true)}
                />
                <Text type="supporting">
                  Removes every plan, schedule and bookmark. Cannot be undone.
                </Text>
              </Stack>
              <Stack
                direction="horizontal"
                width="100%"
                gap={1.5}
                vAlign="center"
              >
                <Button
                  label="Export my data"
                  variant="secondary"
                  size="sm"
                  isDisabled
                />
                <Text type="supporting">coming later</Text>
              </Stack>
              <Link
                href="mailto:sugarlanddevs@gmail.com?subject=Skyspace%20feedback"
                size="sm"
              >
                Feedback
              </Link>
            </Stack>
            <Divider />
            <Stack width="100%" gap={1} align="start">
              <Text as="h2" size="lg" weight="semibold">
                What we never collect
              </Text>
              {NEVER.map(item => (
                <Stack
                  key={item}
                  direction="horizontal"
                  width="100%"
                  gap={1}
                  vAlign="center"
                >
                  <Icon
                    icon="close"
                    size="sm"
                    color="secondary"
                    label="Never collected"
                  />
                  <Text size="sm">{item}</Text>
                </Stack>
              ))}
            </Stack>
          </Stack>
        </Stack>
      </Section>
      <StackItem size="fill">
        <Stack width="100%" height="100%" align="center" isScrollable>
          <Stack width="100%" maxWidth={640} gap={3} padding={5} align="start">
            <Text as="h1" size="xl" weight="semibold">
              Privacy
            </Text>
            <Text color="secondary">
              Skyspace is a student project, not an official Rice service. This
              page says plainly what it keeps and what it never touches.
            </Text>
            <Text as="h2" size="lg" weight="semibold">
              What we store
            </Text>
            <Text>
              Your rice.edu email address, the plans and schedules you build,
              the courses you bookmark, and the self-checks and rule choices you
              record with their reasons. Nothing else is tied to you.
            </Text>
            <Text as="h2" size="lg" weight="semibold">
              What we never collect
            </Text>
            <Text>
              Your NetID password: we never ask for it and there is no field to
              type it into. Your transcript, your grades, or anything from
              Esther. Skyspace reads only public Rice pages.
            </Text>
            <Text as="h2" size="lg" weight="semibold">
              Where the course data comes from
            </Text>
            <Text>
              Course and section data comes from courses.rice.edu. Requirement
              rules come from ga.rice.edu, extracted and then reviewed by a
              person before they affect anyone&apos;s plan. Every seat count
              carries Rice&apos;s own timestamp.
            </Text>
            <Text as="h2" size="lg" weight="semibold">
              Anonymous counters
            </Text>
            <Text>
              We count page views and searches without identifiers, to know
              which subjects need better rule coverage. No third-party
              analytics, no advertising, no cross-site tracking.
            </Text>
            <Text as="h2" size="lg" weight="semibold">
              Deleting your account
            </Text>
            <Text>
              Delete account on this page removes every plan, schedule and
              bookmark immediately. It cannot be undone, and we keep no backup
              copy tied to your email.
            </Text>
          </Stack>
        </Stack>
      </StackItem>
      <AlertDialog
        isOpen={confirming}
        onOpenChange={setConfirming}
        title="Delete your account?"
        description="Every plan, favorite and self-check goes, immediately and for good. There is no backup."
        cancelLabel="Keep my account"
        actionLabel="Delete everything"
        actionVariant="destructive"
        isActionLoading={deleting}
        onAction={() => {
          setDeleting(true);
          void dataSource.deleteAccount().then(() => {
            announceSessionChange();
            setDeleting(false);
            setConfirming(false);
            window.location.assign('/catalog');
          });
        }}
      />
    </Stack>
  );
}
