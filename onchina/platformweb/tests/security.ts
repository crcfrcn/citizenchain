// 平台包唯一公开测试辅助；各独立前端直读真实前后端协议常量。
import { vi } from 'vitest';
import { readFileSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';

// 三档鉴权在前端的头部组装 + 与 Rust 后端的 header 名跨端锁。
//
// 后端独立执行鉴权；这里同时核验实际业务请求，防止只断言辅助函数而漏掉 Bearer。
// 覆盖内容:
//   1. 档位对应的头部组装 —— Passkey 档不得带 grant 头,PasskeyColdSign 档必须两头俱全
//   2. header 名与后端逐字对齐 —— 漂移会让请求被后端当作"未带凭证"整笔拒绝
//
// `assertPasskey` 走 WebAuthn(navigator.credentials),这里 mock 掉:
// 要测真实 WebAuthn 就得 mock 整套浏览器 API,测的是 mock 而非真实行为。

vi.mock('../authentication/passkeyClient', async () => {
  const actual = await vi.importActual<
    typeof import('../authentication/passkeyClient')
  >('../authentication/passkeyClient');
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
} = await import('../authentication/security');
const { PASSKEY_ASSERTION_HEADER } = await import(
  '../authentication/passkeyClient'
);

// 中文注释：按平台包根定位实际Rust源码，测试不保存第二份协议常量。
let repository = process.cwd();
while (!existsSync(join(repository, 'onchina/platform/Cargo.toml'))) {
  const parent = dirname(repository);
  if (parent === repository) throw new Error('测试不属于本产品真实工程');
  repository = parent;
}
const ONCHINA_PLATFORM = join(repository, 'onchina/platform');

/** 从 Rust 源码里抠出 `const NAME: &str = "value";` 的字面量。 */
function rustConst(relativePath: string, name: string): string {
  const source = readFileSync(join(ONCHINA_PLATFORM, relativePath), 'utf8');
  const matched = source.match(
    new RegExp(`const\\s+${name}\\s*:\\s*&str\\s*=\\s*"([^"]+)"`),
  );
  if (!matched) throw new Error(`未在 ${relativePath} 找到常量 ${name}`);
  return matched[1];
}

export { SECURITY_GRANT_HEADER, PASSKEY_ASSERTION_HEADER, passkeySubmitHeaders, securityGrantSubmitHeaders, commitAdminAction, rustConst };
