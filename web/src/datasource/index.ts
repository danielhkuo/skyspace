/**
 * Which data source the app runs on. Only `demo` exists today: there is no
 * API yet, and this module is the one place that would know one.
 */
import {demoDataSource} from './demo';
import type {DataSource} from './types';

export type {DataSource, SavedCollection} from './types';

const requested: string = import.meta.env['VITE_DATA_SOURCE'] ?? 'demo';

if (requested !== 'demo') {
  throw new Error(
    `VITE_DATA_SOURCE=${requested} is not implemented; only "demo" exists.`,
  );
}

export const dataSource: DataSource = demoDataSource;
