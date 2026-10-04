// QR_V1 统一协议 TS 类型与解析器。
//
// 唯一事实源：citizenchain/crates/qr-protocol/registry.json
// Golden fixtures:citizenchain/crates/qr-protocol/tests/fixtures/*.json
//
// body/envelope 约束由 core/qr/generated 产物统一解释；本文件只映射业务类型。
//
// body 键全部单字母,与 citizenapp / citizenwallet / onchina Rust 完全一致:
//   a 动作 / g 算法 / u 公钥 / d 载荷 / s 签名 / o 换绑当前账户 / r 换绑当前账户签名
//   c cid_number / n account_id / v 金额 / t 币种 / m 备注 / l 清算行 CID

import {
  decodeGeneratedBase64Url,
  GENERATED_FIXED_KINDS,
  GENERATED_QR_KIND_CODE,
  type GeneratedQrKind,
  validateGeneratedQrV1,
} from './qrBodies.g';

export const QR_V1 = 'QR_V1' as const;

export type QrKind = GeneratedQrKind;
export const QR_KIND_CODE = GENERATED_QR_KIND_CODE;

// 用户码与账户码都是固定码(无 i/e);收款码与签名请求/响应是临时码。
export const FIXED_KINDS: readonly QrKind[] = GENERATED_FIXED_KINDS;

export function isFixedKind(kind: QrKind): boolean {
  return FIXED_KINDS.includes(kind);
}

export interface SignRequestBody {
  action: number;
  sig_alg: 1;
  signer_public_key: string;
  payload_hex: string;
}

export interface SignResponseBody {
  signer_public_key: string;
  signature: string;
  /** 换绑且当前账户可签名时,与 current_account_signature 成对出现。 */
  current_account_id?: string;
  current_account_signature?: string;
}

/** 用户码 body:身份主键 + 其当前绑定账户。不含昵称与 SS58(见 spec 第 6 节)。 */
export interface UserContactBody {
  cid_number: string;
  account_id: string;
}

/** 收款码 body:收款账户 + 金额币种备注 + 收款方清算行 CID。不含收款人姓名。 */
export interface UserTransferBody {
  account_id: string;
  amount: string;
  symbol: string;
  memo: string;
  bank: string;
}

/** 账户码 body:只声明账户。钱包没有码,账户才有码。 */
export interface AccountIdCodeBody {
  account_id: string;
}

/** `k=6` 冷钱包用途钥响应；这里只映射统一注册表字段，不参与 OnChina 业务处理。 */
export interface AccountDataKeyResponseBody {
  signer_public_key: string;
  signature: string;
  key_exchange_public_key: string;
  encryption_nonce: string;
  ciphertext: string;
}

export type QrBodyByKind = {
  sign_request: SignRequestBody;
  sign_response: SignResponseBody;
  user_contact: UserContactBody;
  user_transfer: UserTransferBody;
  account_id_code: AccountIdCodeBody;
  account_data_key_response: AccountDataKeyResponseBody;
};

export interface QrEnvelope<K extends QrKind = QrKind> {
  p: typeof QR_V1;
  k: number;
  kind: K;
  id?: string;
  expires_at?: number;
  body: QrBodyByKind[K];
}

export class QrParseError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'QrParseError';
  }
}

/** 输入总长度上限:二维码物理容量约 3KB,留足余量后拒绝超大输入。 */
const MAX_PAYLOAD_CHARS = 32768;

/** `account_id` 唯一格式:小写 0x + 64 位十六进制。锚定正则,免疫零宽/同形字。 */
const ACCOUNT_ID_PATTERN = /^0x[0-9a-f]{64}$/;

/** CID / 清算行 CID 字符集白名单:仅 ASCII 字母数字与连字符,挡零宽字符钓鱼。 */
function requireString(obj: Record<string, unknown>, key: string): string {
  const v = obj[key];
  if (typeof v !== 'string' || v.length === 0) {
    throw new QrParseError(`字段 ${key} 必填非空字符串`);
  }
  return v;
}

function requireInt(obj: Record<string, unknown>, key: string): number {
  const v = obj[key];
  if (typeof v !== 'number' || !Number.isInteger(v)) {
    throw new QrParseError(`字段 ${key} 必填整数`);
  }
  return v;
}

/** 严格字段闸:body 出现未知字段直接拒,防止旧协议字段混入。 */
/**
 * base64url(无填充)解码为 0x 十六进制。
 *
 * 严格字母表:拒绝 `=` 填充与标准 base64 的 `+`/`/`,并做重编码回环校验,
 * 拒绝非规范末位比特。与 citizenwallet 的实现同口径 —— 松一点就会出现
 * 「同一载荷此端过、彼端拒」的跨端分歧。
 */
function b64ToHex(input: string, field: string): string {
  return decodeGeneratedBase64Url(input, `b.${field}`);
}

function parseSignRequestBody(b: Record<string, unknown>): SignRequestBody {
  const action = requireInt(b, 'a');
  requireInt(b, 'g');
  const signerPublicKey = b['u'] as string;
  return {
    action,
    sig_alg: 1,
    signer_public_key: signerPublicKey === '' ? '0x' : b64ToHex(signerPublicKey, 'u'),
    payload_hex: b64ToHex(requireString(b, 'd'), 'd'),
  };
}

function parseSignResponseBody(b: Record<string, unknown>): SignResponseBody {
  // o/r 是换绑场景的可选对:要么都在要么都不在,禁止只出现其中一个。
  const hasO = 'o' in b;
  const out: SignResponseBody = {
    signer_public_key: b64ToHex(requireString(b, 'u'), 'u'),
    signature: b64ToHex(requireString(b, 's'), 's'),
  };
  if (hasO) {
    out.current_account_id = b64ToHex(requireString(b, 'o'), 'o');
    out.current_account_signature = b64ToHex(requireString(b, 'r'), 'r');
  }
  return out;
}

function parseUserContactBody(b: Record<string, unknown>): UserContactBody {
  return {
    cid_number: b['c'] as string,
    account_id: b['n'] as string,
  };
}

function parseUserTransferBody(b: Record<string, unknown>): UserTransferBody {
  return {
    account_id: b['n'] as string,
    amount: b['v'] as string,
    symbol: b['t'] as string,
    memo: b['m'] as string,
    bank: b['l'] as string,
  };
}

function parseAccountIdCodeBody(
  b: Record<string, unknown>,
): AccountIdCodeBody {
  return { account_id: b['n'] as string };
}

function parseAccountDataKeyResponseBody(
  b: Record<string, unknown>,
): AccountDataKeyResponseBody {
  return {
    signer_public_key: b64ToHex(requireString(b, 'u'), 'u'),
    signature: b64ToHex(requireString(b, 's'), 's'),
    key_exchange_public_key: b64ToHex(requireString(b, 'x'), 'x'),
    encryption_nonce: b64ToHex(requireString(b, 'q'), 'q'),
    ciphertext: b64ToHex(requireString(b, 'z'), 'z'),
  };
}

export function parseQrEnvelope(
  raw: string | Record<string, unknown>,
): QrEnvelope {
  let data: Record<string, unknown>;
  if (typeof raw === 'string') {
    if (raw.length > MAX_PAYLOAD_CHARS) {
      throw new QrParseError('QR 内容超长');
    }
    try {
      data = JSON.parse(raw) as Record<string, unknown>;
    } catch (e) {
      throw new QrParseError(`QR 内容非合法 JSON: ${(e as Error).message}`);
    }
  } else {
    data = raw;
  }
  if (!data || typeof data !== 'object') {
    throw new QrParseError('QR 内容不是对象');
  }
  let shape;
  try {
    shape = validateGeneratedQrV1(data);
  } catch (error) {
    throw new QrParseError((error as Error).message);
  }
  const kindCode = shape.kindCode;
  const kind = shape.kindKey as QrKind;
  const id = shape.id;
  const expiresAt = shape.expiresAt;
  const b = shape.body;

  let body: QrBodyByKind[QrKind];
  switch (kind) {
    case 'sign_request':
      body = parseSignRequestBody(b);
      break;
    case 'sign_response':
      body = parseSignResponseBody(b);
      break;
    case 'user_contact':
      body = parseUserContactBody(b);
      break;
    case 'user_transfer':
      body = parseUserTransferBody(b);
      break;
    case 'account_id_code':
      body = parseAccountIdCodeBody(b);
      break;
    case 'account_data_key_response':
      body = parseAccountDataKeyResponseBody(b);
      break;
  }

  const env: QrEnvelope = {
    p: QR_V1,
    k: kindCode,
    kind,
    body,
  };
  if (id !== undefined) env.id = id;
  if (expiresAt !== undefined) env.expires_at = expiresAt;
  return env;
}

export function serializeQrEnvelope(env: QrEnvelope): string {
  const out: Record<string, unknown> = {
    p: QR_V1,
    k: QR_KIND_CODE[env.kind],
  };
  if (!isFixedKind(env.kind)) {
    if (env.id === undefined || env.expires_at === undefined) {
      throw new QrParseError(`临时码 ${env.kind} 必须提供 id/expires_at`);
    }
    out['i'] = env.id;
    out['e'] = env.expires_at;
  }
  out['b'] = env.body;
  return JSON.stringify(out);
}

export function buildSignatureMessage(args: {
  kind: QrKind | number;
  id: string;
  system?: string | null;
  expiresAt?: number | null;
  principal: string;
}): string {
  const sys = args.system ?? '';
  const exp = args.expiresAt ?? 0;
  const kindCode =
    typeof args.kind === 'number' ? args.kind : QR_KIND_CODE[args.kind];
  let pp = args.principal;
  if (pp.startsWith('0x') || pp.startsWith('0X')) pp = pp.slice(2);
  pp = pp.toLowerCase();
  if (!ACCOUNT_ID_PATTERN.test(`0x${pp}`)) {
    throw new QrParseError('principal 必须为 32 字节账户标识');
  }
  return `${QR_V1}|${kindCode}|${args.id}|${sys}|${exp}|${pp}`;
}
