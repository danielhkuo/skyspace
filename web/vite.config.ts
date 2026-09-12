import {defineConfig} from 'vite';
import react from '@vitejs/plugin-react';

// Astryx 0.6 ships pre-built CSS, so no StyleX build plugin is needed here.
// See web/AGENTS.md, which the Astryx CLI generates.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
  },
});
