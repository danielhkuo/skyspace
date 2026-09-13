import {Banner} from '@astryxdesign/core/Banner';
import {Button} from '@astryxdesign/core/Button';
import {useState} from 'react';

import {dataSource} from '../datasource';
import {PLAN_ID} from '../fixtures/csStats';

/**
 * Says out loud that nothing here reaches a server, and offers a reset so a
 * demo can start over. Dismissable per session; the reset survives dismissal
 * through the account menu later.
 */
export function DemoBanner() {
  const [dismissed, setDismissed] = useState(false);
  if (dataSource.kind !== 'demo' || dismissed) {
    return null;
  }
  const reset = async (): Promise<void> => {
    await dataSource.resetPlan(PLAN_ID);
    window.location.reload();
  };
  return (
    <Banner
      status="info"
      container="section"
      collapsible={false}
      isDismissable
      onDismiss={() => setDismissed(true)}
      title="Demo data"
      description="Fall 2026 fixture, one sample plan. Nothing reaches a server; edits stay in this browser."
      endContent={
        <Button
          label="Reset demo"
          variant="secondary"
          size="sm"
          clickAction={reset}
        />
      }
    />
  );
}
