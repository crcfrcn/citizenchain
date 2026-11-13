// 所属业务前端回归，公共测试辅助只有一份。
import { describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, join, normalize } from 'node:path';
import { SECURITY_GRANT_HEADER, PASSKEY_ASSERTION_HEADER, passkeySubmitHeaders, securityGrantSubmitHeaders, commitAdminAction, rustConst } from './security';

describe('本地资料写入使用二级验证', () => {
  it('上传删除各验证一次Passkey，读取不验证，均不进入冷签授权流程', async () => {
    const { assertPasskey } = await import('../authentication/passkeyClient');
    const { uploadDocument, deleteDocument, listDocuments } = await import('../documents/api');
    const auth = { setupRequired: false, accessToken: 'fixture-session' } as never;
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => new Response(JSON.stringify({ code: 0, data: [] })));
    try {
      vi.mocked(assertPasskey).mockClear();
      await listDocuments(auth, 'fixture');
      expect(assertPasskey).not.toHaveBeenCalled();
      for (const run of [() => uploadDocument(auth, 'fixture', new File(['fixture'], 'fixture.txt'), 'OTHER'), () => deleteDocument(auth, 'fixture', 1)]) {
        vi.mocked(assertPasskey).mockClear(); fetcher.mockClear();
        await run();
        expect(assertPasskey).toHaveBeenCalledTimes(1);
        expect(fetcher).toHaveBeenCalledTimes(1);
        const [path, init] = fetcher.mock.calls[0];
        expect(String(path)).toContain('/api/institutions/fixture/docs');
        const headers = new Headers(init?.headers);
        expect(headers.get(PASSKEY_ASSERTION_HEADER)).toBe('fake-passkey-assertion');
        expect(headers.has(SECURITY_GRANT_HEADER)).toBe(false);
      }
    } finally { fetcher.mockRestore(); vi.mocked(assertPasskey).mockClear(); }
  });
});

describe('首次设置与已有密钥变更的客户端通路', () => {
  it('首次设置不要求旧凭证，已有密钥变更先验证一次并随finish提交', async () => {
    const { registerPasskey } = await import('../authentication/passkeyClient');
    const bytes = () => new Uint8Array([1]).buffer;
    const credential = { id: 'fixture', rawId: bytes(), type: 'public-key', response: {
      attestationObject: bytes(), clientDataJSON: bytes(), authenticatorData: bytes(), signature: bytes(), userHandle: null,
    }, getClientExtensionResults: () => ({}) };
    const create = vi.fn().mockResolvedValue(credential), get = vi.fn().mockResolvedValue(credential);
    vi.stubGlobal('window', { isSecureContext: true, PublicKeyCredential: class {} });
    vi.stubGlobal('navigator', { credentials: { create, get } });
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async path => {
      const name = String(path);
      const challenge = name.includes('/assert/')
        ? { publicKey: { challenge: 'AA', rpId: 'onchina.crcfrcn.com' } }
        : { publicKey: { challenge: 'AA', rp: { id: 'onchina.crcfrcn.com', name: 'fixture' }, user: { id: 'AA', name: 'fixture', displayName: 'fixture' }, pubKeyCredParams: [{ type: 'public-key', alg: -7 }] } };
      return new Response(JSON.stringify({ code: 0, data: name.endsWith('/begin') ? { ceremonyId: 'fixture', challenge } : { assertionId: 'fixture-assertion' } }));
    });
    try {
      for (const setupRequired of [true, false]) {
        fetcher.mockClear(); create.mockClear(); get.mockClear();
        await registerPasskey({ setupRequired, accessToken: 'fixture-session' } as never);
        expect(create).toHaveBeenCalledTimes(1);
        expect(get).toHaveBeenCalledTimes(setupRequired ? 0 : 1);
        expect(fetcher).toHaveBeenCalledTimes(setupRequired ? 2 : 4);
        const finish = fetcher.mock.calls.find(([path]) => String(path).endsWith('/register/finish'))!;
        const headers = new Headers(finish[1]?.headers);
        expect(headers.get(PASSKEY_ASSERTION_HEADER)).toBe(setupRequired ? null : 'fixture-assertion');
        expect(headers.has(SECURITY_GRANT_HEADER)).toBe(false);
      }
    } finally { fetcher.mockRestore(); vi.unstubAllGlobals(); }
  });
});

describe('查询字段与非JSON拒绝响应', () => {
  it('城市与登录轮询使用后端严格驼峰参数，保留中文及特殊字符', async () => {
    const { listCidCities } = await import('../codes/api');
    const { queryAdminQrLoginResult } = await import('../authentication/api');
    const fetcher = vi.spyOn(globalThis, 'fetch').mockImplementation(async () =>
      new Response(JSON.stringify({ code: 0, data: [] })));
    try {
      await listCidCities({ accessToken: 'fixture-session' } as never, '贵州省 & 测试');
      await queryAdminQrLoginResult('challenge&=1', 'session + 2');
      const cities = new URL(String(fetcher.mock.calls[0][0]), 'https://example.invalid');
      expect([...cities.searchParams]).toEqual([['provinceName', '贵州省 & 测试']]);
      expect(new Headers(fetcher.mock.calls[0][1]?.headers).get('authorization')).toBe('Bearer fixture-session');
      const result = new URL(String(fetcher.mock.calls[1][0]), 'https://example.invalid');
      expect([...result.searchParams]).toEqual([['challengeId', 'challenge&=1'], ['sessionId', 'session + 2']]);
    } finally { fetcher.mockRestore(); }
  });

  it('纯文本400保留状态，纯文本401仍触发一次登录失效，正常和业务响应保持', async () => {
    const { request, ApiError, AuthExpiredError, setOnUnauthorized } = await import('../workspace/http');
    const { normalizeNoticeText } = await import('../controls/notice');
    const unauthorized = vi.fn();
    setOnUnauthorized(unauthorized);
    const fetcher = vi.spyOn(globalThis, 'fetch');
    try {
      fetcher.mockResolvedValueOnce(new Response('Failed to deserialize query string', { status: 400 }));
      const error = await request('/fixture').catch(error => error);
      expect(error).toBeInstanceOf(ApiError);
      if (!(error instanceof ApiError)) throw error;
      expect(error.status).toBe(400);
      expect(normalizeNoticeText(error)).toContain('请求参数错误');
      expect(error.message).not.toContain('重启');
      expect(unauthorized).not.toHaveBeenCalled();
      for (let index = 0; index < 2; index++) {
        fetcher.mockResolvedValueOnce(new Response('unauthorized', { status: 401 }));
        await expect(request('/fixture')).rejects.toBeInstanceOf(AuthExpiredError);
      }
      expect(unauthorized).toHaveBeenCalledTimes(1);
      fetcher.mockResolvedValueOnce(new Response(JSON.stringify({ code: 1001, message: '姓名不能为空' }), { status: 400 }));
      await expect(request('/fixture')).rejects.toMatchObject({ status: 400, code: 1001, message: '姓名不能为空' });
      fetcher.mockResolvedValueOnce(new Response(JSON.stringify({ code: 0, data: { value: 1 } })));
      await expect(request('/fixture')).resolves.toEqual({ value: 1 });
      fetcher.mockResolvedValueOnce(new Response('<html>unexpected</html>'));
      await expect(request('/fixture')).rejects.toMatchObject({ status: 200, message: '服务响应格式错误，请联系管理员' });
    } finally { setOnUnauthorized(null); fetcher.mockRestore(); }
  });

  it('密钥状态只接受布尔值，格式漂移不能被当作已设置', async () => {
    const { getPasskeyStatus } = await import('../authentication/passkeyClient');
    const fetcher = vi.spyOn(globalThis, 'fetch');
    try {
      for (const registered of ['false', 1, null]) {
        fetcher.mockResolvedValueOnce(new Response(JSON.stringify({ code: 0, data: { registered } })));
        await expect(getPasskeyStatus({ accessToken: 'fixture-session' } as never)).rejects.toThrow('密钥状态响应格式错误');
      }
      fetcher.mockResolvedValueOnce(new Response(JSON.stringify({ code: 0, data: { registered: false } })));
      await expect(getPasskeyStatus({ accessToken: 'fixture-session' } as never)).resolves.toBe(false);
    } finally { fetcher.mockRestore(); }
  });

  it('旧会话的迟到401不能清退新会话，新会话仍独立处理自己的401', async () => {
    const { request, adminBlobRequest, AuthExpiredError, setOnUnauthorized } = await import('../workspace/http');
    const fetcher = vi.spyOn(globalThis, 'fetch');
    try {
      for (const run of [() => request('/fixture'), () => adminBlobRequest('/fixture', { accessToken: 'fixture-session' } as never)]) {
        const previous = vi.fn(), current = vi.fn();
        setOnUnauthorized(previous);
        let complete!: (value: Response) => void;
        fetcher.mockImplementationOnce(() => new Promise(resolve => { complete = resolve; }));
        const old = run();
        setOnUnauthorized(current);
        complete(new Response('unauthorized', { status: 401 }));
        await expect(old).rejects.toBeInstanceOf(AuthExpiredError);
        expect(previous).not.toHaveBeenCalled();
        expect(current).not.toHaveBeenCalled();
        fetcher.mockResolvedValueOnce(new Response('unauthorized', { status: 401 }));
        await expect(run()).rejects.toBeInstanceOf(AuthExpiredError);
        expect(current).toHaveBeenCalledTimes(1);
      }
    } finally { setOnUnauthorized(null); fetcher.mockRestore(); }
  });
});

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
  const auth = { accountId: '0x' + '11'.repeat(32) } as never;

  it('本地写(Passkey 档)只带 passkey 断言,不得带冷签 grant 头', async () => {
    const headers = await passkeySubmitHeaders(auth);
    expect(headers[PASSKEY_ASSERTION_HEADER]).toBe('fake-passkey-assertion');
    // 带上 grant 头意味着把本地写伪装成链上写;后端会去消费一个不存在的 grant。
    expect(headers[SECURITY_GRANT_HEADER]).toBeUndefined();
  });

  it('链上写(PasskeyColdSign 档)必须同时带 grant 与 passkey 两个头', async () => {
    const headers = await securityGrantSubmitHeaders(auth, {
      grantId: 'grant-123',
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
describe('实际业务认证', () => {
const auth = { accessToken: 'test-session', accountId: '0x' + '11'.repeat(32) } as never;
it('直接治理commit取一次Passkey，普通grant commit不重复取', async () => {
    const { assertPasskey } = await import('../authentication/passkeyClient');
    const fetcher = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(JSON.stringify({ code: 0, data: {} })));
    try {
      for (const actionType of ['CREATE_SUBORDINATE_REGISTRY', 'DELETE_SUBORDINATE_REGISTRY', 'NODE_BINDING_UNBIND', 'INSTITUTION_CREATE_ACCOUNT'] as const) {
        vi.mocked(assertPasskey).mockClear();
        fetcher.mockResolvedValue(new Response(JSON.stringify({ code: 0, data: {} })));
        await commitAdminAction(auth, { actionId: 'action', actionType, payloadHash: 'hash', accountId: 'account', signature: 'signature' });
        expect(assertPasskey).toHaveBeenCalledTimes(actionType === 'INSTITUTION_CREATE_ACCOUNT' ? 0 : 1);
        const calls = fetcher.mock.calls;
        const body = JSON.parse(String(calls[calls.length - 1]?.[1]?.body));
        expect(body).toEqual({ actionId: 'action', payloadHash: 'hash', accountId: 'account', signature: 'signature' });
      }
    } finally { fetcher.mockRestore(); }
  });
});
describe('实际业务认证', () => {
const auth = { accessToken: 'test-session', accountId: '0x' + '11'.repeat(32) } as never;
it('Passkey被取消时不得发送业务写请求', async () => {
    const { assertPasskey } = await import('../authentication/passkeyClient');
    const fetcher = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(JSON.stringify({ code: 0, data: {} })));
    try {
      vi.mocked(assertPasskey).mockRejectedValueOnce(new Error('cancelled'));
      await expect(commitAdminAction(auth, { actionId: 'action', actionType: 'NODE_BINDING_UNBIND', payloadHash: 'hash' })).rejects.toThrow('cancelled');
      expect(fetcher).not.toHaveBeenCalled();
    } finally { fetcher.mockRestore(); }
  });
});
describe('机构治理真实请求', () => {
const auth = { accessToken: 'test-session', accountId: '0x' + '11'.repeat(32) } as never;
it('机构治理准备等待一次Passkey，保留请求且失败不重试', async () => {
    const { assertPasskey } = await import('../authentication/passkeyClient');
    const { prepareInstitutionGovernance } = await import('../institutions/governance');
    const governance = { cidNumber: 'fixture-institution', proposerRoleCode: 'REPRESENTATIVE' };
    const operations = [{ path: '/api/admin/institution/governance/prepare', payload: governance, run: () => prepareInstitutionGovernance(auth, governance) }];
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

it('非安全上下文在发送认证请求前拒绝，只提示正式公信HTTPS入口', async () => {
  const actual = await vi.importActual<typeof import('../authentication/passkeyClient')>('../authentication/passkeyClient');
  const fetcher = vi.spyOn(globalThis,'fetch');
  vi.stubGlobal('window',{isSecureContext:false,location:{protocol:'https:'}});
  vi.stubGlobal('navigator',{});
  try {
    await expect(actual.registerPasskey({accessToken:'synthetic-session',accountId:'synthetic-account'} as never)).rejects.toThrow('公信 HTTPS');
    expect(fetcher).not.toHaveBeenCalled();
  } finally { vi.unstubAllGlobals(); fetcher.mockRestore(); }
});
