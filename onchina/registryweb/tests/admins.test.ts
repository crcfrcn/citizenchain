// 所属业务前端回归，公共测试辅助只有一份。
import { describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, join, normalize } from 'node:path';
import { SECURITY_GRANT_HEADER, PASSKEY_ASSERTION_HEADER, passkeySubmitHeaders, securityGrantSubmitHeaders, commitAdminAction, rustConst } from '@gmb/onchina-platform/testing';

describe('机构本地资料与照片写入', () => {
  it('保存资料和上传照片只取一次Passkey，不请求冷签prepare或commit', async () => {
    const { updateInstitution, uploadLegalRepresentativePhoto } = await import('../private/api');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const auth = { setupRequired: false, accessToken: 'fixture-session' } as never;
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => new Response(JSON.stringify({ code: 0, data: {} })));
    try {
      for (const run of [() => updateInstitution(auth, 'fixture', { cidFullName: 'fixture' }), () => uploadLegalRepresentativePhoto(auth, new File(['photo'], 'photo.png', { type: 'image/png' }))]) {
        fetcher.mockClear(); vi.mocked(assertPasskey).mockClear();
        await run();
        expect(assertPasskey).toHaveBeenCalledTimes(1);
        expect(fetcher).toHaveBeenCalledTimes(1);
        const [path, init] = fetcher.mock.calls[0];
        expect(String(path)).toMatch(/^\/api\/institutions\//);
        const headers = new Headers(init?.headers);
        expect(headers.get(PASSKEY_ASSERTION_HEADER)).toBe('fake-passkey-assertion');
        expect(headers.has(SECURITY_GRANT_HEADER)).toBe(false);
      }
    } finally { fetcher.mockRestore(); vi.mocked(assertPasskey).mockClear(); }
  });
});

describe('注册局首帧和既有双Tab', () => {
  it('未知状态不显示入口；未设置密钥直接进所属入口；已设置仍保留两个Tab', async () => {
    vi.resetModules();
    const React = await import('react');
    const { renderToStaticMarkup } = await import('react-dom/server');
    vi.doMock('../citizens/CitizensView', () => ({ CitizensView: () => React.createElement('div', { 'data-page': 'citizens' }) }));
    vi.doMock('../admins/ProvinceDetailView', () => ({ RegistryAdminsView: ({ mode }: { mode: string }) => React.createElement('div', { 'data-page': mode }) }));
    try {
      const { RegistryWorkspace } = await import('../controls/Workspace');
      const { EMPTY_CAPABILITIES } = await import('@gmb/onchina-platform/workspace/modules');
      const capabilities = { ...EMPTY_CAPABILITIES, canViewCitizens: true, canViewCityRegistry: true, canViewFederalRegistry: true };
      const props = { auth: { accountId: 'fixture-account', institutionCidNumber: 'fixture-institution', institutionCode: 'FRG' } as never,
        capabilities, cidMeta: null, setCidMeta: () => {} };
      const pending = renderToStaticMarkup(React.createElement(RegistryWorkspace, { ...props, passkeyRegistered: null }));
      expect(pending).not.toContain('data-page');
      expect(pending).not.toContain('<button');
      const locked = renderToStaticMarkup(React.createElement(RegistryWorkspace, { ...props, passkeyRegistered: false }));
      expect(locked).toContain('data-page="federal-registry"');
      expect(locked).not.toContain('data-page="citizens"');
      expect(locked).not.toContain('>市注册局</button>');
      const city = renderToStaticMarkup(React.createElement(RegistryWorkspace, { ...props, auth: { ...props.auth as object, institutionCode: 'CREG' } as never, passkeyRegistered: false }));
      expect(city).toContain('data-page="city-registry"');
      const ready = renderToStaticMarkup(React.createElement(RegistryWorkspace, { ...props, passkeyRegistered: true }));
      expect(ready).toContain('>市注册局</button>');
      expect(ready).toContain('>联邦注册局</button>');
      const denied = renderToStaticMarkup(React.createElement(RegistryWorkspace, { ...props, capabilities: EMPTY_CAPABILITIES, passkeyRegistered: true }));
      expect(denied).toContain('当前身份没有可访问的工作台入口');
      expect(denied).not.toContain('data-page');
    } finally {
      vi.doUnmock('../citizens/CitizensView'); vi.doUnmock('../admins/ProvinceDetailView'); vi.resetModules();
    }
  });
});

describe('机构操作记录读取', () => {
  const browserTest = process.env.ONCHINA_TEST_DOM ? it : it.skip;
  browserTest('按targetCid查询，失败显示重试而非零条记录，重试后显示真实记录', async () => {
    vi.resetModules();
    const { createRequire } = await import('node:module');
    const { isAbsolute } = await import('node:path');
    if (!isAbsolute(process.env.ONCHINA_TEST_DOM!)) throw new Error('DOM测试运行库必须是显式绝对路径');
    const { JSDOM } = createRequire(import.meta.url)(process.env.ONCHINA_TEST_DOM!);
    const dom = new JSDOM('<div id="root"></div>', { url: 'https://onchina.example.invalid/' });
    dom.window.matchMedia = () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {} });
    for (const name of ['window', 'document', 'navigator', 'HTMLElement', 'Element', 'ShadowRoot', 'SVGElement']) vi.stubGlobal(name, dom.window[name]);
    vi.stubGlobal('getComputedStyle', (element: Element) => dom.window.getComputedStyle(element));
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.doMock('@gmb/onchina-platform/institutions/institutionLabels', () => ({ useInstitutionCodeLabels: () => ({}) }));
    const React = await import('react');
    const { createRoot } = await import('react-dom/client');
    const root = createRoot(document.getElementById('root')!);
    const fetcher = vi.spyOn(globalThis, 'fetch')
      .mockResolvedValueOnce(new Response('Failed to deserialize query string', { status: 400 }))
      .mockResolvedValueOnce(new Response(JSON.stringify({ code: 0, data: [{ seq: 1, action: 'CITIZEN_CREATE', actorAccountId: '', detail: {}, createdAt: '2026-01-01T00:00:00Z' }] })));
    try {
      const { OperationRecords } = await import('../government/OperationRecords');
      await React.act(async () => root.render(React.createElement(OperationRecords, { auth: { accessToken: 'fixture-session' } as never, cidNumber: 'fixture & cid' })));
      expect(document.body.textContent).toContain('请求参数错误');
      expect(document.body.textContent).not.toContain('操作记录(0)');
      const retry = [...document.querySelectorAll('button')].find(button => button.textContent?.replace(/\s/g, '') === '重试')!;
      await React.act(async () => retry.click());
      expect(document.body.textContent).toContain('操作记录(1)');
      expect(document.body.textContent).toContain('注册局录入公民');
      expect(fetcher).toHaveBeenCalledTimes(2);
      for (const [path, init] of fetcher.mock.calls) {
        const url = new URL(String(path), 'https://example.invalid');
        expect([...url.searchParams]).toEqual([['targetCid', 'fixture & cid'], ['limit', '1000']]);
        expect(new Headers(init?.headers).get('authorization')).toBe('Bearer fixture-session');
      }
    } finally {
      await React.act(async () => root.unmount());
      fetcher.mockRestore(); dom.window.close();
      vi.doUnmock('@gmb/onchina-platform/institutions/institutionLabels');
      vi.unstubAllGlobals(); vi.resetModules();
    }
  });
});

describe('注册局登记真实请求', () => {
const auth = { accessToken: 'test-session', accountId: '0x' + '11'.repeat(32) } as never;
it('注册局登记准备等待一次Passkey，保留请求且失败不重试', async () => {
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const { prepareInstitutionGovernance } = await import('@gmb/onchina-platform/institutions/governance');
    const { prepareRegisterInstitutionAdmins } = await import('../admins/cityRegistryAdminsApi');
    const governance = { cidNumber: 'fixture-institution', proposerRoleCode: 'REPRESENTATIVE' };
    const registration = { cidNumber: 'fixture-institution', actorRoleCode: 'REGISTRY-ROLE', admins: [{ accountId: 'fixture-account', familyName: '测', givenName: '试' }] };
    const operations = [{ path: '/api/admin/institution/admins/register/prepare', payload: registration, run: () => prepareRegisterInstitutionAdmins(auth, registration) }];
    const assertion = vi.mocked(assertPasskey);
    const fetcher = vi.spyOn(globalThis, 'fetch');
    const prepared = { requestId: 'fixture-request', signRequest: 'fixture-sign-request' };
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
describe('登记管理员原有请求参数衔接', () => {
  it('原URL、Bearer与管理员集合保持，透传操作注册局岗位码', async () => {
    const { prepareRegisterInstitutionAdmins } = await import('../admins/cityRegistryAdminsApi');
    const { assertPasskey } = await import('@gmb/onchina-platform/authentication/passkeyClient');
    const auth = { accessToken: 'test-session' } as never;
    const input = { cidNumber: 'target-fixture-cid', admins: [{ accountId: 'fixture-account', familyName: '测', givenName: '试' }], actorRoleCode: 'REGISTRY-ROLE' };
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
