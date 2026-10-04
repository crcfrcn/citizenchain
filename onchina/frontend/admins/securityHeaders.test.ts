import { describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

// 三档鉴权在前端的头部组装 + 与 Rust 后端的 header 名跨端锁。
//
// 为什么只测这两件事:onchina 前端 137 个 ts/tsx 里,绝大多数是展示层;
// 真正的鉴权判据在后端 `require_admin_security_grant`(已由 operation_auth.rs
// 的内联测试锁住档位判定)。前端这层被绕过后端仍 fail-closed,故它是**体验保护**
// 而非安全边界本身。值得测的只有:
//   1. 档位对应的头部组装 —— Passkey 档不得带 grant 头,PasskeyColdSign 档必须两头俱全
//   2. header 名与后端逐字对齐 —— 漂移会让请求被后端当作"未带凭证"整笔拒绝
//
// `assertPasskey` 走 WebAuthn(navigator.credentials),这里 mock 掉:
// 要测真实 WebAuthn 就得 mock 整套浏览器 API,测的是 mock 而非真实行为。

vi.mock('../auth/passkey/passkeyClient', async () => {
  const actual = await vi.importActual<
    typeof import('../auth/passkey/passkeyClient')
  >('../auth/passkey/passkeyClient');
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
} = await import('./securityApi');
const { PASSKEY_ASSERTION_HEADER } = await import(
  '../auth/passkey/passkeyClient'
);

const ONCHINA_SRC = join(import.meta.dirname, '..', '..', 'src');

/** 从 Rust 源码里抠出 `const NAME: &str = "value";` 的字面量。 */
function rustConst(relativePath: string, name: string): string {
  const source = readFileSync(join(ONCHINA_SRC, relativePath), 'utf8');
  const matched = source.match(
    new RegExp(`const\\s+${name}\\s*:\\s*&str\\s*=\\s*"([^"]+)"`),
  );
  if (!matched) throw new Error(`未在 ${relativePath} 找到常量 ${name}`);
  return matched[1];
}

describe('前端 header 名与 Rust 后端一致(直读后端源码)', () => {
  // 照搬 cloudflare/test/cross_end_contract.test.ts 的做法:直接读另一端源文件。
  // 两端各自的测试都只对齐自己这一侧,名字漂移会两边全绿而线上 100% 拒绝。
  it('SECURITY_GRANT_HEADER 与 auth/actions.rs 的 ADMIN_SECURITY_GRANT_HEADER 逐字一致', () => {
    const backend = rustConst('auth/actions.rs', 'ADMIN_SECURITY_GRANT_HEADER');
    expect(SECURITY_GRANT_HEADER).toBe(backend);
  });

  it('PASSKEY_ASSERTION_HEADER 与 auth/passkey/mod.rs 逐字一致', () => {
    // 严格相等,不做大小写归一:后端 http_security.rs 用 HeaderName::from_static
    // 注册该头,那个 API 只接受小写、否则 panic,故小写是全仓唯一合法写法。
    const backend = rustConst(
      'auth/passkey/mod.rs',
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
