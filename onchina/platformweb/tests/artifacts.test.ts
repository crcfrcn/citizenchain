// 所属业务前端回归，公共测试辅助只有一份。
import { describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, join, normalize } from 'node:path';
import { SECURITY_GRANT_HEADER, PASSKEY_ASSERTION_HEADER, passkeySubmitHeaders, securityGrantSubmitHeaders, commitAdminAction, rustConst } from './security';

(process.env.ONCHINA_TEST_MODULE_ROOT ? it : it.skip)('实际IIFE装载复用宿主React、认证Provider与真实后端声明', async () => {
  vi.resetModules();
  const { realpathSync } = await import('node:fs');
  const { isAbsolute } = await import('node:path');
  const { execFile } = await import('node:child_process');
  const { promisify } = await import('node:util');
  const { createContext, runInContext, runInNewContext } = await import('node:vm');
  const moduleRoot = process.env.ONCHINA_TEST_MODULE_ROOT!;
  if (!isAbsolute(moduleRoot) || realpathSync(moduleRoot) !== moduleRoot) throw new Error('模块验收必须使用本轮实际绝对目录');
  const React = await import('react');
  const ReactDOM = await import('react-dom/client');
  const JSX = await import('react/jsx-runtime');
  const JSXDev = await import('react/jsx-dev-runtime');
  const antd = await import('antd');
  const icons = await import('@ant-design/icons');
  const { default: dayjs } = await import('dayjs');
  const { renderToString } = await import('react-dom/server');
  const bindings = { SharedSDK: await import("../workspace/index") };
  // 执行宿主真实初始化表达式，SDK字段只取真实公开index.ts。
  const main = readFileSync(join(import.meta.dirname, '../workspace/main.tsx'), 'utf8');
  const initializer = main.match(/globalThis\.OnChinaPlatform = (Object\.freeze\(\{[\s\S]*?\}\));/);
  if (!initializer) throw new Error('宿主公开对象初始化声明不存在');
  const sdk = runInNewContext(initializer[1], bindings) as Readonly<Record<string, unknown>>;
  const { AuthProvider } = await import('../authentication/Context');
  const { EMPTY_CAPABILITIES } = await import('../workspace/modules');
  const layout = JSON.parse(readFileSync(join(import.meta.dirname, '../../platform/tests/layout.json'), 'utf8')) as {
    capabilityCases: { code: string; owner: string | null; kind: string; flags: string[]; sections: unknown[] }[];
  };
  // 能力与工作台预期读取所属Rust包的唯一金标，不能遗漏拆包后的业务用例。
  for (const owner of ['registry', 'legislation', 'private', 'judicial']) {
    layout.capabilityCases.push(...JSON.parse(readFileSync(join(import.meta.dirname, '../../' + owner + '/tests/capabilities.json'), 'utf8')));
  }
  const assets: import('../workspace/modules').ModuleAsset[] = [];
  const scripts = new Map<string, string>();
  const execute = promisify(execFile);
  const ids: unknown = JSON.parse(process.env.ONCHINA_TEST_MODULE_IDS || 'null');
  if (!Array.isArray(ids) || !ids.length || new Set(ids).size !== ids.length || ids.some(id => typeof id !== 'string' || !/^[a-z][a-z0-9_]{0,63}$/.test(id))) throw new Error('缺少所选模块ID');
  for (const id of ids as string[]) {
    const directory = join(moduleRoot, id);
    const executable = join(directory, 'onchina-' + id + (process.platform === 'win32' ? '.exe' : ''));
    if (realpathSync(directory) !== directory || realpathSync(executable) !== executable) throw new Error('验收模块不能经过链接');
    const { stdout } = await execute(executable, ['--describe'], { encoding: 'utf8', timeout: 10000, maxBuffer: 1024 * 1024 });
    const description = JSON.parse(stdout) as { id: string; version: string; interface_version: number; frontend_exports: string[]; frontend_workspace: string | null; workspaces: Record<string, unknown> };
    expect(description.id).toBe(id);
    expect(description.interface_version).toBe(1);
    const base = '/modules/' + description.id + '/';
    assets.push({ id: description.id, version: description.version, interface_version: description.interface_version,
      exports: description.frontend_exports, workspace: description.frontend_workspace, institution_codes: Object.keys(description.workspaces), script: base + 'module.js', stylesheet: base + 'module.css' });
    scripts.set(base + 'module.js', readFileSync(join(directory, 'frontend/module.js'), 'utf8'));
  }
  expect(assets.length).toBeGreaterThan(0);
  const sharedModules: Record<string, import('../workspace/modules').LoadedModule> = Object.create(null);
  const sandbox = createContext({ OnChinaPlatform: Object.freeze(sdk), OnChinaModules: sharedModules, console, setTimeout, clearTimeout, URL, TextEncoder, TextDecoder, AbortController });
  vi.stubGlobal('OnChinaModules', sharedModules);
  vi.stubGlobal('window', { location: { href: 'https://onchina.local/', origin: 'https://onchina.local' } });
  vi.stubGlobal('document', {
    createElement: (tag: string) => ({ tag, remove() {}, onload: null, onerror: null }),
    head: { append: (...elements: { tag: string; src?: string; onload?: ((event: Event) => void) | null }[]) => {
      for (const element of elements) {
        if (element.tag === 'script') {
          const path = new URL(element.src!, 'https://onchina.local/').pathname;
          const script = scripts.get(path);
          if (!script) throw new Error('验收脚本不存在');
          runInContext(script, sandbox, { timeout: 10000 });
          globalThis.OnChinaModules = sandbox.OnChinaModules;
        }
        element.onload?.(new Event('load'));
      }
    } },
  });
  const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async (path) => {
    expect(path).toBe('/api/health');
    return new Response(JSON.stringify({ code: 0, data: { modules: assets } }));
  });
  sandbox.window = globalThis.window; sandbox.document = globalThis.document;
  sandbox.crypto = globalThis.crypto; sandbox.navigator = globalThis.navigator;
  let stored = '';
  vi.stubGlobal('sessionStorage', { getItem: () => stored || null, setItem: (_key: string, value: string) => { stored = value; }, removeItem: () => { stored = ''; } });
  try {
    const loader = await import('../workspace/modules');
    await loader.initializeModules();
    for (const asset of assets) {
      for (const name of asset.exports) expect(typeof loader.moduleView(asset.id, name)).toBe('function');
      const golden = layout.capabilityCases.find(item => item.owner === asset.id);
      if (!golden) throw new Error('缺少模块工作台金标');
      const capabilities = { ...EMPTY_CAPABILITIES };
      for (const flag of golden.flags) capabilities[flag as keyof typeof capabilities] = true;
      const auth = { accessToken: 'fixture-session', accountId: 'fixture-account', institutionCidNumber: 'fixture-institution', institutionCode: golden.code,
        familyName: '测', givenName: '试', capabilities,
        workspace: { workspace_kind: golden.kind, workspace_title: 'fixture', workspace_sections: golden.sections, workspace_modules: [] } };
      stored = JSON.stringify(auth);
      const View = loader.moduleView<Record<string, unknown>>(asset.id, asset.workspace ?? asset.exports[0]);
      const html = renderToString(React.createElement(AuthProvider, null,
        React.createElement(View, { auth, capabilities, passkeyRegistered: true, cidMeta: null, setCidMeta: () => {} })));
      expect(typeof html).toBe('string');
      expect(sandbox.OnChinaPlatform.react).toBe(React);
      expect(sandbox.OnChinaPlatform.platform_authentication_AuthContext).toBe(bindings.SharedSDK.platform_authentication_AuthContext);
    }
  } finally { fetcher.mockRestore(); vi.unstubAllGlobals(); }
}, 60000);
