// 所属业务前端回归，公共测试辅助只有一份。
import { describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, join, normalize } from 'node:path';
import { SECURITY_GRANT_HEADER, PASSKEY_ASSERTION_HEADER, passkeySubmitHeaders, securityGrantSubmitHeaders, commitAdminAction, rustConst } from '@gmb/onchina-platform/testing';

describe('实际业务认证', () => {
const auth = { accessToken: 'test-session', accountId: '0x' + '11'.repeat(32) } as never;
it('公民编辑和资料上传删除均携带一次性断言', async () => {
    const api = await import('../citizens/api');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
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
});
describe('实际业务认证', () => {
const auth = { accessToken: 'test-session', accountId: '0x' + '11'.repeat(32) } as never;
it('公民编辑保留原URL、JSON字段及一次认证，取消时不发送写请求', async () => {
    const { editCitizen } = await import('../citizens/api');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const fetcher = vi.spyOn(globalThis, 'fetch');
    const payload = { familyName: '测', givenName: '试', cityCode: '001' } as never;
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
});

// 使用本仓显式交付的DOM运行库验证真实页面，扫码签名只替换设备边界。
describe('公民详情布局与绑定钱包授权', () => {
  const browserTest = process.env.ONCHINA_TEST_DOM ? it : it.skip;
  const account = (byte: string) => '0x' + byte.repeat(32);
  async function mount({ failedBinding = false, inactiveBinding = false } = {}) {
    vi.resetModules();
    const { createRequire } = await import('node:module');
    const { JSDOM } = createRequire(import.meta.url)(process.env.ONCHINA_TEST_DOM!);
    const dom = new JSDOM('<div id="root"></div>', { url: 'https://onchina.example.invalid/', pretendToBeVisual: true });
    const computedStyle = dom.window.getComputedStyle.bind(dom.window);
    // jsdom不计算伪元素布局；此处只验证DOM结构与交互，像素布局由浏览器复核。
    dom.window.getComputedStyle = (element: Element) => computedStyle(element);
    dom.window.matchMedia = () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {} });
    for (const name of ['window', 'document', 'navigator', 'HTMLElement', 'HTMLBodyElement', 'Element', 'ShadowRoot', 'SVGElement', 'getComputedStyle']) vi.stubGlobal(name, dom.window[name]);
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    const React = await import('react');
    const assertion = vi.fn(async () => 'fixture-assertion');
    const adminSign = vi.fn(async () => ({ accountId: account('44'), signature: 'fixture-admin-signature' }));
    const walletSign = vi.fn(async () => ({ accountId: account('55'), signature: 'fixture-wallet-signature' }));
    const submit = vi.fn(async () => ({ txHash: 'fixture-transaction' }));
    const error = vi.fn();
    vi.doMock('@gmb/onchina-platform/authentication/passkeyClient', () => ({ assertPasskey: assertion, PASSKEY_ASSERTION_HEADER: 'x-passkey-assertion' }));
    vi.doMock('@gmb/onchina-platform/qr/useChainSign', () => ({ useChainSign: (title: string) => ({ signChain: title.startsWith('请新钱包') ? walletSign : adminSign, chainSignModal: null }), submitChainSign: submit }));
    vi.doMock('@gmb/onchina-platform/controls/notice', () => ({ notice: { success: vi.fn(), error } }));
    vi.doMock('../citizens/EditCitizenModal', () => ({ EditCitizenModal: () => null }));
    vi.doMock('@gmb/onchina-platform/qr/CitizenSignatureModal', () => ({ CitizenSignatureModal: (props: { open: boolean; onDetected: (value: string) => void }) => props.open ? React.createElement('button', { onClick: () => props.onDetected('fixture-citizen-response') }, '模拟公民回签') : null }));
    let reads = 0;
    const response = (data: unknown) => new Response(JSON.stringify({ code: 0, data }));
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async (url, init) => {
      const path = String(url);
      if (path.endsWith('/binding')) {
        reads++;
        if (reads > 1 && failedBinding) return new Response('unavailable', { status: 503 });
        return response({ bindingActive: !(reads > 1 && inactiveBinding), accountId: reads === 1 ? account('22') : account('33'), bindingRevision: reads, finalizedBlockNumber: reads, finalizedBlockHash: 'fixture-block' });
      }
      if (path.endsWith('/documents')) return response([]);
      if (path.endsWith('/onchain/prepare')) return response({ ...JSON.parse(String(init?.body)), signRequest: 'fixture-citizen-request', actionLabelZh: '公民授权' });
      if (path.endsWith('/onchain/complete')) return response({ requestId: 'fixture-admin-request', identitySignRequest: 'fixture-admin-code' });
      if (path.endsWith('/rebind/prepare')) return response({ requestId: 'fixture-rebind-request', walletSignRequest: 'fixture-new-wallet-code' });
      if (path.endsWith('/rebind/submit')) return response({ requestId: 'fixture-rebind-admin', signRequest: 'fixture-rebind-admin-code' });
      throw new Error('unexpected request: ' + path);
    });
    const auth = { setupRequired: false, accessToken: 'fixture-session', accountId: account('44'), institutionCidNumber: 'fixture-registry', institutionCode: 'FRG' } as never;
    const citizen = { cidNumber: 'fixture-citizen', accountId: account('22'), bindingRevision: 1, citizenBirthDate: '1990-01-01', citizenStatus: 'NORMAL', identityStatus: 'NORMAL', votingEligible: true } as never;
    const { createRoot } = await import('react-dom/client');
    const { CitizenDetailPage } = await import('../citizens/CitizenDetailPage');
    const root = createRoot(document.getElementById('root')!);
    const flush = () => React.act(async () => { await new Promise(resolve => setTimeout(resolve, 40)); });
    await React.act(async () => root.render(React.createElement(CitizenDetailPage, { auth, citizen, canWrite: true, onBack: vi.fn(), onUpdated: vi.fn() })));
    await flush();
    const button = (text: string) => [...document.querySelectorAll('button')].find(node => node.textContent?.replace(/\s/g, '') === text) as HTMLButtonElement | undefined;
    const click = async (text: string) => { expect(button(text), text).toBeTruthy(); await React.act(async () => button(text)!.click()); await flush(); };
    const writes = () => fetcher.mock.calls.filter(([, init]) => init?.method === 'POST');
    return { assertion, adminSign, walletSign, submit, fetcher, error, button, click, writes, dispose: async () => {
      await React.act(async () => root.unmount());
      fetcher.mockRestore();
      for (const path of ['@gmb/onchina-platform/authentication/passkeyClient', '@gmb/onchina-platform/qr/useChainSign', '@gmb/onchina-platform/controls/notice', '../citizens/EditCitizenModal', '@gmb/onchina-platform/qr/CitizenSignatureModal']) vi.doUnmock(path);
      vi.unstubAllGlobals(); dom.window.close(); vi.resetModules();
    } };
  }
  browserTest('名称列统一120px，按钮各有一个入口，无岗位控件和岗位预查询', async () => {
    const c = await mount();
    try {
      const tables = document.querySelectorAll('.ant-descriptions table');
      expect(tables).toHaveLength(2);
      for (const label of document.querySelectorAll<HTMLElement>('.ant-descriptions-item-label')) {
        expect(label.style.width).toBe('120px');
        expect(label.style.minWidth).toBe('120px');
        expect(label.style.maxWidth).toBe('120px');
      }
      const header = document.querySelector('.ant-card-head-title')!;
      const actions = [...header.querySelectorAll('button')].map(node => node.textContent?.replace(/\s/g, ''));
      expect(actions).toEqual(['返回', '吊销身份', '编辑资料']);
      expect(c.button('吊销身份')?.className).toContain('ant-btn-dangerous');
      expect(document.body.textContent).not.toContain('墓碑');
      expect(c.button('扫描钱包')).toBeUndefined();
      expect(document.querySelector('input[placeholder*="公民账户"]')).toBeNull();
      expect(c.button('更换')?.closest('tr')?.querySelector('th')?.textContent).toBe('绑定钱包账户');
      expect(c.button('换绑钱包')).toBeUndefined();
      expect(document.body.textContent).not.toContain('办理岗位');
      expect(document.body.textContent).not.toContain('换绑钱包账户');
      expect(document.querySelector('input[placeholder*="岗位"]')).toBeNull();
      expect(c.fetcher.mock.calls.filter(([url]) => String(url).includes('/operation-roles?'))).toHaveLength(0);
      expect(c.writes()).toHaveLength(0);
    } finally { await c.dispose(); }
  }, 15000);
  browserTest('更换先确认，取消不触发Passkey、写请求或钱包签名', async () => {
    const c = await mount();
    try {
      await c.click('更换');
      expect(document.body.textContent).toContain('确认更换绑定钱包账户');
      expect(c.writes()).toHaveLength(0);
      expect(c.assertion).not.toHaveBeenCalled();
      await c.click('取消');
      expect(c.writes()).toHaveLength(0);
      expect(c.assertion).not.toHaveBeenCalled();
      expect(c.walletSign).not.toHaveBeenCalled();
      expect(c.adminSign).not.toHaveBeenCalled();
    } finally { await c.dispose(); }
  }, 15000);
  browserTest('确认更换沿用一次Passkey、新钱包签名和管理员冷签，成功后重新读取绑定', async () => {
    const c = await mount();
    try {
      await c.click('更换'); await c.click('确认更换');
      expect(c.assertion).toHaveBeenCalledTimes(1);
      expect(c.walletSign).toHaveBeenCalledTimes(1);
      expect(c.adminSign).toHaveBeenCalledTimes(1);
      expect(c.submit).toHaveBeenCalledTimes(1);
      expect(c.writes().map(([url]) => url)).toEqual(['/api/admin/citizens/rebind/prepare', '/api/admin/citizens/rebind/submit']);
      expect(JSON.parse(String(c.writes()[0][1]?.body))).toEqual({ cidNumber: 'fixture-citizen' });
      expect(c.fetcher.mock.calls.filter(([url]) => String(url).endsWith('/binding'))).toHaveLength(2);
      expect(c.error).not.toHaveBeenCalled();
    } finally { await c.dispose(); }
  }, 15000);
  browserTest('公民授权采用点击时最新绑定账户，回签仍使用准备阶段账户且不传岗位', async () => {
    const c = await mount();
    try {
      await c.click('生成签名二维码');
      const prepare = c.writes()[0];
      expect(prepare[0]).toBe('/api/admin/citizens/fixture-citizen/onchain/prepare');
      expect(JSON.parse(String(prepare[1]?.body))).toEqual({ accountId: account('33'), identityLevel: 'voting' });
      expect(c.assertion).not.toHaveBeenCalled();
      await c.click('模拟公民回签');
      expect(c.assertion).toHaveBeenCalledTimes(1);
      expect(JSON.parse(String(c.writes()[1][1]?.body))).toEqual({ accountId: account('33'), identityLevel: 'voting', signResponse: 'fixture-citizen-response' });
      expect(c.adminSign).toHaveBeenCalledTimes(1);
      expect(c.walletSign).not.toHaveBeenCalled();
      expect(c.submit).toHaveBeenCalledTimes(1);
      expect(c.error).not.toHaveBeenCalled();
    } finally { await c.dispose(); }
  }, 15000);
  for (const options of [{ failedBinding: true }, { inactiveBinding: true }]) {
    browserTest(`最新绑定${options.failedBinding ? '读取失败' : '无效'}时不生成授权、不回退旧账户`, async () => {
      const c = await mount(options);
      try {
        await c.click('生成签名二维码');
        expect(c.writes()).toHaveLength(0);
        expect(c.assertion).not.toHaveBeenCalled();
        expect(c.button('模拟公民回签')).toBeUndefined();
        expect(c.error).toHaveBeenCalledTimes(1);
      } finally { await c.dispose(); }
    }, 15000);
  }
});
