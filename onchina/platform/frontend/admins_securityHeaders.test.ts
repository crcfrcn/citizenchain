import { describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

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
        const body = JSON.parse(String(fetcher.mock.calls.at(-1)?.[1]?.body));
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
    expect(node.fileNames).toContain(join(import.meta.dirname, 'admins_securityHeaders.test.ts'));
    expect(node.fileNames).toContain(join(import.meta.dirname, 'vitest.config.ts'));
    const root = ts.readConfigFile(join(import.meta.dirname, '../../tsconfig.json'), ts.sys.readFile);
    expect(root.config.files).toEqual([]);
    expect(root.config.references.map((entry: { path: string }) => entry.path)).toEqual(['./platform/frontend/tsconfig.json', './platform/frontend/tsconfig.node.json']);
  });
});
