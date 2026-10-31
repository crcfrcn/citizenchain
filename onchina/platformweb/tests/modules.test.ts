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
      return new Response(JSON.stringify({ code: 0, data: { modules: items } }));
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
});
