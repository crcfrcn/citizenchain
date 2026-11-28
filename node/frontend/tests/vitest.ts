// 提供节点前端测试共用的Vitest辅助接口。

import { defineConfig, type UserConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const { FLAT_FEE, qrWasmDefinition } = createRequire(import.meta.url)('../../../scripts/build.mjs');

export default defineConfig(async (): Promise<UserConfig> => ({
  // 测试根固定为前端包，避免从仓库根启动时在根目录生成测试缓存。
  root: fileURLToPath(new URL('../', import.meta.url)),
  esbuild: { jsx: 'automatic' },
  ssr: { noExternal: ['@gmb/scanner-react', 'qrcode.react'] },
  define: { FLAT_FEE: JSON.stringify(FLAT_FEE), __CITIZEN_QR_WASM__: await qrWasmDefinition() },
  resolve: { dedupe: ['react', 'react-dom'] },
  server: { fs: { allow: [fileURLToPath(new URL('../', import.meta.url)), fileURLToPath(new URL('../../../icons', import.meta.url)), fileURLToPath(new URL('../../../crates/scanner', import.meta.url)),fileURLToPath(new URL('../../../runtime/transaction/mldsa', import.meta.url))] } },
  test: {
    environment: 'jsdom',
    // Node 25宿主的实验性WebStorage不得覆盖jsdom提供的浏览器存储。
    poolOptions: { forks: { execArgv: ['--no-experimental-webstorage'] } },
    include: ['tests/**/*.test.ts', 'tests/**/*.test.tsx'], exclude: ['tests/documents.test.tsx'], setupFiles: ['tests/setup.ts'], restoreMocks: true,
  },
}));
