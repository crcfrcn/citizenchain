import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  esbuild: { jsx: 'automatic' },
  resolve: { dedupe: ['react', 'react-dom'] },
  server: { fs: { allow: [fileURLToPath(new URL('../', import.meta.url)), fileURLToPath(new URL('../../../icons', import.meta.url)), fileURLToPath(new URL('../../../crates/scanner', import.meta.url))] } },
  test: { environment: 'jsdom', include: ['tests/**/*.test.ts', 'tests/**/*.test.tsx'], exclude: ['tests/documents.test.tsx'], setupFiles: ['tests/setup.ts'], restoreMocks: true },
});
