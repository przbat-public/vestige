import path from 'node:path';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, 'src'),
      '@components': path.resolve(__dirname, 'src/components'),
      '@stores': path.resolve(__dirname, 'src/stores'),
      '@graph': path.resolve(__dirname, 'src/graph'),
    },
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    include: ['src/**/*.test.{ts,tsx}'],
    css: false,
    // Must stay above the 5 s async-utility budget configured in
    // src/test/setup.ts: with a shorter test timeout a slow assertion is
    // reported as "test timed out", which hides which assertion failed.
    testTimeout: 15000,
  },
});
