import { fileURLToPath } from 'node:url';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';

const page = (name: string) => fileURLToPath(new URL(`./src/${name}`, import.meta.url));

// Tauri loads http://localhost:1420 in dev and ../dist in release (src-tauri/tauri.conf.json).
export default defineConfig({
  root: 'src',
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    outDir: '../dist',
    emptyOutDir: true,
    target: 'es2022',
    rolldownOptions: {
      input: {
        stack: page('stack.html'),
        settings: page('settings.html'),
        compare: page('compare.html'),
        batch: page('batch.html'),
        drop: page('drop.html'),
      },
    },
  },
  test: { include: ['**/*.test.ts'] },
});
