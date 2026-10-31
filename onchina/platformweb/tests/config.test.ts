// 工程结构使用真实声明与公开入口，类型检查由本仓公开命令实际执行。
import { expect, it } from 'vitest';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
it('独立前端只保留必需根文件，公开入口指向当前唯一实现', () => {
  const root = resolve(import.meta.dirname, '../..');
  for (const folder of ['platformweb', 'registryweb', 'legislationweb', 'private/frontend', 'judicial/frontend']) {
    const frontend = join(root, folder);
    const expected = ['.npmrc', 'package-lock.json', 'package.json', ...(folder.endsWith('/frontend') ? ['Workspace.tsx', 'Workspace.test.tsx'] : [])].sort();
    expect(readdirSync(frontend, {withFileTypes: true}).filter(item => item.isFile()).map(item => item.name).sort()).toEqual(expected);
    const declared = JSON.parse(readFileSync(join(frontend, 'package.json'), 'utf8'));
    const lock = JSON.parse(readFileSync(join(frontend, 'package-lock.json'), 'utf8'));
    expect(lock.name).toBe(declared.name);
    expect(declared.scripts.typecheck).toContain('typecheck --frontend .');
    expect(declared.scripts.test).toContain('test --frontend .');
    for (const target of Object.values(declared.exports)) {
      // 仅供共享类型的入口使用标准types条件，不声明不存在的运行时对象。
      if (typeof target !== 'string') expect(Object.keys(target as object)).toEqual(['types']);
      const source = typeof target === 'string' ? target : (target as { types: string }).types;
      expect(source.startsWith('./')).toBe(true);
      expect(existsSync(resolve(frontend, source))).toBe(true);
    }
    if (folder.endsWith('/frontend')) expect(readdirSync(frontend, {withFileTypes: true}).filter(item => item.isDirectory() && item.name !== 'node_modules')).toEqual([]);
  }
  expect(existsSync(join(root, 'tsconfig.json'))).toBe(false);
});

it('办证授权只属于运行期，构建无需启动宿主或配置证书', async () => {
  const {manifest} = await import(new URL('../../../scripts/config.mjs', import.meta.url).href);
  expect(manifest.runtime.certificate).toEqual({command:'onchina authorize-tls',input:'hidden-terminal',storage:'system-keyring'});
  expect(manifest.configuration.items.some((item: {name: string}) => item.name === 'ONCHINA_CERT_AUTHORIZATION')).toBe(false);
  const {frontendConfiguration} = await import(new URL('../../../scripts/onchina.mjs', import.meta.url).href);
  const {vi} = await import('vitest');
  vi.stubEnv('ONCHINA_BASE_URL','http://invalid-build-only.example');
  try {
    const config = await frontendConfiguration(resolve(import.meta.dirname,'..'),{command:'build'});
    expect(config.server.proxy).toBeUndefined();
    expect(config.preview.proxy).toBeUndefined();
  } finally { vi.unstubAllEnvs(); }
});
