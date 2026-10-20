import { describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { join, normalize } from 'node:path';

// 三档鉴权在前端的头部组装 + 与 Rust 后端的 header 名跨端锁。
//
// 后端独立执行鉴权；这里同时核验实际业务请求，防止只断言辅助函数而漏掉 Bearer。
// 覆盖内容:
//   1. 档位对应的头部组装 —— Passkey 档不得带 grant 头,PasskeyColdSign 档必须两头俱全
//   2. header 名与后端逐字对齐 —— 漂移会让请求被后端当作"未带凭证"整笔拒绝
//
// `assertPasskey` 走 WebAuthn(navigator.credentials),这里 mock 掉:
// 要测真实 WebAuthn 就得 mock 整套浏览器 API,测的是 mock 而非真实行为。

vi.mock('./authentication_passkeyClient', async () => {
  const actual = await vi.importActual<
    typeof import('./authentication_passkeyClient')
  >('./authentication_passkeyClient');
  return {
    ...actual,
    // 只替换需要浏览器的那一个;常量仍取真实值,否则跨端锁就成了自证。
    assertPasskey: vi.fn(async () => 'fake-passkey-assertion'),
  };
});

const {
  SECURITY_GRANT_HEADER,
  passkeySubmitHeaders,
  securityGrantSubmitHeaders,
  commitAdminAction,
} = await import('./admins_securityApi');
const { PASSKEY_ASSERTION_HEADER } = await import(
  './authentication_passkeyClient'
);

// 中文注释：前端目录与平台功能目录相邻，直接读取同一平台的实际 Rust 源码。
const ONCHINA_PLATFORM = join(import.meta.dirname, '..');

/** 从 Rust 源码里抠出 `const NAME: &str = "value";` 的字面量。 */
function rustConst(relativePath: string, name: string): string {
  const source = readFileSync(join(ONCHINA_PLATFORM, relativePath), 'utf8');
  const matched = source.match(
    new RegExp(`const\\s+${name}\\s*:\\s*&str\\s*=\\s*"([^"]+)"`),
  );
  if (!matched) throw new Error(`未在 ${relativePath} 找到常量 ${name}`);
  return matched[1];
}

describe('前端 header 名与 Rust 后端一致(直读后端源码)', () => {
  // 直接读取本产品后端源码，对齐实际请求头契约。
  // 两端各自的测试都只对齐自己这一侧,名字漂移会两边全绿而线上 100% 拒绝。
  it('SECURITY_GRANT_HEADER 与 authentication/actions.rs 的 ADMIN_SECURITY_GRANT_HEADER 逐字一致', () => {
    const backend = rustConst('authentication/actions.rs', 'ADMIN_SECURITY_GRANT_HEADER');
    expect(SECURITY_GRANT_HEADER).toBe(backend);
  });

  it('PASSKEY_ASSERTION_HEADER 与 authentication/passkey.rs 逐字一致', () => {
    // 严格相等,不做大小写归一:后端 security/http.rs 用 HeaderName::from_static
    // 注册该头,那个 API 只接受小写、否则 panic,故小写是全仓唯一合法写法。
    const backend = rustConst(
      'authentication/passkey.rs',
      'PASSKEY_ASSERTION_HEADER',
    );
    expect(PASSKEY_ASSERTION_HEADER).toBe(backend);
  });
});

describe('三档鉴权的前端头部组装', () => {
  const auth = { account_id: '0x' + '11'.repeat(32) } as never;

  it('本地写(Passkey 档)只带 passkey 断言,不得带冷签 grant 头', async () => {
    const headers = await passkeySubmitHeaders(auth);
    expect(headers[PASSKEY_ASSERTION_HEADER]).toBe('fake-passkey-assertion');
    // 带上 grant 头意味着把本地写伪装成链上写;后端会去消费一个不存在的 grant。
    expect(headers[SECURITY_GRANT_HEADER]).toBeUndefined();
  });

  it('链上写(PasskeyColdSign 档)必须同时带 grant 与 passkey 两个头', async () => {
    const headers = await securityGrantSubmitHeaders(auth, {
      grant_id: 'grant-123',
    } as never);
    // 缺任一个后端都 fail-closed;这里钉死"两者俱全"这个前置条件。
    expect(headers[SECURITY_GRANT_HEADER]).toBe('grant-123');
    expect(headers[PASSKEY_ASSERTION_HEADER]).toBe('fake-passkey-assertion');
  });

  it('基础头被保留且不被鉴权头覆盖', async () => {
    const headers = await passkeySubmitHeaders(auth, {
      'content-type': 'application/json',
    });
    expect(headers['content-type']).toBe('application/json');
    expect(headers[PASSKEY_ASSERTION_HEADER]).toBe('fake-passkey-assertion');
  });
});

describe('实际业务API补齐一次Passkey且不改变提交字段', () => {
  const auth = { access_token: 'test-session', account_id: '0x' + '11'.repeat(32) } as never;

  it('公民编辑和资料上传删除均携带一次性断言', async () => {
    const api = await import('../../registry/frontend/citizens_api');
    const { assertPasskey } = await import('./authentication_passkeyClient');
    const fetcher = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(JSON.stringify({ code: 0, data: {} })));
    try {
      for (const operation of [
        () => api.editCitizen(auth, 'citizen', {} as never),
        () => api.uploadCitizenDocument(auth, 'citizen', new File(['file'], 'sample.bin'), '其他材料'),
        () => api.deleteCitizenDocument(auth, 'citizen', 1),
      ]) {
        vi.mocked(assertPasskey).mockClear();
        fetcher.mockClear();
        fetcher.mockResolvedValue(new Response(JSON.stringify({ code: 0, data: {} })));
        await operation();
        expect(assertPasskey).toHaveBeenCalledTimes(1);
        const headers = new Headers(fetcher.mock.calls[0][1]?.headers);
        expect(headers.get('authorization')).toBe('Bearer test-session');
        expect(headers.get(PASSKEY_ASSERTION_HEADER)).toBe('fake-passkey-assertion');
      }
    } finally { fetcher.mockRestore(); }
  });

  it('公民编辑保留原URL、JSON字段及一次认证，取消时不发送写请求', async () => {
    const { editCitizen } = await import('../../registry/frontend/citizens_api');
    const { assertPasskey } = await import('./authentication_passkeyClient');
    const fetcher = vi.spyOn(globalThis, 'fetch');
    const payload = { family_name: '测', given_name: '试', city_code: '001' } as never;
    try {
      vi.mocked(assertPasskey).mockClear();
      fetcher.mockResolvedValue(new Response(JSON.stringify({ code: 0, data: {} })));
      await editCitizen(auth, 'citizen', payload);
      expect(assertPasskey).toHaveBeenCalledTimes(1);
      expect(fetcher).toHaveBeenCalledTimes(1);
      expect(fetcher.mock.calls[0][0]).toBe('/api/admin/citizens/citizen/edit');
      expect(fetcher.mock.calls[0][1]?.method).toBe('POST');
      expect(JSON.parse(String(fetcher.mock.calls[0][1]?.body))).toEqual(payload);
      expect(new Headers(fetcher.mock.calls[0][1]?.headers).get('content-type')).toBe('application/json');
      fetcher.mockClear();
      vi.mocked(assertPasskey).mockRejectedValueOnce(new Error('cancelled'));
      await expect(editCitizen(auth, 'citizen', payload)).rejects.toThrow('cancelled');
      expect(fetcher).not.toHaveBeenCalled();
    } finally { fetcher.mockRestore(); }
  });

  it('直接治理commit取一次Passkey，普通grant commit不重复取', async () => {
    const { assertPasskey } = await import('./authentication_passkeyClient');
    const fetcher = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(JSON.stringify({ code: 0, data: {} })));
    try {
      for (const action_type of ['CREATE_SUBORDINATE_REGISTRY', 'DELETE_SUBORDINATE_REGISTRY', 'NODE_BINDING_UNBIND', 'INSTITUTION_UPDATE'] as const) {
        vi.mocked(assertPasskey).mockClear();
        fetcher.mockResolvedValue(new Response(JSON.stringify({ code: 0, data: {} })));
        await commitAdminAction(auth, { action_id: 'action', action_type, payload_hash: 'hash', account_id: 'account', signature: 'signature' });
        expect(assertPasskey).toHaveBeenCalledTimes(action_type === 'INSTITUTION_UPDATE' ? 0 : 1);
        const calls = fetcher.mock.calls;
        const body = JSON.parse(String(calls[calls.length - 1]?.[1]?.body));
        expect(body).toEqual({ action_id: 'action', payload_hash: 'hash', account_id: 'account', signature: 'signature' });
      }
    } finally { fetcher.mockRestore(); }
  });

  it('Passkey被取消时不得发送业务写请求', async () => {
    const { assertPasskey } = await import('./authentication_passkeyClient');
    const fetcher = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(JSON.stringify({ code: 0, data: {} })));
    try {
      vi.mocked(assertPasskey).mockRejectedValueOnce(new Error('cancelled'));
      await expect(commitAdminAction(auth, { action_id: 'action', action_type: 'NODE_BINDING_UNBIND', payload_hash: 'hash' })).rejects.toThrow('cancelled');
      expect(fetcher).not.toHaveBeenCalled();
    } finally { fetcher.mockRestore(); }
  });

  it('五处链签名准备各等待一次Passkey，保留请求且失败不重试', async () => {
    const { assertPasskey } = await import('./authentication_passkeyClient');
    const { prepareInstitutionGovernance } = await import('./admins_api');
    const { prepareRegisterInstitutionAdmins } = await import('../../registry/frontend/admins_cityRegistryAdminsApi');
    const { proposeLegislation, castRepresentativeVote } = await import('../../legislation/frontend/legislation_api');
    const { proposePlatformPrice } = await import('../../private/frontend/membership_api');
    const governance = { cid_number: 'fixture-institution', proposer_role_code: 'REPRESENTATIVE' };
    const registration = { cid_number: 'fixture-institution', actor_role_code: 'REGISTRY-ROLE', admins: [{ account_id: 'fixture-account', family_name: '测', given_name: '试' }] };
    const legislation: import('../../legislation/frontend/legislation_types').ProposeLawInput = {
      lawAction: 'enact', proposerRoleCode: 'REPRESENTATIVE', tier: 1,
      voteType: 2, title: '测试法律', chapters: [], effectiveAt: 1000, lawId: null,
    };
    const operations = [
      { path: '/api/admin/institution/governance/prepare', payload: governance, run: () => prepareInstitutionGovernance(auth, governance) },
      { path: '/api/admin/institution/admins/register/prepare', payload: registration, run: () => prepareRegisterInstitutionAdmins(auth, registration) },
      { path: '/api/legislation/propose', payload: legislation, run: () => proposeLegislation(auth, legislation) },
      { path: '/api/legislation/representative-vote', payload: { proposalId: 7, voterRoleCode: 'REPRESENTATIVE', approve: true }, run: () => castRepresentativeVote(auth, 7, 'REPRESENTATIVE', true) },
      { path: '/api/membership/platform-prices/propose', payload: { proposer_role_code: 'COMMITTEE_MEMBER', membership_level: 'freedom', new_price_fen: '100' }, run: () => proposePlatformPrice(auth, 'COMMITTEE_MEMBER', 'freedom', '100') },
    ];
    const assertion = vi.mocked(assertPasskey);
    const fetcher = vi.spyOn(globalThis, 'fetch');
    const prepared = { request_id: 'fixture-request', sign_request: 'fixture-sign-request' };
    try {
      for (const operation of operations) {
        assertion.mockReset(); fetcher.mockReset();
        let complete!: (value: string) => void;
        assertion.mockReturnValueOnce(new Promise<string>(resolve => { complete = resolve; }));
        fetcher.mockResolvedValueOnce(new Response(JSON.stringify({ code: 0, data: prepared })));
        const pending = operation.run();
        expect(assertion).toHaveBeenCalledTimes(1);
        expect(assertion).toHaveBeenCalledWith(auth);
        expect(fetcher).not.toHaveBeenCalled();
        complete('fake-passkey-assertion');
        await expect(pending).resolves.toEqual(prepared);
        expect(fetcher).toHaveBeenCalledTimes(1);
        const [path, init] = fetcher.mock.calls[0];
        const headers = new Headers(init?.headers);
        expect(path).toBe(operation.path);
        expect(init?.method).toBe('POST');
        expect(headers.get('authorization')).toBe('Bearer test-session');
        expect(headers.get('content-type')).toBe('application/json');
        expect(headers.get(PASSKEY_ASSERTION_HEADER)).toBe('fake-passkey-assertion');
        expect(headers.has(SECURITY_GRANT_HEADER)).toBe(false);
        expect(JSON.parse(String(init?.body))).toEqual(operation.payload);

        for (const failure of [new Error('cancelled'), new Error('verification failed')]) {
          assertion.mockReset(); fetcher.mockClear();
          assertion.mockRejectedValueOnce(failure);
          await expect(operation.run()).rejects.toBe(failure);
          expect(assertion).toHaveBeenCalledTimes(1);
          expect(fetcher).not.toHaveBeenCalled();
        }
        assertion.mockReset(); fetcher.mockClear();
        assertion.mockResolvedValueOnce('fake-passkey-assertion');
        fetcher.mockResolvedValueOnce(new Response(JSON.stringify({ code: 2003, message: 'denied' }), { status: 403 }));
        await expect(operation.run()).rejects.toThrow('denied');
        expect(assertion).toHaveBeenCalledTimes(1);
        expect(fetcher).toHaveBeenCalledTimes(1);
      }
    } finally {
      assertion.mockReset(); assertion.mockResolvedValue('fake-passkey-assertion');
      fetcher.mockRestore();
    }
  });
});


// 用 TypeScript 自己的配置解析器验证所有功能前端都进入真实项目，不依赖编辑器缓存。
describe('跨目录前端项目配置', () => {
  it('浏览器、Node 工具与根解决方案分别发现正确文件', async () => {
    const ts = await import('typescript');
    const browserPath = join(import.meta.dirname, 'tsconfig.json');
    const nodePath = join(import.meta.dirname, 'tsconfig.node.json');
    const parse = (path: string) => {
      const file = ts.readConfigFile(path, ts.sys.readFile);
      expect(file.error).toBeUndefined();
      return ts.parseJsonConfigFileContent(file.config, ts.sys, import.meta.dirname);
    };
    const browser = parse(browserPath);
    expect(browser.errors).toEqual([]);
    expect(browser.options.noEmit).toBe(true);
    for (const suffix of ['/registry/frontend/citizens_CitizensView.tsx', '/legislation/frontend/legislation_LegislationView.tsx', '/judicial/frontend/judicial_JudicialDisplay.tsx', '/private/frontend/membership_PlatformPricePanel.tsx']) {
      expect(browser.fileNames.some((file) => file.endsWith(suffix))).toBe(true);
    }
    expect(browser.fileNames.some((file) => file.endsWith('vite.config.ts'))).toBe(false);
    const node = parse(nodePath);
    expect(node.errors).toEqual([]);
    expect(node.options.types).toContain('node');
    // TypeScript返回斜线路径，先转为当前平台格式再核对完整文件身份。
    const nodeFiles = node.fileNames.map(normalize);
    expect(nodeFiles).toContain(join(import.meta.dirname, 'admins_securityHeaders.test.ts'));
    expect(nodeFiles).toContain(join(import.meta.dirname, 'vitest.config.ts'));
    const root = ts.readConfigFile(join(import.meta.dirname, '../../tsconfig.json'), ts.sys.readFile);
    expect(root.config.files).toEqual([]);
    expect(root.config.references.map((entry: { path: string }) => entry.path)).toEqual(['./platform/frontend/tsconfig.json', './platform/frontend/tsconfig.node.json']);
  });
});


// 模块装载只改变资源入口；回归不启动服务、钱包或浏览器。
describe('模块前端装载与重启边界', () => {
  type Asset = import('./platform_capabilityMap').ModuleAsset;
  type ElementFixture = { tag: string; src?: string; href?: string; onload?: ((event: Event) => unknown) | null; onerror?: ((event: Event) => unknown) | null; removed: boolean; remove: () => void };
  const asset = (id: string, codes = ['FRG']): Asset => ({
    id, version: '1.0.0', interface_version: 1, script: '/modules/' + id + '/module.js?v=1.0.0',
    stylesheet: '/modules/' + id + '/module.css?v=1.0.0', exports: ['Workspace'], institution_codes: codes, workspace: 'Workspace',
  });
  async function fixture(items: Asset[], options: { badVersion?: string; badStyle?: string; badScript?: string; pendingScript?: string; pendingHealth?: boolean } = {}) {
    vi.resetModules();
    const elements: ElementFixture[] = [];
    const views = Object.fromEntries(items.map(item => [item.id, () => null]));
    const modules = Object.fromEntries(items.map(item => [item.id, {
      Workspace: views[item.id], __onchina: { id: item.id, version: options.badVersion === item.id ? '2.0.0' : item.version, interface_version: 1 },
    }]));
    vi.stubGlobal('window', { location: { href: 'https://onchina.local/', origin: 'https://onchina.local' } });
    vi.stubGlobal('OnChinaModules', modules);
    vi.stubGlobal('document', {
      createElement: (tag: string) => {
        const element: ElementFixture = { tag, removed: false, remove: () => { element.removed = true; } };
        return element;
      },
      head: { append: (...nodes: ElementFixture[]) => {
        elements.push(...nodes);
        for (const node of nodes) {
          if (node.tag === 'link' && options.badStyle && node.href?.includes('/' + options.badStyle + '/')) node.onerror?.(new Event('error'));
          else if (node.tag === 'script' && options.badScript && node.src?.includes('/' + options.badScript + '/')) node.onerror?.(new Event('error'));
          else if (!(node.tag === 'script' && options.pendingScript && node.src?.includes('/' + options.pendingScript + '/'))) node.onload?.(new Event('load'));
        }
      } },
    });
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async (_input, init) => {
      if (options.pendingHealth) return new Promise<Response>((_resolve, reject) => {
        init?.signal?.addEventListener('abort', () => reject(new Error('aborted')), { once: true });
      });
      return new Response(JSON.stringify({ code: 0, data: { modules: items } }));
    });
    const loader = await import('./platform_capabilityMap');
    return { loader, fetcher, elements, views, dispose: () => { fetcher.mockRestore(); vi.unstubAllGlobals(); vi.useRealTimers(); } };
  }

  it('一次启动只装载一次，返回模块原组件并保留原会话请求方式', async () => {
    const context = await fixture([asset('example')]);
    try {
      await context.loader.initializeModules();
      await context.loader.initializeModules();
      expect(context.fetcher).toHaveBeenCalledTimes(1);
      expect(context.fetcher.mock.calls[0][0]).toBe('/api/health');
      expect(context.elements.map(element => element.tag)).toEqual(['link', 'script']);
      expect(context.loader.moduleView('example', 'Workspace')).toBe(context.views.example);
      expect(() => context.loader.moduleView('missing', 'Workspace')).toThrow('未装载');
    } finally { context.dispose(); }
  });
  it('后端前端版本不符时只隔离该模块，不替换其他模块', async () => {
    const context = await fixture([asset('broken'), asset('ready', ['NJD'])], { badVersion: 'broken' });
    try {
      await context.loader.initializeModules();
      expect(() => context.loader.moduleView('broken', 'Workspace')).toThrow('不匹配');
      expect(context.loader.moduleView('ready', 'Workspace')).toBe(context.views.ready);
    } finally { context.dispose(); }
  });
  it('样式失败及跨源资源均拒绝，不能执行外部模块脚本', async () => {
    const foreign = { ...asset('foreign'), script: 'https://other.invalid/modules/foreign/module.js' };
    const context = await fixture([asset('broken'), foreign], { badStyle: 'broken' });
    try {
      await context.loader.initializeModules();
      expect(() => context.loader.moduleView('broken', 'Workspace')).toThrow('样式装载失败');
      expect(() => context.loader.moduleView('foreign', 'Workspace')).toThrow('资源地址无效');
      expect(context.elements.filter(element => element.src?.startsWith('https://other.invalid'))).toEqual([]);
    } finally { context.dispose(); }
  });
  it('脚本失败清理本模块资源和导出，其他模块保持', async () => {
    const context = await fixture([asset('broken'), asset('ready', ['NJD'])], { badScript: 'broken' });
    try {
      await context.loader.initializeModules();
      expect(() => context.loader.moduleView('broken', 'Workspace')).toThrow('脚本装载失败');
      expect(globalThis.OnChinaModules.broken).toBeUndefined();
      expect(context.elements.filter(element => (element.src ?? element.href)?.includes('/broken/')).every(element => element.removed)).toBe(true);
      expect(context.elements.filter(element => (element.src ?? element.href)?.includes('/ready/')).every(element => !element.removed)).toBe(true);
      expect(context.loader.moduleView('ready', 'Workspace')).toBe(context.views.ready);
    } finally { context.dispose(); }
  });
  it('迟迟不结束的资源超时隔离，清理计时器并结束本次启动', async () => {
    vi.useFakeTimers();
    const context = await fixture([asset('slow'), asset('ready', ['NJD'])], { pendingScript: 'slow' });
    try {
      const startup = context.loader.initializeModules();
      await vi.advanceTimersByTimeAsync(16000);
      await startup;
      expect(() => context.loader.moduleView('slow', 'Workspace')).toThrow('装载超时');
      expect(context.loader.moduleView('ready', 'Workspace')).toBe(context.views.ready);
      expect(context.elements.filter(element => (element.src ?? element.href)?.includes('/slow/')).every(element => element.removed && !element.onload && !element.onerror)).toBe(true);
      expect(globalThis.OnChinaModules.slow).toBeUndefined();
      globalThis.OnChinaModules.slow = { Workspace: () => null, __onchina: { id: 'slow', version: '1.0.0', interface_version: 1 } };
      expect(globalThis.OnChinaModules.slow).toBeUndefined();
      expect(() => context.loader.moduleView('slow', 'Workspace')).toThrow('装载超时');
      expect(vi.getTimerCount()).toBe(0);
      await context.loader.initializeModules();
      expect(context.fetcher).toHaveBeenCalledTimes(1);
    } finally { context.dispose(); }
  });
  it('健康声明超时也结束等待，不创建资源节点', async () => {
    vi.useFakeTimers();
    const context = await fixture([], { pendingHealth: true });
    try {
      const rejected = expect(context.loader.initializeModules()).rejects.toThrow('aborted');
      await vi.advanceTimersByTimeAsync(16000);
      await rejected;
      expect(context.elements).toEqual([]);
      expect(vi.getTimerCount()).toBe(0);
    } finally { context.dispose(); }
  });
  it('准确实例工作台优先；同一机构类出现冲突时拒绝猜测归属', async () => {
    const context = await fixture([asset('generic'), asset('instance')]);
    try {
      await context.loader.initializeModules();
      const auth = { institution_code: 'FRG', workspace: { workspace_modules: ['instance'] } } as never;
      expect(context.loader.workspaceView(auth)).toBe(context.views.instance);
      expect(() => context.loader.workspaceView({ institution_code: 'FRG' } as never)).toThrow('归属冲突');
      expect(context.loader.workspaceView({ institution_code: 'UNKNOWN' } as never)).toBeNull();
    } finally { context.dispose(); }
  });
});


describe('模块装载失败清理与路径边界', () => {
  it('POSIX与Windows均拒绝同名前缀、父级和跨盘路径', async () => {
    const { insideModule } = await import('./vite.config');
    const { posix, win32 } = await import('node:path');
    for (const [paths, owner] of [[posix, '/repo/onchina/registry/frontend'], [win32, 'C:\\repo\\onchina\\registry\\frontend']] as const) {
      expect(insideModule(owner, paths.join(owner, 'Workspace.tsx'), paths)).toBe(true);
      for (const candidate of [owner, owner + '-other/Workspace.tsx', paths.join(owner, '../other.tsx')]) {
        expect(insideModule(owner, candidate, paths)).toBe(false);
      }
    }
    expect(insideModule('C:\\repo', 'D:\\repo\\Workspace.tsx', win32)).toBe(false);
  });

  it('开发与预览的API和模块资源复用同一受校验HTTPS代理', async () => {
    const { default: config } = await import('./vite.config');
    const value = await config({ command: 'build', mode: 'test' });
    for (const options of [value.server, value.preview]) {
      const proxy = options?.proxy;
      expect(proxy).toMatchObject({ '/api': { secure: true }, '/modules': { secure: true, agent: { options: { rejectUnauthorized: true } } } });
      if (!proxy) throw new Error('missing proxy');
      const api = proxy['/api']; const modules = proxy['/modules'];
      if (typeof api === 'string' || typeof modules === 'string' || !api || !modules) throw new Error('invalid proxy');
      expect(Object.keys(proxy)).toEqual(['/api', '/modules']);
      expect(api.target).toBe(modules.target);
      expect(api.agent).toBe(modules.agent);
    }
  });
});


describe('登记管理员原有请求参数衔接', () => {
  it('原URL、Bearer与管理员集合保持，透传操作注册局岗位码', async () => {
    const { prepareRegisterInstitutionAdmins } = await import('../../registry/frontend/admins_cityRegistryAdminsApi');
    const { assertPasskey } = await import('./authentication_passkeyClient');
    const auth = { access_token: 'test-session' } as never;
    const input = { cid_number: 'target-fixture-cid', admins: [{ account_id: 'fixture-account', family_name: '测', given_name: '试' }], actor_role_code: 'REGISTRY-ROLE' };
    const fetcher = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(JSON.stringify({ code: 0, data: {} })));
    try {
      vi.mocked(assertPasskey).mockClear();
      await prepareRegisterInstitutionAdmins(auth, input);
      expect(assertPasskey).toHaveBeenCalledTimes(1);
      expect(fetcher).toHaveBeenCalledTimes(1);
      const [path, init] = fetcher.mock.calls[0];
      expect(path).toBe('/api/admin/institution/admins/register/prepare');
      expect(init?.method).toBe('POST');
      expect(new Headers(init?.headers).get('authorization')).toBe('Bearer test-session');
      expect(new Headers(init?.headers).get(PASSKEY_ASSERTION_HEADER)).toBe('fake-passkey-assertion');
      expect(JSON.parse(String(init?.body))).toEqual(input);
    } finally { fetcher.mockRestore(); }
  });
});


// 编译后的独立验收：必须显式传入本轮模块产物，普通单元测试不要求程序已构建。
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
  const bindings: Record<string, unknown> = { ReactRuntime: React, ReactDOMRuntime: ReactDOM, JSXRuntime: JSX, JSXDevRuntime: JSXDev, AntdRuntime: antd, IconRuntime: icons, dayjs };
  // 只从宿主真实公开导入声明取得名称，不另维护SDK清单，不导入main.tsx启动页面。
  const main = readFileSync(join(import.meta.dirname, 'main.tsx'), 'utf8');
  const sources = import.meta.glob<Record<string, unknown>>('./*.{ts,tsx}');
  for (const match of main.matchAll(/import \* as Shared_([A-Za-z0-9_]+) from ["'](\.\/[^"']+)["']/g)) {
    const load = sources[match[2] + '.tsx'] ?? sources[match[2] + '.ts'];
    if (!load) throw new Error('宿主公开导入不存在：' + match[1]);
    bindings['Shared_' + match[1]] = await load();
  }
  const initializer = main.match(/globalThis\.OnChinaPlatform = (Object\.freeze\(\{[\s\S]*?\}\));/);
  if (!initializer) throw new Error('宿主公开对象初始化声明不存在');
  const sdk = runInNewContext(initializer[1], bindings) as Readonly<Record<string, unknown>>;
  const { AuthProvider } = await import('./authentication_AuthContext');
  const { EMPTY_CAPABILITIES } = await import('./platform_capabilityMap');
  const layout = JSON.parse(readFileSync(join(import.meta.dirname, '../modules/layout.fixture.json'), 'utf8')) as {
    capabilityCases: { code: string; owner: string | null; kind: string; flags: string[]; sections: unknown[] }[];
  };
  const assets: import('./platform_capabilityMap').ModuleAsset[] = [];
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
  const sharedModules: Record<string, import('./platform_capabilityMap').LoadedModule> = Object.create(null);
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
    const loader = await import('./platform_capabilityMap');
    await loader.initializeModules();
    for (const asset of assets) {
      for (const name of asset.exports) expect(typeof loader.moduleView(asset.id, name)).toBe('function');
      const golden = layout.capabilityCases.find(item => item.owner === asset.id);
      if (!golden) throw new Error('缺少模块工作台金标');
      const capabilities = { ...EMPTY_CAPABILITIES };
      for (const flag of golden.flags) capabilities[flag as keyof typeof capabilities] = true;
      const auth = { access_token: 'fixture-session', account_id: 'fixture-account', institution_cid_number: 'fixture-institution', institution_code: golden.code,
        family_name: '测', given_name: '试', capabilities,
        workspace: { workspace_kind: golden.kind, workspace_title: 'fixture', workspace_sections: golden.sections, workspace_modules: [] } };
      stored = JSON.stringify(auth);
      const View = loader.moduleView<Record<string, unknown>>(asset.id, asset.workspace ?? asset.exports[0]);
      const html = renderToString(React.createElement(AuthProvider, null,
        React.createElement(View, { auth, capabilities, passkeyRegistered: true, cidMeta: null, setCidMeta: () => {} })));
      expect(typeof html).toBe('string');
      expect(sandbox.OnChinaPlatform.react).toBe(React);
      expect(sandbox.OnChinaPlatform.platform_authentication_AuthContext).toBe(bindings.Shared_authentication_AuthContext);
    }
  } finally { fetcher.mockRestore(); vi.unstubAllGlobals(); }
}, 60000);
