import {beforeEach, describe, expect, it, vi} from 'vitest';

import type {Plan, PlanId} from '../domain';
import {StaleVersionError} from './types';

const savePlan = vi.fn<(plan: Plan) => Promise<void>>();

vi.mock('./index', () => ({dataSource: {savePlan: (p: Plan) => savePlan(p)}}));

const {flushPlanSave, getSaveStatus, queuePlanSave, subscribeSaveStatus} =
  await import('./planSaver');

const plan = (id: string): Plan => ({
  id: id as PlanId,
  name: id,
  catalogYear: 2026,
  matriculation: {academicYear: 2025, season: 'fall'},
  programs: [],
  incomingCredit: [],
  terms: [],
  selfChecks: [],
});

describe('planSaver', () => {
  beforeEach(() => {
    savePlan.mockReset();
  });

  it('reports saved after a write lands', async () => {
    savePlan.mockResolvedValue(undefined);
    queuePlanSave(plan('a'), 0);
    await flushPlanSave();
    expect(savePlan).toHaveBeenCalledTimes(1);
    expect(getSaveStatus()).toBe('saved');
  });

  it('reports conflict on a stale version and does not retry it', async () => {
    savePlan.mockRejectedValue(new StaleVersionError('The plan'));
    const seen: string[] = [];
    const stop = subscribeSaveStatus(() => seen.push(getSaveStatus()));
    queuePlanSave(plan('b'), 0);
    await flushPlanSave();
    stop();
    expect(getSaveStatus()).toBe('conflict');
    expect(seen).toContain('conflict');
    await flushPlanSave();
    expect(savePlan).toHaveBeenCalledTimes(1);
  });

  it('reports failed on any other rejection', async () => {
    savePlan.mockRejectedValue(new Error('network'));
    queuePlanSave(plan('c'), 0);
    await flushPlanSave();
    expect(getSaveStatus()).toBe('failed');
  });

  it('recovers on the next successful write', async () => {
    savePlan.mockRejectedValueOnce(new Error('network'));
    queuePlanSave(plan('d'), 0);
    await flushPlanSave();
    expect(getSaveStatus()).toBe('failed');
    savePlan.mockResolvedValueOnce(undefined);
    queuePlanSave(plan('d'), 0);
    await flushPlanSave();
    expect(getSaveStatus()).toBe('saved');
  });
});
