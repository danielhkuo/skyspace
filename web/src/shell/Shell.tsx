import {AppShell} from '@astryxdesign/core/AppShell';
import {Button} from '@astryxdesign/core/Button';
import {Icon} from '@astryxdesign/core/Icon';
import {Stack, StackItem} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {TopNav, TopNavItem} from '@astryxdesign/core/TopNav';
import {Outlet, useLocation} from 'react-router';

import {DemoBanner} from './DemoBanner';
import {NarrowViewportNotice} from './NarrowViewportNotice';
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
              <Button label="Sign in" variant="secondary" size="sm" />
            </Stack>
          }
        />
      }
    >
      <Stack width="100%" height="100%" gap={0}>
        <DemoBanner />
        <StackItem size="fill">
          <Outlet />
        </StackItem>
      </Stack>
    </AppShell>
  );
}
