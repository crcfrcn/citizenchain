// 统一的签名二维码 payload 解析工具。
// 唯一事实源：citizenchain/crates/qr-protocol/registry.json
// 使用 QR_V1 envelope,不支持字段别名。

import { parseQrEnvelope, QrParseError } from '../core/citizenQr';
import type { SignResponseBody } from '../core/citizenQr';

// 两层字段名不同,别"顺手改成一致":
// - QR body 层:`sign_response.b.u` 解析后叫 `signer_public_key`(与 Dart 的
//   `signerPublicKey`、Rust 的 `u` 同名同义),表达"这次签名是谁签的"。
// - 后端 DTO 层:接口收的是 `account_id`。
// sr25519 公钥即账户标识,`parseSignResponseBody` 已把 `u` 转成 `0x` + 64 hex 的
// 规范 account_id,所以此处是直接改名映射,不是格式转换。
export type SignedLoginPayload = {
  challenge_id: string;
  account_id: string;
  signature: string;
};

export function parseSignedLoginPayload(
  raw: string,
  fallbackChallengeId: string,
): SignedLoginPayload {
  let env;
  try {
    env = parseQrEnvelope(raw);
  } catch (e) {
    if (e instanceof QrParseError) {
      throw new Error(`签名二维码解析失败: ${e.message}`);
    }
    throw e;
  }
  if (env.kind !== 'sign_response') {
    throw new Error(`期望 sign_response,实际: ${env.kind}`);
  }
  const body = env.body as SignResponseBody;
  const challenge_id = env.id || fallbackChallengeId;
  if (!challenge_id || !body.signer_public_key || !body.signature) {
    throw new Error('签名二维码缺少必要字段(id/signer_public_key/signature)');
  }
  return {
    challenge_id,
    account_id: body.signer_public_key,
    signature: body.signature,
  };
}

export type SignedReceiptPayload = {
  challenge_id: string;
  signature: string;
  account_id?: string;
  payload_hash?: string;
  current_account_id?: string;
  current_account_signature?: string;
};

// 解析"挑战签名响应"二维码 payload。
// 只接受 QR_V1 envelope(sign_response)。
// 返回结构供调用方提交后端 verify/commit。
export function parseSignedReceiptPayload(
  raw: string,
  fallbackChallengeId: string,
): SignedReceiptPayload {
  const trimmed = raw.trim();
  if (!trimmed) {
    throw new Error('签名二维码内容为空');
  }
  if (!trimmed.startsWith('{')) {
    throw new Error('签名二维码必须使用 QR_V1 envelope');
  }
  let env;
  try {
    env = parseQrEnvelope(trimmed);
  } catch (e) {
    if (e instanceof QrParseError) {
      throw new Error(`签名二维码解析失败: ${e.message}`);
    }
    throw e;
  }
  if (env.kind !== 'sign_response') {
    throw new Error(`期望 sign_response,实际: ${env.kind}`);
  }
  const challenge_id = env.id || fallbackChallengeId;
  const body = env.body as SignResponseBody;
  if (!challenge_id || !body.signature || !body.signer_public_key) {
    throw new Error('签名二维码缺少必要字段(id/signer_public_key/signature)');
  }
  return {
    challenge_id,
    signature: body.signature,
    account_id: body.signer_public_key,
    current_account_id: body.current_account_id,
    current_account_signature: body.current_account_signature,
  };
}
