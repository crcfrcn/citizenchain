// 单元测试只加载自己的配置，不启动开发 HTTPS、HMR 或生产代理。
import { defineConfig } from 'vitest/config';
import { readFileSync } from 'node:fs';

const dependencies = Object.keys(
  JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8')).dependencies,
);

export default defineConfig({
  resolve: { dedupe: dependencies },
  test: { environment: 'node', include: ['**/*.test.ts', '**/*.test.tsx'] },
});
