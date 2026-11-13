// 所属业务前端回归，公共测试辅助只有一份。
import { describe, expect, it, vi } from 'vitest';

describe('模块前端装载与重启边界', () => {
  type Asset = import('../workspace/modules').ModuleAsset;
  type ElementFixture = { tag: string; src?: string; href?: string; onload?: ((event: Event) => unknown) | null; onerror?: ((event: Event) => unknown) | null; removed: boolean; remove: () => void };
  const asset = (id: string, codes = ['FRG']): Asset => ({
    id, version: '1.0.0', interface_version: 2, script: '/modules/' + id + '/module.js?v=1.0.0',
    stylesheet: '/modules/' + id + '/module.css?v=1.0.0', exports: ['Workspace'], institution_codes: codes, workspace: 'Workspace',
  });
  async function fixture(items: Asset[], options: { badVersion?: string; badStyle?: string; badScript?: string; pendingScript?: string; pendingHealth?: boolean } = {}) {
    vi.resetModules();
    const elements: ElementFixture[] = [];
    const views = Object.fromEntries(items.map(item => [item.id, () => null]));
    const modules = Object.fromEntries(items.map(item => [item.id, {
      Workspace: views[item.id], __onchina: { id: item.id, version: options.badVersion === item.id ? '2.0.0' : item.version, interface_version: 2 },
    }]));
    vi.stubGlobal('window', { location: { href: 'https://onchina.crcfrcn.com/', origin: 'https://onchina.crcfrcn.com' } });
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
      const wire = items.map(({interface_version, institution_codes, ...rest}) => ({
        ...rest, interfaceVersion: interface_version, institutionCodes: institution_codes,
      }));
      return new Response(JSON.stringify({ code: 0, data: { modules: wire } }));
    });
    const loader = await import('../workspace/modules');
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
      globalThis.OnChinaModules.slow = { Workspace: () => null, __onchina: { id: 'slow', version: '1.0.0', interface_version: 2 } };
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
      const auth = { institutionCode: 'FRG', workspace: { workspace_modules: ['instance'] } } as never;
      expect(context.loader.workspaceView(auth)).toBe(context.views.instance);
      expect(() => context.loader.workspaceView({ institutionCode: 'FRG' } as never)).toThrow('归属冲突');
      expect(context.loader.workspaceView({ institutionCode: 'UNKNOWN' } as never)).toBeNull();
    } finally { context.dispose(); }
  });
  it('模块落点依据已装载资源和扫码后的机构授权', async () => {
    const context = await fixture([asset('registry'), asset('legislation', ['NLG']), asset('private', ['SFGF'])]);
    try {
      await context.loader.initializeModules();
      const registry = { institutionCode:'FRG', workspace:{workspace_kind:'registry',workspace_modules:[]} } as never;
      const privateInstitution = { institutionCode:'SFGF', workspace:{workspace_kind:'private',workspace_modules:['private']} } as never;
      const legislature = { institutionCode:'NLG', workspace:{workspace_kind:'legislation',workspace_modules:[]}, capabilities:{canViewLegislation:true} } as never;
      expect(context.loader.resolveModuleLanding(registry, 'registry')).toBe('workspace');
      expect(context.loader.resolveModuleLanding(registry, 'private')).toBe('forbidden');
      expect(context.loader.resolveModuleLanding(privateInstitution, 'private')).toBe('workspace');
      expect(context.loader.resolveModuleLanding(legislature, 'legislation')).toBe('legislation');
      expect(context.loader.resolveModuleLanding(registry, 'legislation')).toBe('forbidden');
      expect(context.loader.resolveModuleLanding(registry, 'judicial')).toBe('unavailable');
      expect(context.loader.resolveModuleLanding(registry, 'https://other.example/')).toBe('unavailable');
    } finally { context.dispose(); }
  });

});

// 浏览器生命周期验收由调用方显式交付已安装的 DOM 测试运行库，不下载或新增产品依赖。
describe('当前会话的密钥状态与工作台首屏', () => {
  const browserTest = process.env.ONCHINA_TEST_DOM ? it : it.skip;
  async function browser() {
    vi.resetModules();
    const { createRequire } = await import('node:module');
    const { isAbsolute } = await import('node:path');
    if (!isAbsolute(process.env.ONCHINA_TEST_DOM!)) throw new Error('DOM测试运行库必须是显式绝对路径');
    const { JSDOM } = createRequire(import.meta.url)(process.env.ONCHINA_TEST_DOM!);
    const dom = new JSDOM('<div id="root"></div>', { url: 'https://onchina.example.invalid/' });
    for (const name of ['window', 'document', 'navigator', 'sessionStorage', 'HTMLElement', 'Element', 'ShadowRoot', 'SVGElement', 'getComputedStyle']) {
      vi.stubGlobal(name, dom.window[name]);
    }
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.doMock('../controls/notice', () => ({ notice: { success: vi.fn(), error: vi.fn(), warning: vi.fn() } }));
    const React = await import('react');
    const { createRoot } = await import('react-dom/client');
    const root = createRoot(document.getElementById('root')!);
    const auth = { setupRequired: false, accessToken: 'fixture-session', accountId: 'fixture-account', institutionCidNumber: 'fixture-institution',
      institutionCode: 'FRG', familyName: '测', givenName: '试', workspace: { workspace_kind: 'registry' } } as import('../authentication/types').AdminAuth;
    return { React, root, auth, dispose: async () => {
      await React.act(async () => root.unmount());
      dom.window.close();
      vi.doUnmock('../controls/notice');
      vi.doUnmock('../authentication/LoginView');
      vi.unstubAllGlobals();
    } };
  }
  const response = (registered: boolean) => new Response(JSON.stringify({ code: 0, data: { registered } }));

  browserTest('登录快照无需密钥请求；换会话丢弃迟到刷新，首次设置同步全部入口', async () => {
    const context = await browser();
    const { React, root, auth } = context;
    const { AuthProvider } = await import('../authentication/Context');
    const { useAuth } = await import('../authentication/useAuth');
    const { usePasskeyRegistration } = await import('../authentication/usePasskey');
    const passkeys = await import('../authentication/passkeyClient');
    const register = vi.spyOn(passkeys, 'registerPasskey').mockResolvedValue();
    let snapshot!: ReturnType<typeof useAuth>;
    let registration!: ReturnType<typeof usePasskeyRegistration>;
    function Consumer() {
      snapshot = useAuth(); registration = usePasskeyRegistration();
      return React.createElement('span', null, snapshot.passkeyState.status);
    }
    const pending: ((value: Response) => void)[] = [];
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async path => {
      expect(path).toBe('/api/admin/auth/passkey/status');
      return new Promise<Response>(resolve => pending.push(resolve));
    });
    sessionStorage.setItem('onchina_admin_auth', JSON.stringify(auth));
    try {
      await React.act(async () => root.render(React.createElement(AuthProvider, null, React.createElement(Consumer))));
      expect(snapshot.passkeyState).toEqual({ status: 'ready', registered: true });
      expect(fetcher).not.toHaveBeenCalled();
      let previous!: Promise<void>;
      await React.act(async () => { previous = snapshot.refreshPasskey(); });
      await React.act(async () => snapshot.setAuth({ ...auth, accessToken: 'fixture-next-session', institutionCidNumber: 'fixture-next-institution', setupRequired: true }));
      await React.act(async () => { pending[0](response(true)); await previous; });
      expect(snapshot.passkeyState).toEqual({ status: 'ready', registered: false });
      expect(registration.registered).toBe(false);
      await React.act(async () => {
        const action = registration.register();
        await vi.waitFor(() => expect(pending).toHaveLength(2));
        pending[1](response(true)); await action;
      });
      expect(register).toHaveBeenCalledTimes(1);
      expect(snapshot.auth?.setupRequired).toBe(false);
      expect(snapshot.passkeyState).toEqual({ status: 'ready', registered: true });
      expect(registration.registered).toBe(true);
      expect(fetcher).toHaveBeenCalledTimes(2);
      await React.act(async () => snapshot.logout());
      expect(snapshot.passkeyState.status).toBe('loading');
    } finally { register.mockRestore(); fetcher.mockRestore(); await context.dispose(); }
  });

  browserTest('未设置密钥不能进入业务工作台；设置后刷新失败可恢复且不重复注册', async () => {
    const context = await browser();
    const { React, root, auth } = context;
    const { useAuth } = await import('../authentication/useAuth');
    const passkeys = await import('../authentication/passkeyClient');
    const register = vi.spyOn(passkeys, 'registerPasskey').mockResolvedValue();
    let workspaceRenders = 0;
    const loader = await import('../workspace/modules');
    const workspace = vi.spyOn(loader, 'workspaceView').mockReturnValue((() => {
      workspaceRenders++; return React.createElement('div', null, 'fixture-workspace');
    }) as never);
    vi.doMock('../authentication/LoginView', () => ({ LoginView: () => {
      const { setAuth } = useAuth();
      return React.createElement('button', { onClick: () => setAuth({ ...auth, setupRequired: true }) }, 'fixture-login');
    } }));
    const pending: ((value: Response) => void)[] = [];
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async path => {
      if (path === '/api/health') return new Response(JSON.stringify({ code: 0, data: { modules: [] } }));
      expect(path).toBe('/api/admin/auth/passkey/status');
      return new Promise<Response>(resolve => pending.push(resolve));
    });
    const button = (name: string) => [...document.querySelectorAll('button')].find(button => button.textContent?.replace(/\s/g, '') === name)!;
    try {
      const { default: App } = await import('../workspace/App');
      await React.act(async () => root.render(React.createElement(App)));
      await React.act(async () => button('fixture-login').click());
      expect(document.body.textContent).toContain('请先设置 Passkey 密钥');
      expect(pending).toHaveLength(0); expect(workspaceRenders).toBe(0);
      await React.act(async () => button('设置密钥').click());
      expect(workspaceRenders).toBe(0);
      await React.act(async () => pending[0](new Response('unavailable', { status: 503 })));
      expect(workspaceRenders).toBe(0);
      await React.act(async () => button('刷新状态').click());
      await React.act(async () => pending[1](response(true)));
      expect(document.body.textContent).toContain('fixture-workspace');
      expect(register).toHaveBeenCalledTimes(1);
    } finally { register.mockRestore(); fetcher.mockRestore(); workspace.mockRestore(); await context.dispose(); }
  });
});
