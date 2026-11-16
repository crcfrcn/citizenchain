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


// 真实页面与AntD交互；只替换设备扫码/签名边界，业务请求使用原API。
describe('公民身份行、绑定钱包与不可逆升级', () => {
  const browserTest = process.env.ONCHINA_TEST_DOM ? it : it.skip;
  type Level = 'none' | 'voting' | 'candidate';
  const account = (byte: string) => '0x' + byte.repeat(32);
  async function mount(options: {
    identity?: Level; failedBinding?: boolean; inactiveBinding?: boolean;
    initialFailure?: boolean; delayedFirst?: boolean; stableRevision?: boolean;
    canWrite?: boolean; underAge?: boolean; rejectAdmin?: boolean; conflictAt?: 'prepare' | 'complete';
  } = {}) {
    vi.resetModules();
    const { createRequire } = await import('node:module');
    const { JSDOM } = createRequire(import.meta.url)(process.env.ONCHINA_TEST_DOM!);
    const dom = new JSDOM('<div id="root"></div>', { url: 'https://onchina.example.invalid/', pretendToBeVisual: true });
    const computed = dom.window.getComputedStyle.bind(dom.window);
    // jsdom不计算伪元素尺寸，本组验证DOM结构、状态和真实交互，不伪造视觉测量。
    dom.window.getComputedStyle = (element: Element) => computed(element);
    dom.window.matchMedia = () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {} });
    for (const name of ['window', 'document', 'navigator', 'HTMLElement', 'HTMLBodyElement', 'Element', 'ShadowRoot', 'SVGElement', 'getComputedStyle']) vi.stubGlobal(name, dom.window[name]);
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    const React = await import('react');
    const assertion = vi.fn(async () => 'fixture-assertion');
    const adminSign = vi.fn(async () => {
      if (options.rejectAdmin) throw new Error('fixture-admin-cancelled');
      return { accountId: account('44'), signature: 'fixture-admin-signature' };
    });
    const walletSign = vi.fn(async () => ({ accountId: account('55'), signature: 'fixture-wallet-signature' }));
    let actual: Level = options.identity ?? 'none';
    let preparedLevel: 'voting' | 'candidate' | undefined;
    const submit = vi.fn(async () => { if (preparedLevel) actual = preparedLevel; return { txHash: 'fixture-transaction' }; });
    const error = vi.fn();
    vi.doMock('@gmb/onchina-platform/authentication/passkeyClient', () => ({ assertPasskey: assertion, PASSKEY_ASSERTION_HEADER: 'x-passkey-assertion' }));
    vi.doMock('@gmb/onchina-platform/qr/useChainSign', () => ({ useChainSign: (title: string) => ({ signChain: title.startsWith('请新钱包') ? walletSign : adminSign, chainSignModal: null }), submitChainSign: submit }));
    vi.doMock('@gmb/onchina-platform/controls/notice', () => ({ notice: { success: vi.fn(), error } }));
    vi.doMock('../citizens/EditCitizenModal', () => ({ EditCitizenModal: () => null }));
    vi.doMock('@gmb/onchina-platform/qr/CitizenSignatureModal', () => ({ CitizenSignatureModal: (props: { open: boolean; onDetected: (value: string) => void; onCancel: () => void }) => props.open ? React.createElement('div', null,
      React.createElement('button', { onClick: () => props.onDetected('fixture-citizen-response') }, '模拟公民回签'),
      React.createElement('button', { onClick: props.onCancel }, '取消公民授权')) : null }));
    let reads = 0;
    let initialFailure = options.initialFailure;
    let release!: (level: Level) => void;
    const response = (data: unknown) => new Response(JSON.stringify({ code: 0, data }));
    const binding = (cid: string, level: Level) => ({ cidNumber: cid, cidStatus: 'ACTIVE',
      bindingActive: !(reads > 1 && options.inactiveBinding), accountId: reads === 1 || options.stableRevision ? account('22') : account('33'),
      bindingRevision: options.stableRevision ? 1 : reads, votingIdentity: level !== 'none', candidateIdentity: level === 'candidate',
      finalizedBlockNumber: reads, finalizedBlockHash: 'fixture-block', genesisHash: 'fixture-genesis' });
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async (url, init) => {
      const path = String(url);
      if (path.endsWith('/binding')) {
        reads++;
        const cid = decodeURIComponent(path.split('/citizens/')[1].split('/binding')[0]);
        if (options.delayedFirst && reads === 1) return new Promise<Response>(resolve => { release = level => resolve(response(binding(cid, level))); });
        if (initialFailure || reads > 1 && options.failedBinding) { initialFailure = false; return new Response('unavailable', { status: 503 }); }
        return response(binding(cid, actual));
      }
      if (path.endsWith('/documents')) return response([]);
      if (path.endsWith('/onchain/prepare')) {
        const body = JSON.parse(String(init?.body)); preparedLevel = body.identityLevel;
        if (options.conflictAt === 'prepare') { actual = 'candidate'; return new Response(JSON.stringify({ code: 1005, message: '已是竞选身份，不能再次更换或签名' }), { status: 409 }); }
        return response({ ...body, signRequest: 'fixture-citizen-request', actionLabelZh: '公民授权' });
      }
      if (path.endsWith('/onchain/complete')) {
        if (options.conflictAt === 'complete') { actual = 'candidate'; return new Response(JSON.stringify({ code: 1005, message: '已是竞选身份，不能再次更换或签名' }), { status: 409 }); }
        return response({ requestId: 'fixture-admin-request', identitySignRequest: 'fixture-admin-code' });
      }
      if (path.endsWith('/rebind/prepare')) return response({ requestId: 'fixture-rebind-request', walletSignRequest: 'fixture-new-wallet-code' });
      if (path.endsWith('/rebind/submit')) return response({ requestId: 'fixture-rebind-admin', signRequest: 'fixture-rebind-admin-code' });
      throw new Error('unexpected request: ' + path);
    });
    const auth = { setupRequired: false, accessToken: 'fixture-session', accountId: account('44'), institutionCidNumber: 'fixture-registry', institutionCode: 'FRG' };
    const citizen = { cidNumber: 'fixture-citizen', accountId: account('22'), bindingRevision: 1, citizenBirthDate: options.underAge ? '2015-01-01' : '1990-01-01', citizenStatus: 'NORMAL', identityStatus: 'NORMAL', votingEligible: true };
    const onUpdated = vi.fn();
    const { createRoot } = await import('react-dom/client');
    const { CitizenDetailPage } = await import('../citizens/CitizenDetailPage');
    const root = createRoot(document.getElementById('root')!);
    const render = (cid = citizen.cidNumber) => React.act(async () => root.render(React.createElement(CitizenDetailPage, { auth: auth as never, citizen: { ...citizen, cidNumber: cid } as never, canWrite: options.canWrite !== false, onBack: vi.fn(), onUpdated })));
    const flush = () => React.act(async () => { await new Promise(resolve => setTimeout(resolve, 35)); });
    await render(); await flush();
    const row = (label: string) => [...document.querySelectorAll('.ant-descriptions-item-label')].find(node => node.textContent === label)!.closest('tr')!;
    const button = (text: string, label?: string) => [...(label ? row(label) : document).querySelectorAll('button')].find(node => node.textContent?.replace(/\s/g, '') === text) as HTMLButtonElement | undefined;
    const click = async (text: string, label?: string) => { expect(button(text, label), text).toBeTruthy(); await React.act(async () => button(text, label)!.click()); await flush(); };
    const openSelect = async () => {
      await React.act(async () => row('身份上链').querySelector('.ant-select-selector')!.dispatchEvent(new dom.window.MouseEvent('mousedown', { bubbles: true })));
      await flush();
    };
    const choose = async (text: '投票身份' | '竞选身份') => {
      await openSelect();
      const option = document.querySelector(`.ant-select-dropdown:not(.ant-select-dropdown-hidden) .ant-select-item-option[title="${text}"]`) as HTMLElement;
      expect(option).toBeTruthy(); expect(option.className).not.toContain('ant-select-item-option-disabled');
      await React.act(async () => option.click()); await flush();
    };
    const input = () => row('身份上链').querySelector('input') as HTMLInputElement;
    const writes = () => fetcher.mock.calls.filter(([, init]) => init?.method === 'POST');
    const action = () => row('身份上链').querySelector('button') as HTMLButtonElement;
    return { assertion, adminSign, walletSign, submit, fetcher, error, button, click, row, input, action, writes, choose, openSelect, flush, render, onUpdated,
      setIdentity: (level: Level) => { actual = level; }, release: (level: Level) => release(level),
      dispose: async () => {
        await React.act(async () => root.unmount()); fetcher.mockRestore();
        for (const path of ['@gmb/onchina-platform/authentication/passkeyClient', '@gmb/onchina-platform/qr/useChainSign', '@gmb/onchina-platform/controls/notice', '../citizens/EditCitizenModal', '@gmb/onchina-platform/qr/CitizenSignatureModal']) vi.doUnmock(path);
        vi.unstubAllGlobals(); dom.window.close(); vi.resetModules();
      } };
  }
  browserTest('身份行紧接绑定账户，120px名称列和原入口保留，未上链默认空选禁签', async () => {
    const c = await mount();
    try {
      expect(c.row('绑定钱包账户').nextElementSibling).toBe(c.row('身份上链'));
      expect(document.querySelectorAll('.ant-descriptions table')).toHaveLength(2);
      for (const label of document.querySelectorAll<HTMLElement>('.ant-descriptions-item-label')) expect(label.style.width).toBe('120px');
      // 检查布局合同；jsdom不进行像素排版，不将DOM样式断言冒充实屏测量。
      expect(document.querySelectorAll('.citizen-details .ant-descriptions-row')).toHaveLength(18);
      for (const cell of document.querySelectorAll<HTMLElement>('.citizen-details th, .citizen-details td')) {
        expect(cell.style.height).toBe('40px'); expect(cell.style.boxSizing).toBe('border-box');
        expect(cell.style.lineHeight).toBe('20px'); expect(cell.style.padding).toBe('8px 16px');
      }
      const select = c.row('身份上链').querySelector<HTMLElement>('.ant-select')!;
      expect(select.style.height).toBe('20px'); expect(select.style.width).toBe('100px');
      expect(select.className).toContain('ant-select-borderless');
      for (const label of ['绑定钱包账户', '身份上链']) {
        const action = c.row(label).querySelector<HTMLButtonElement>('button')!;
        expect(action.style.height).toBe('20px'); expect(action.style.padding).toBe('0px');
        expect(action.style.borderWidth).toBe('0px'); expect(action.querySelector('svg')).toBeNull();
      }
      const address = c.row('绑定钱包账户').querySelector<HTMLElement>('[title]')!;
      expect(address.title).toBe(address.textContent);
      expect(address.style.whiteSpace).toBe('nowrap'); expect(address.style.textOverflow).toBe('ellipsis');

      expect(c.row('身份上链').textContent).toContain('选择身份');
      expect(c.input().disabled).toBe(false); expect(c.action().disabled).toBe(true); expect(c.action().textContent).toContain('签名');
      expect([...document.querySelectorAll('.ant-card-head-title button')].map(node => node.textContent?.replace(/\s/g, ''))).toEqual(['返回', '吊销身份', '编辑资料']);
      expect(document.body.textContent).not.toContain('链上身份上链');
      expect(document.body.textContent).not.toContain('生成签名二维码');
      expect(document.body.textContent).not.toContain('办理岗位');
      expect(c.button('扫描钱包')).toBeUndefined();
      expect(c.fetcher.mock.calls.filter(([url]) => String(url).includes('/operation-roles'))).toHaveLength(0);
      expect(c.writes()).toHaveLength(0);
    } finally { await c.dispose(); }
  }, 15000);
  browserTest('钱包更换仍弹确认，取消不触发认证或写入', async () => {
    const c = await mount({ identity: 'voting' });
    try {
      await c.click('更换', '绑定钱包账户');
      expect(document.body.textContent).toContain('确认更换绑定钱包账户');
      await c.click('取消');
      expect(c.writes()).toHaveLength(0); expect(c.assertion).not.toHaveBeenCalled(); expect(c.walletSign).not.toHaveBeenCalled();
    } finally { await c.dispose(); }
  }, 15000);
  browserTest('钱包更换保留一次Passkey、新钱包和管理员签名，不受竞选身份锁定影响', async () => {
    const c = await mount({ identity: 'candidate' });
    try {
      await c.click('更换', '绑定钱包账户'); await c.click('确认更换');
      expect(c.assertion).toHaveBeenCalledTimes(1); expect(c.walletSign).toHaveBeenCalledTimes(1); expect(c.adminSign).toHaveBeenCalledTimes(1); expect(c.submit).toHaveBeenCalledTimes(1);
      expect(c.writes().map(([url]) => url)).toEqual(['/api/admin/citizens/rebind/prepare', '/api/admin/citizens/rebind/submit']);
      expect(JSON.parse(String(c.writes()[0][1]?.body))).toEqual({ cidNumber: 'fixture-citizen' });
      expect(c.action().disabled).toBe(true); expect(c.input().disabled).toBe(true);
    } finally { await c.dispose(); }
  }, 15000);
  for (const target of ['投票身份', '竞选身份'] as const) {
    browserTest(`未上链选择${target}后蓝签名，沿用最新账户、公民授权、一次密钥和管理员冷签`, async () => {
      const c = await mount({ stableRevision: true });
      try {
        await c.choose(target);
        expect(c.action().disabled).toBe(false); expect(c.action().style.color).toBe('rgb(22, 119, 255)');
        await c.click('签名', '身份上链');
        const level = target === '投票身份' ? 'voting' : 'candidate';
        expect(JSON.parse(String(c.writes()[0][1]?.body))).toEqual({ accountId: account('22'), identityLevel: level });
        expect(c.assertion).not.toHaveBeenCalled(); expect(c.action().disabled).toBe(true);
        await c.click('模拟公民回签');
        expect(c.assertion).toHaveBeenCalledTimes(1); expect(c.adminSign).toHaveBeenCalledTimes(1); expect(c.submit).toHaveBeenCalledTimes(1);
        expect(c.walletSign).not.toHaveBeenCalled();
        expect(JSON.parse(String(c.writes()[1][1]?.body))).toEqual({ accountId: account('22'), identityLevel: level, signResponse: 'fixture-citizen-response' });
        expect(c.row('身份上链').textContent).toContain(target);
        expect(c.input().disabled).toBe(true);
        expect(c.action().textContent).toContain(level === 'candidate' ? '签名' : '更换');
        expect(c.action().disabled).toBe(level === 'candidate');
        expect(c.row('绑定版本').textContent).toContain('1');
        expect(c.error).not.toHaveBeenCalled();
      } finally { await c.dispose(); }
    }, 15000);
  }
  browserTest('投票身份红更换只解锁，选竞选后才蓝签名，完成后永久锁定该控件', async () => {
    const c = await mount({ identity: 'voting', stableRevision: true });
    try {
      expect(c.input().disabled).toBe(true); expect(c.action().className).toContain('ant-btn-dangerous');
      const reads = c.fetcher.mock.calls.length;
      await c.click('更换', '身份上链');
      expect(c.fetcher).toHaveBeenCalledTimes(reads); expect(c.assertion).not.toHaveBeenCalled();
      expect(c.input().disabled).toBe(false); expect(c.action().disabled).toBe(true);
      expect(c.row('身份上链').textContent).toContain('投票身份');
      await c.openSelect();
      expect(document.querySelector('.ant-select-item-option[title="投票身份"]')?.className).toContain('ant-select-item-option-disabled');
      const option = document.querySelector('.ant-select-item-option[title="竞选身份"]') as HTMLElement;
      const React = await import('react'); await React.act(async () => option.click()); await c.flush();
      expect(c.action().disabled).toBe(false);
      await c.click('签名', '身份上链'); await c.click('模拟公民回签');
      expect(c.row('身份上链').textContent).toContain('竞选身份');
      expect(c.input().disabled).toBe(true); expect(c.action().disabled).toBe(true);
      expect(c.row('绑定版本').textContent).toContain('1');
      const count = c.writes().length; await c.click('签名', '身份上链');
      expect(c.writes()).toHaveLength(count); expect(c.assertion).toHaveBeenCalledTimes(1);
    } finally { await c.dispose(); }
  }, 15000);
  browserTest('同时有投票和竞选记录时优先竞选，进入即锁，不能签名', async () => {
    const c = await mount({ identity: 'candidate' });
    try {
      expect(c.row('身份上链').textContent).toContain('竞选身份');
      expect(c.input().disabled).toBe(true); expect(c.action().disabled).toBe(true);
      expect(c.action().textContent).toContain('签名');
      await c.click('签名', '身份上链');
      expect(c.writes()).toHaveLength(0); expect(c.assertion).not.toHaveBeenCalled();
    } finally { await c.dispose(); }
  }, 15000);
  browserTest('取消公民授权不变更投票身份，保持可重试的竞选草稿', async () => {
    const c = await mount({ identity: 'voting' });
    try {
      await c.click('更换', '身份上链'); await c.choose('竞选身份'); await c.click('签名', '身份上链'); await c.click('取消公民授权');
      expect(c.input().disabled).toBe(false); expect(c.action().disabled).toBe(false);
      expect(c.assertion).not.toHaveBeenCalled(); expect(c.submit).not.toHaveBeenCalled();
      await c.render('next-citizen'); await c.flush();
      expect(c.row('身份上链').textContent).toContain('投票身份'); expect(c.action().textContent).toContain('更换'); expect(c.input().disabled).toBe(true);
    } finally { await c.dispose(); }
  }, 15000);
  browserTest('管理员取消冷签不伪造竞选上链完成', async () => {
    const c = await mount({ identity: 'voting', rejectAdmin: true });
    try {
      await c.click('更换', '身份上链'); await c.choose('竞选身份'); await c.click('签名', '身份上链'); await c.click('模拟公民回签');
      expect(c.submit).not.toHaveBeenCalled(); expect(c.error).toHaveBeenCalledTimes(1);
      expect(c.input().disabled).toBe(false); expect(c.action().disabled).toBe(false);
    } finally { await c.dispose(); }
  }, 15000);
  for (const options of [{ failedBinding: true }, { inactiveBinding: true }]) {
    browserTest(`签名前最新绑定${options.failedBinding ? '读取失败' : '失效'}不能回退旧账户生成二维码`, async () => {
      const c = await mount(options);
      try {
        await c.choose('投票身份'); await c.click('签名', '身份上链');
        expect(c.writes()).toHaveLength(0); expect(c.assertion).not.toHaveBeenCalled(); expect(c.error).toHaveBeenCalledTimes(1);
        expect(c.action().disabled).toBe(true);
      } finally { await c.dispose(); }
    }, 15000);
  }
  browserTest('首次读取失败时禁用，重试恢复空选，不误认已登记或默认选投票', async () => {
    const c = await mount({ initialFailure: true });
    try {
      expect(c.action().disabled).toBe(true); expect(c.input().disabled).toBe(true);
      expect(document.body.textContent).toContain('读取失败');
      expect(c.row('身份上链').textContent).not.toContain('读取失败');
      expect(c.button('重试')!.closest('.citizen-details')).toBeNull();
      await c.click('重试');
      expect(c.input().disabled).toBe(false); expect(c.action().disabled).toBe(true); expect(c.row('身份上链').textContent).toContain('选择身份');
    } finally { await c.dispose(); }
  }, 15000);
  browserTest('读取未完成及切换公民后的迟到结果均不能开放或覆盖身份控件', async () => {
    const c = await mount({ delayedFirst: true });
    try {
      expect(c.input().disabled).toBe(true); expect(c.action().disabled).toBe(true);
      await c.render('next-citizen'); await c.flush();
      c.release('candidate'); await c.flush();
      expect(c.row('身份上链').textContent).toContain('选择身份'); expect(c.input().disabled).toBe(false); expect(c.action().disabled).toBe(true);
      expect(c.row('身份上链').textContent).not.toContain('竞选身份');
      expect(c.error).not.toHaveBeenCalled();
    } finally { await c.dispose(); }
  }, 15000);
  browserTest('并发已升级竞选时签名前刷新直接锁定，不发旧投票登记请求', async () => {
    const c = await mount();
    try {
      await c.choose('投票身份'); c.setIdentity('candidate'); await c.click('签名', '身份上链');
      expect(c.writes()).toHaveLength(0); expect(c.assertion).not.toHaveBeenCalled();
      expect(c.row('身份上链').textContent).toContain('竞选身份'); expect(c.input().disabled).toBe(true); expect(c.action().disabled).toBe(true);
    } finally { await c.dispose(); }
  }, 15000);
  for (const phase of ['prepare', 'complete'] as const) {
    browserTest(`${phase}期间他人先完成竞选升级时，冲突后重读并锁定选择和签名`, async () => {
      const c = await mount({ identity: 'voting', conflictAt: phase });
      try {
        await c.click('更换', '身份上链'); await c.choose('竞选身份'); await c.click('签名', '身份上链');
        if (phase === 'complete') await c.click('模拟公民回签');
        expect(c.row('身份上链').textContent).toContain('竞选身份');
        expect(c.input().disabled).toBe(true); expect(c.action().disabled).toBe(true);
        expect(c.adminSign).not.toHaveBeenCalled(); expect(c.submit).not.toHaveBeenCalled();
        expect(c.assertion).toHaveBeenCalledTimes(phase === 'complete' ? 1 : 0);
        expect(c.error).toHaveBeenCalledTimes(1);
      } finally { await c.dispose(); }
    }, 15000);
  }
  for (const options of [{ canWrite: false }, { underAge: true }]) {
    browserTest(`${options.underAge ? '未满16岁' : '没有写权限'}时不因空选身份控件开放而获得签名能力`, async () => {
      const c = await mount(options);
      try {
        expect(c.input().disabled).toBe(true); expect(c.action().disabled).toBe(true); expect(c.writes()).toHaveLength(0);
        expect(document.querySelector('.ant-alert')!.closest('.citizen-details')).toBeNull();
      }
      finally { await c.dispose(); }
    }, 15000);
  }
});
