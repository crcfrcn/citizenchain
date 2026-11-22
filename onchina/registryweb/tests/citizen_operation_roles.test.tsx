// 验证公民办理界面仅消费后端确定的业务岗位授权。

import '@gmb/onchina-platform/testing';
import { describe, it, expect, vi } from 'vitest';

// 设备认证只替换浏览器边界；公民API请求、头部和错误传播使用真实实现。
describe('公民业务后端自动授权的前端契约', () => {
  it('五项办理不读取候选、不发送岗位，原Passkey时点和次数保持', async () => {
    const api = await import('../citizens/api');
    const { assertPasskey, PASSKEY_ASSERTION_HEADER } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const auth = { accessToken: 'fixture-session' } as never;
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => new Response(JSON.stringify({ code: 0, data: {} })));
    try {
      for (const item of [
        { run: () => api.prepareCitizenOnchainSignature(auth, 'cid', 'wallet', 'voting'), path: '/api/admin/citizens/cid/onchain/prepare', body: { accountId: 'wallet', identityLevel: 'voting' }, passkey: false },
        { run: () => api.prepareCitizenOnchainSignature(auth, 'cid', 'wallet', 'candidate'), path: '/api/admin/citizens/cid/onchain/prepare', body: { accountId: 'wallet', identityLevel: 'candidate' }, passkey: false },
        { run: () => api.completeCitizenOnchainSignature(auth, 'cid', 'wallet', 'voting', 'response'), path: '/api/admin/citizens/cid/onchain/complete', body: { accountId: 'wallet', identityLevel: 'voting', signResponse: 'response' }, passkey: true },
        { run: () => api.prepareCitizenOccupy(auth, { cidType: 'CTZN' }), path: '/api/admin/citizens', body: { cidType: 'CTZN' }, passkey: true },
        { run: () => api.prepareCitizenRebind(auth, 'cid'), path: '/api/admin/citizens/rebind/prepare', body: { cidNumber: 'cid' }, passkey: true },
        { run: () => api.prepareCitizenRevoke(auth, 'cid'), path: '/api/admin/citizens/cid/onchain/revoke/prepare', body: {}, passkey: true },
      ]) {
        fetcher.mockClear(); vi.mocked(assertPasskey).mockClear();
        await item.run();
        expect(fetcher).toHaveBeenCalledTimes(1);
        const [path, init] = fetcher.mock.calls[0];
        expect(path).toBe(item.path);
        expect(init?.method).toBe('POST');
        const payload = JSON.parse(String(init?.body));
        expect(payload).toEqual(item.body);
        expect(payload).not.toHaveProperty('actorRoleCode');
        const headers = new Headers(init?.headers);
        expect(headers.get('authorization')).toBe('Bearer fixture-session');
        expect(headers.has(PASSKEY_ASSERTION_HEADER)).toBe(item.passkey);
        expect(assertPasskey).toHaveBeenCalledTimes(item.passkey ? 1 : 0);
      }
    } finally { fetcher.mockRestore(); vi.mocked(assertPasskey).mockClear(); }
  });
  it('后端拒绝权限时不重试或另查岗位，认证取消不发业务请求', async () => {
    const api = await import('../citizens/api');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const auth = { accessToken: 'fixture-session' } as never;
    const fetcher = vi.spyOn(globalThis, 'fetch');
    try {
      fetcher.mockResolvedValueOnce(new Response(JSON.stringify({ code: 1003, message: '当前管理员无此业务办理权限' }), { status: 403 }));
      await expect(api.prepareCitizenRebind(auth, 'cid')).rejects.toThrow('当前管理员无此业务办理权限');
      expect(fetcher).toHaveBeenCalledTimes(1);
      expect(fetcher.mock.calls[0][0]).not.toContain('operation-roles');
      fetcher.mockClear();
      vi.mocked(assertPasskey).mockRejectedValueOnce(new Error('cancelled'));
      await expect(api.prepareCitizenRevoke(auth, 'cid')).rejects.toThrow('cancelled');
      expect(fetcher).not.toHaveBeenCalled();
    } finally { fetcher.mockRestore(); vi.mocked(assertPasskey).mockClear(); }
  });
});

const browserTest = process.env.ONCHINA_TEST_DOM ? it : it.skip;
browserTest('新增公民不显示岗位输入，取消无认证，占号保留公民签名及管理员冷签', async () => {
  vi.resetModules();
  const { createRequire } = await import('node:module');
  const { JSDOM } = createRequire(import.meta.url)(process.env.ONCHINA_TEST_DOM!);
  const dom = new JSDOM('<div id="root"></div>', { url: 'https://onchina.example.invalid/' });
  const computed = dom.window.getComputedStyle.bind(dom.window);
  dom.window.getComputedStyle = (element: Element) => computed(element);
  dom.window.matchMedia = () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {} });
  for (const name of ['window', 'document', 'navigator', 'HTMLElement', 'HTMLBodyElement', 'Element', 'ShadowRoot', 'SVGElement', 'getComputedStyle']) vi.stubGlobal(name, dom.window[name]);
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  const React = await import('react');
  const { createRoot } = await import('react-dom/client');
  const assertion = vi.fn(async () => 'fixture-assertion');
  const citizenSign = vi.fn(async () => ({ accountId: 'fixture-citizen-account', signature: 'fixture-citizen-signature' }));
  const adminSign = vi.fn(async () => ({ accountId: 'fixture-admin-account', signature: 'fixture-admin-signature' }));
  const submit = vi.fn(async () => ({ txHash: 'fixture-transaction', citizen: { cidNumber: 'fixture-cid' } }));
  const onClose = vi.fn(), onCreated = vi.fn(), error = vi.fn();
  vi.doMock('@gmb/onchina-platform/authentication/passkeyClient', () => ({ assertPasskey: assertion, PASSKEY_ASSERTION_HEADER: 'x-passkey-assertion' }));
  vi.doMock('@gmb/onchina-platform/qr/useChainSign', () => ({ useChainSign: (title: string) => ({ signChain: title.startsWith('请公民本人') ? citizenSign : adminSign, chainSignModal: null }), submitChainSign: submit }));
  vi.doMock('@gmb/onchina-platform/controls/notice', () => ({ notice: { success: vi.fn(), error } }));
  const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async (url) => {
    if (String(url) === '/api/admin/citizens') return new Response(JSON.stringify({ code: 0, data: { requestId: 'fixture-occupy', citizenSignRequest: 'fixture-occupy-code' } }));
    if (String(url) === '/api/admin/citizens/occupy/submit') return new Response(JSON.stringify({ code: 0, data: { requestId: 'fixture-admin', signRequest: 'fixture-admin-code' } }));
    throw new Error('unexpected request: ' + url);
  });
  const root = createRoot(document.getElementById('root')!);
  try {
    const { CitizenCreateModal } = await import('../citizens/CitizenCreateModal');
    const auth = { accessToken: 'fixture-session', accountId: 'fixture-admin' } as never;
    await React.act(async () => root.render(React.createElement(CitizenCreateModal, { auth, open: true, provinceName: '测试省', cityName: '测试市', onClose, onCreated })));
    const click = async (text: string) => { const button = [...document.querySelectorAll('button')].find(node => node.textContent?.replace(/\s/g, '') === text); expect(button).toBeTruthy(); await React.act(async () => button!.click()); };
    expect(document.body.textContent).not.toContain('岗位');
    expect(document.querySelector('input[placeholder*=岗位]')).toBeNull();
    expect(fetcher).not.toHaveBeenCalled();
    await click('取消');
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(assertion).not.toHaveBeenCalled();
    expect(fetcher).not.toHaveBeenCalled();
    await click('发起占号');
    await React.act(async () => { await new Promise(resolve => setTimeout(resolve, 80)); });
    expect(assertion).toHaveBeenCalledTimes(1);
    expect(citizenSign).toHaveBeenCalledTimes(1);
    expect(adminSign).toHaveBeenCalledTimes(1);
    expect(submit).toHaveBeenCalledTimes(1);
    expect(onCreated).toHaveBeenCalledWith('fixture-cid');
    expect(JSON.parse(String(fetcher.mock.calls[0][1]?.body))).toEqual({ cidType: 'CTZN' });
    expect(error).not.toHaveBeenCalled();
  } finally {
    await React.act(async () => root.unmount());
    fetcher.mockRestore();
    for (const path of ['@gmb/onchina-platform/authentication/passkeyClient', '@gmb/onchina-platform/qr/useChainSign', '@gmb/onchina-platform/controls/notice']) vi.doUnmock(path);
    vi.unstubAllGlobals(); dom.window.close(); vi.resetModules();
  }
}, 15000);
