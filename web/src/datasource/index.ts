/**
 * Which data source the app runs on. `VITE_DATA_SOURCE=api` talks to
 * `skyspace-api` on the same origin; anything else, or nothing, is the demo.
 */
import {apiDataSource} from './api';
import {demoDataSource} from './demo';
import type {DataSource} from './types';

export type {DataSource} from './types';

function select(requested: string): DataSource {
  switch (requested) {
    case 'demo':
      return demoDataSource;
    case 'api':
      return apiDataSource;
    default:
      throw new Error(
        `VITE_DATA_SOURCE=${requested} is not implemented; only "demo" and "api" exist.`,
      );
  }
}

export const dataSource: DataSource = select(
  import.meta.env['VITE_DATA_SOURCE'] ?? 'demo',
);
