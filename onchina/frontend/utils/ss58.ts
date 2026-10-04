// SS58 地址编/解码工具
//
// CitizenChain 的 SS58 prefix 固定为 2027（runtime/primitives/src/core_const.rs）。
// 所有面向用户的地址展示统一使用 SS58；提交到链或后端的账户字段统一使用
// 小写 `0x` 加 64 位十六进制的 account_id。两者通过本文件提供的编解码函数互转。
//
// 编码格式（substrate SS58）：
//   payload  = prefix_bytes ++ pubkey_32
//   checksum = blake2b_512("SS58PRE" ++ payload)[0..2]
//   address  = base58( payload ++ checksum )
//
// prefix 编码规则：
//   0..=63          → 单字节 prefix
//   64..=16383      → 双字节 prefix（高位编码到 6 位）
//   2027 落在双字节区间。

import { blake2b } from '@noble/hashes/blake2.js';

export const CITIZENCHAIN_SS58_PREFIX = 2027;

const BASE58_ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
const BASE58_INDEX = new Map<string, number>(
  [...BASE58_ALPHABET].map((ch, idx) => [ch, idx]),
);
const SS58PRE = new TextEncoder().encode('SS58PRE');

function encodeBase58(bytes: Uint8Array): string {
  let zeros = 0;
  while (zeros < bytes.length && bytes[zeros] === 0) zeros++;

  // 转大整数 → base58 数字
  const digits: number[] = [0];
  for (let i = zeros; i < bytes.length; i++) {
    let carry = bytes[i];
    for (let j = 0; j < digits.length; j++) {
      const v = digits[j] * 256 + carry;
      digits[j] = v % 58;
      carry = (v / 58) | 0;
    }
    while (carry > 0) {
      digits.push(carry % 58);
      carry = (carry / 58) | 0;
    }
  }

  let out = '';
  for (let i = 0; i < zeros; i++) out += BASE58_ALPHABET[0];
  for (let i = digits.length - 1; i >= 0; i--) out += BASE58_ALPHABET[digits[i]];
  return out;
}

function decodeBase58(input: string): Uint8Array {
  if (!input) throw new Error('地址为空');
  let zeros = 0;
  while (zeros < input.length && input[zeros] === BASE58_ALPHABET[0]) zeros++;

  const bytes: number[] = [0];
  for (const ch of input) {
    const v = BASE58_INDEX.get(ch);
    if (v === undefined) throw new Error('地址含非法字符');
    let carry = v;
    for (let i = 0; i < bytes.length; i++) {
      const x = bytes[i] * 58 + carry;
      bytes[i] = x & 0xff;
      carry = x >> 8;
    }
    while (carry > 0) {
      bytes.push(carry & 0xff);
      carry >>= 8;
    }
  }

  const out = new Uint8Array(zeros + bytes.length);
  for (let i = 0; i < bytes.length; i++) out[out.length - 1 - i] = bytes[i];
  return out;
}

/// 把 SS58 prefix 编码成 1 或 2 个字节，对照 substrate Ss58Codec：
///   prefix ≤ 63       → 单字节 prefix
///   64 ≤ prefix ≤ 16383 → 双字节：
///       first  = ((prefix & 0xfc) >> 2) | 0x40
///       second = (prefix >> 8) | ((prefix & 0x03) << 6)
function encodePrefix(prefix: number): Uint8Array {
  if (prefix < 0 || prefix > 16383) {
    throw new Error(`SS58 prefix 超出范围：${prefix}`);
  }
  if (prefix <= 63) {
    return new Uint8Array([prefix]);
  }
  const f = ((prefix & 0xfc) >> 2) | 0x40;
  const s = (prefix >> 8) | ((prefix & 0x03) << 6);
  return new Uint8Array([f, s]);
}

function decodePrefix(buf: Uint8Array): { prefix: number; len: number } {
  if (buf.length === 0) throw new Error('地址数据为空');
  const b0 = buf[0];
  if (b0 <= 63) {
    return { prefix: b0, len: 1 };
  }
  if (b0 <= 127) {
    if (buf.length < 2) throw new Error('SS58 prefix 截断');
    const b1 = buf[1];
    const prefix = ((b0 & 0x3f) << 2) | (b1 >> 6) | ((b1 & 0x3f) << 8);
    return { prefix, len: 2 };
  }
  throw new Error('SS58 prefix 编码无效');
}

function ss58Checksum(payload: Uint8Array): Uint8Array {
  const buf = new Uint8Array(SS58PRE.length + payload.length);
  buf.set(SS58PRE);
  buf.set(payload, SS58PRE.length);
  return blake2b(buf, { dkLen: 64 });
}

/// 把规范 account_id 编码成 SS58 展示地址。
export function encodeSs58(account_id: string, prefix: number = CITIZENCHAIN_SS58_PREFIX): string {
  account_id = account_id.trim();
  if (!/^0x[0-9a-f]{64}$/.test(account_id)) {
    throw new Error('账户 ID 必须是小写 0x 加 64 位十六进制');
  }
  const cleaned = account_id.slice(2);
  const pubkey = new Uint8Array(32);
  for (let i = 0; i < 32; i++) {
    pubkey[i] = parseInt(cleaned.substr(i * 2, 2), 16);
  }
  const prefixBytes = encodePrefix(prefix);
  const payload = new Uint8Array(prefixBytes.length + pubkey.length);
  payload.set(prefixBytes);
  payload.set(pubkey, prefixBytes.length);
  const hash = ss58Checksum(payload);
  const checksum = hash.slice(0, 2);
  const full = new Uint8Array(payload.length + 2);
  full.set(payload);
  full.set(checksum, payload.length);
  return encodeBase58(full);
}

/// 把 SS58 地址解码回规范 account_id。
/// 同时校验 prefix 与校验和；任何不通过即抛错。
export function decodeSs58(address: string, expectedPrefix: number = CITIZENCHAIN_SS58_PREFIX): string {
  const data = decodeBase58(address.trim());
  if (data.length < 3) throw new Error('地址长度无效');
  const { prefix, len: prefixLen } = decodePrefix(data);
  if (prefix !== expectedPrefix) {
    throw new Error(`地址 prefix 不匹配（应为 ${expectedPrefix}，实际 ${prefix}）`);
  }
  const payloadLen = data.length - prefixLen - 2;
  if (payloadLen !== 32) {
    throw new Error('地址账户长度不是 32 字节');
  }
  const payload = data.slice(0, prefixLen + payloadLen);
  const expectedChecksum = data.slice(prefixLen + payloadLen);
  const hash = ss58Checksum(payload);
  if (expectedChecksum[0] !== hash[0] || expectedChecksum[1] !== hash[1]) {
    throw new Error('地址校验和无效');
  }
  const pubkey = data.slice(prefixLen, prefixLen + payloadLen);
  let hex = '';
  for (let i = 0; i < pubkey.length; i++) {
    hex += pubkey[i].toString(16).padStart(2, '0');
  }
  return `0x${hex}`;
}

/// 安全版本：失败时返回原始值（用于无法保证输入正确性的展示场景）。
export function tryEncodeSs58(account_id: string | null | undefined): string {
  if (!account_id) return '-';
  try {
    return encodeSs58(account_id);
  } catch {
    return account_id;
  }
}
