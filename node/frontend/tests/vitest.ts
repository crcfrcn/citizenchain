import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const { FLAT_FEE } = createRequire(import.meta.url)('../../../scripts/build.mjs');

export default defineConfig({
  esbuild: { jsx: 'automatic' },
  define: { FLAT_FEE: JSON.stringify(FLAT_FEE) },
  resolve: { dedupe: ['react', 'react-dom'] },
  server: { fs: { allow: [fileURLToPath(new URL('../', import.meta.url)), fileURLToPath(new URL('../../../icons', import.meta.url)), fileURLToPath(new URL('../../../crates/scanner', import.meta.url)),fileURLToPath(new URL('../../../runtime/transaction/mldsa', import.meta.url))] } },
  test: { environment: 'jsdom', include: ['tests/**/*.test.ts', 'tests/**/*.test.tsx'], exclude: ['tests/documents.test.tsx'], setupFiles: ['tests/setup.ts'], restoreMocks: true },
});
