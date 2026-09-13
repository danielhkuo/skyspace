import {AlertDialog} from '@astryxdesign/core/AlertDialog';
import {AppShell} from '@astryxdesign/core/AppShell';
import {Avatar} from '@astryxdesign/core/Avatar';
import {Banner} from '@astryxdesign/core/Banner';
import {DropdownMenu} from '@astryxdesign/core/DropdownMenu';
import {Button} from '@astryxdesign/core/Button';
import {Icon} from '@astryxdesign/core/Icon';
import {Stack, StackItem} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {TopNav, TopNavItem} from '@astryxdesign/core/TopNav';
import {useEffect, useState, useSyncExternalStore} from 'react';
import {Outlet, useLocation, useNavigate} from 'react-router';

import {dataSource} from '../datasource';

import {DemoBanner} from './DemoBanner';
import {NarrowViewportNotice} from './NarrowViewportNotice';
import {getLeaveGuard, setLeaveGuard, subscribeLeaveGuard} from './leaveGuard';
import {useSession} from './useSession';
import {MIN_SUPPORTED_WIDTH, useViewportWidth} from './useViewportWidth';

const NAV = [
  {label: 'Catalog', href: '/catalog'},
  {label: 'Schedule', href: '/schedule'},
  {label: 'Plan', href: '/plan'},
] as const;

/** The frame every page shares: top bar, then the routed page. */
export function Shell() {
  const width = useViewportWidth();
  const {pathname} = useLocation();
  const navigate = useNavigate();
  const {session, signOut} = useSession();
  const [stale, setStale] = useState<{since?: string} | undefined>(undefined);
  const guard = useSyncExternalStore(
    subscribeLeaveGuard,
    getLeaveGuard,
    getLeaveGuard,
  );
  const [pendingHref, setPendingHref] = useState<string | undefined>(undefined);
  const go = (href: string): void => {
    if (guard !== undefined && !pathname.startsWith(href)) {
      setPendingHref(href);
    } else {
      void navigate(href);
    }
  };
  useEffect(() => {
    void dataSource
      .freshness()
      .then(f => setStale(f.stale ? {since: f.staleSince} : undefined))
      .catch(() => setStale(undefined));
  }, []);

  if (width < MIN_SUPPORTED_WIDTH) {
    return <NarrowViewportNotice />;
  }

  return (
    <AppShell
      variant="section"
      contentPadding={0}
      mobileNav={false}
      topNav={
        <TopNav
          label="Skyspace navigation"
          heading={
            <Text weight="semibold" size="lg">
              Skyspace
            </Text>
          }
          centerContent={
            <Stack direction="horizontal" gap={1} vAlign="center">
              {NAV.map(item => (
                <TopNavItem
                  key={item.href}
                  label={item.label}
                  href={item.href}
                  isSelected={pathname.startsWith(item.href)}
                  onClick={e => {
                    e.preventDefault();
                    go(item.href);
                  }}
                />
              ))}
            </Stack>
          }
          endContent={
            <Stack direction="horizontal" gap={1} vAlign="center">
              <Button
                label="Fall Semester 2026"
                variant="ghost"
                size="sm"
                endContent={<Icon icon="chevronDown" size="sm" />}
              />
              {session ? (
                <DropdownMenu
                  button={{
                    label: session.name,
                    variant: 'ghost',
                    size: 'sm',
                    icon: <Avatar name={session.name} size="sm" />,
                  }}
                  alignment="end"
                  items={[
                    {
                      id: 'account',
                      label: 'Account and privacy',
                      onClick: () => void navigate('/account'),
                    },
                    {type: 'divider'},
                    {
                      id: 'out',
                      label: 'Sign out',
                      onClick: () => void signOut(),
                    },
                  ]}
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
          }
        />
      }
    >
      <AlertDialog
        isOpen={pendingHref !== undefined}
        onOpenChange={open => {
          if (!open) {
            setPendingHref(undefined);
          }
        }}
        title="Leave this page?"
        description={guard ?? ''}
        cancelLabel="Stay"
        actionLabel="Leave"
        actionVariant="destructive"
        onAction={() => {
          const href = pendingHref;
          setPendingHref(undefined);
          setLeaveGuard(undefined);
          if (href !== undefined) {
            void navigate(href);
          }
        }}
      />
      <Stack width="100%" height="100%" gap={0}>
        {stale !== undefined && (
          <Banner
            status="warning"
            container="section"
            collapsible={false}
            isDismissable
            title={
              stale.since === undefined
                ? 'Course data may be out of date: a pull from Rice is overdue.'
                : `Course data may be out of date: nothing has been pulled from Rice since ${stale.since}.`
            }
          />
        )}
        <DemoBanner />
        <StackItem size="fill">
          <Outlet />
        </StackItem>
      </Stack>
    </AppShell>
  );
}
