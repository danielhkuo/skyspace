import {defineConfig, loadEnv} from 'vite';
import react from '@vitejs/plugin-react';

// The API's development CSRF origin is `http://127.0.0.1:5173` exactly, so
// the app must be served and opened at that origin, never `localhost`.
const HOST = '127.0.0.1';

// Astryx 0.6 ships pre-built CSS, so no StyleX build plugin is needed here.
// See web/AGENTS.md, which the Astryx CLI generates.
export default defineConfig(({mode}) => {
  // `SKYSPACE_API_PORT` is the API's own variable (README); 8080 is its default.
  const apiPort =
    loadEnv(mode, '.', 'SKYSPACE_')['SKYSPACE_API_PORT'] ?? '8080';
  const apiTarget = `http://${HOST}:${apiPort}`;
  return {
    plugins: [react()],
    server: {
      host: HOST,
      port: 5173,
      // No rewrite: the API serves `/api/v1/...` and `/health` at those paths.
      // `changeOrigin: false` keeps the browser's `Origin` header, which the
      // API's CSRF guard compares against its public origin.
      proxy: {
        '/api': {target: apiTarget, changeOrigin: false},
        '/health': {target: apiTarget, changeOrigin: false},
      },
    },
  };
});
