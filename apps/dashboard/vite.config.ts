import path from 'node:path';
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  base: '/dashboard',
  resolve: {
    alias: {
      '@': path.resolve(__dirname, 'src'),
      '@components': path.resolve(__dirname, 'src/components'),
      '@stores': path.resolve(__dirname, 'src/stores'),
      '@graph': path.resolve(__dirname, 'src/graph'),
    },
  },
  build: {
    outDir: 'build',
    emptyOutDir: true,
    // Three.js alone is ~600 kB minified; with shaders + postprocessing it
    // crosses the default 500 kB warning every build. Raise the threshold
    // to silence the noise — the actual size budget is enforced via
    // bundle-size CI checks, not Vite's warning.
    chunkSizeWarningLimit: 1024,
    rollupOptions: {
      output: {
        // Split vendor chunks per concern so a Tailwind or i18n bump
        // doesn't bust the `three` cache (and vice versa). Returning
        // `undefined` lets Rollup fall back to its default grouping for
        // app code.
        //
        // CRITICAL — keep every CJS package that touches React's
        // `exports` object in a single chunk. React 19 ships as CJS;
        // Rollup wraps it in a lazy factory (`function av(){...}`) and
        // when two such CJS modules end up in DIFFERENT chunks but
        // share React's exports namespace, Rollup's chunk splitter can
        // drop the `var k = module.exports;` initializer at the top of
        // the factory. The factory then runs `k.Activity = …` against
        // an undefined `k`, which surfaces as a runtime
        // "Cannot set properties of undefined (setting 'Activity')"
        // crash before any component renders. Reproducer:
        // `use-sync-external-store` (CJS shim) was in the `state`
        // chunk while `react`+`react-dom` were in `react` — every page
        // load failed. Keep `use-sync-external-store` and `scheduler`
        // beside React so the factory stays whole.
        manualChunks(id) {
          if (!id.includes('node_modules')) return undefined;
          if (id.includes('three')) return 'three';
          if (id.includes('@tanstack/react-query')) return 'query';
          if (
            id.includes('react-router') ||
            id.includes('/react-dom/') ||
            id.includes('/react/') ||
            id.includes('/scheduler/') ||
            id.includes('use-sync-external-store')
          ) {
            return 'react';
          }
          if (id.includes('i18next') || id.includes('react-i18next')) return 'i18n';
          if (id.includes('zustand')) return 'state';
          if (id.includes('zod')) return 'zod';
          return undefined;
        },
      },
    },
  },
  server: {
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:3927',
        changeOrigin: true,
      },
      '/ws': {
        target: 'ws://127.0.0.1:3927',
        ws: true,
      },
    },
  },
});
