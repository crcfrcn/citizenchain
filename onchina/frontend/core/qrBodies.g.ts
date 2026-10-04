// 本文件由 citizenchain/crates/qr-protocol/registry/kinds.yaml 生成，禁止手改。
export const QR_BODY_SCHEMA = [
  {
    "fields": [
      {
        "allowed_ints": [],
        "constraint": "positive_int",
        "empty_for_action_codes": [],
        "exact_bytes": null,
        "field_key": "action",
        "min_bytes": null,
        "required": true,
        "wire_key": "a"
      },
      {
        "allowed_ints": [
          1
        ],
        "constraint": "enum_int",
        "empty_for_action_codes": [],
        "exact_bytes": null,
        "field_key": "sig_alg",
        "min_bytes": null,
        "required": true,
        "wire_key": "g"
      },
      {
        "allowed_ints": [],
        "constraint": "signer_public_key",
        "empty_for_action_codes": [
          10,
          11
        ],
        "exact_bytes": null,
        "field_key": "signer_public_key",
        "min_bytes": null,
        "required": true,
        "wire_key": "u"
      },
      {
        "allowed_ints": [],
        "constraint": "b64u_bytes",
        "empty_for_action_codes": [],
        "exact_bytes": null,
        "field_key": "review_payload",
        "min_bytes": 1,
        "required": true,
        "wire_key": "d"
      }
    ],
    "kind_code": 1,
    "kind_key": "sign_request",
    "optional_pairs": [],
    "temporary": true
  },
  {
    "fields": [
      {
        "allowed_ints": [],
        "constraint": "b64u_bytes",
        "empty_for_action_codes": [],
        "exact_bytes": 32,
        "field_key": "signer_public_key",
        "min_bytes": null,
        "required": true,
        "wire_key": "u"
      },
      {
        "allowed_ints": [],
        "constraint": "b64u_bytes",
        "empty_for_action_codes": [],
        "exact_bytes": 64,
        "field_key": "signature",
        "min_bytes": null,
        "required": true,
        "wire_key": "s"
      },
      {
        "allowed_ints": [],
        "constraint": "b64u_bytes",
        "empty_for_action_codes": [],
        "exact_bytes": 32,
        "field_key": "current_account_id",
        "min_bytes": null,
        "required": false,
        "wire_key": "o"
      },
      {
        "allowed_ints": [],
        "constraint": "b64u_bytes",
        "empty_for_action_codes": [],
        "exact_bytes": 64,
        "field_key": "current_account_signature",
        "min_bytes": null,
        "required": false,
        "wire_key": "r"
      }
    ],
    "kind_code": 2,
    "kind_key": "sign_response",
    "optional_pairs": [
      [
        "o",
        "r"
      ]
    ],
    "temporary": true
  },
  {
    "fields": [
      {
        "allowed_ints": [],
        "constraint": "cid_number",
        "empty_for_action_codes": [],
        "exact_bytes": null,
        "field_key": "cid_number",
        "min_bytes": null,
        "required": true,
        "wire_key": "c"
      },
      {
        "allowed_ints": [],
        "constraint": "account_id",
        "empty_for_action_codes": [],
        "exact_bytes": null,
        "field_key": "account_id",
        "min_bytes": null,
        "required": true,
        "wire_key": "n"
      }
    ],
    "kind_code": 3,
    "kind_key": "user_contact",
    "optional_pairs": [],
    "temporary": false
  },
  {
    "fields": [
      {
        "allowed_ints": [],
        "constraint": "account_id",
        "empty_for_action_codes": [],
        "exact_bytes": null,
        "field_key": "account_id",
        "min_bytes": null,
        "required": true,
        "wire_key": "n"
      },
      {
        "allowed_ints": [],
        "constraint": "text_nonempty",
        "empty_for_action_codes": [],
        "exact_bytes": null,
        "field_key": "amount",
        "min_bytes": null,
        "required": true,
        "wire_key": "v"
      },
      {
        "allowed_ints": [],
        "constraint": "text_nonempty",
        "empty_for_action_codes": [],
        "exact_bytes": null,
        "field_key": "symbol",
        "min_bytes": null,
        "required": true,
        "wire_key": "t"
      },
      {
        "allowed_ints": [],
        "constraint": "text",
        "empty_for_action_codes": [],
        "exact_bytes": null,
        "field_key": "memo",
        "min_bytes": null,
        "required": true,
        "wire_key": "m"
      },
      {
        "allowed_ints": [],
        "constraint": "cid_number",
        "empty_for_action_codes": [],
        "exact_bytes": null,
        "field_key": "bank_cid_number",
        "min_bytes": null,
        "required": true,
        "wire_key": "l"
      }
    ],
    "kind_code": 4,
    "kind_key": "user_transfer",
    "optional_pairs": [],
    "temporary": true
  },
  {
    "fields": [
      {
        "allowed_ints": [],
        "constraint": "account_id",
        "empty_for_action_codes": [],
        "exact_bytes": null,
        "field_key": "account_id",
        "min_bytes": null,
        "required": true,
        "wire_key": "n"
      }
    ],
    "kind_code": 5,
    "kind_key": "account_id_code",
    "optional_pairs": [],
    "temporary": false
  },
  {
    "fields": [
      {
        "allowed_ints": [],
        "constraint": "b64u_bytes",
        "empty_for_action_codes": [],
        "exact_bytes": 32,
        "field_key": "signer_public_key",
        "min_bytes": null,
        "required": true,
        "wire_key": "u"
      },
      {
        "allowed_ints": [],
        "constraint": "b64u_bytes",
        "empty_for_action_codes": [],
        "exact_bytes": 64,
        "field_key": "signature",
        "min_bytes": null,
        "required": true,
        "wire_key": "s"
      },
      {
        "allowed_ints": [],
        "constraint": "b64u_bytes",
        "empty_for_action_codes": [],
        "exact_bytes": 32,
        "field_key": "key_exchange_public_key",
        "min_bytes": null,
        "required": true,
        "wire_key": "x"
      },
      {
        "allowed_ints": [],
        "constraint": "b64u_bytes",
        "empty_for_action_codes": [],
        "exact_bytes": 12,
        "field_key": "encryption_nonce",
        "min_bytes": null,
        "required": true,
        "wire_key": "q"
      },
      {
        "allowed_ints": [],
        "constraint": "b64u_bytes",
        "empty_for_action_codes": [],
        "exact_bytes": null,
        "field_key": "ciphertext",
        "min_bytes": 17,
        "required": true,
        "wire_key": "z"
      }
    ],
    "kind_code": 6,
    "kind_key": "account_data_key_response",
    "optional_pairs": [],
    "temporary": true
  }
] as const;

export type GeneratedQrKind = (typeof QR_BODY_SCHEMA)[number]['kind_key'];
export const GENERATED_QR_KIND_CODE = Object.fromEntries(
  QR_BODY_SCHEMA.map((kind) => [kind.kind_key, kind.kind_code]),
) as Record<GeneratedQrKind, number>;
export const GENERATED_FIXED_KINDS = QR_BODY_SCHEMA
  .filter((kind) => !kind.temporary)
  .map((kind) => kind.kind_key) as readonly GeneratedQrKind[];

export type GeneratedQrShape = {
  kindKey: string;
  kindCode: number;
  temporary: boolean;
  id?: string;
  expiresAt?: number;
  body: Record<string, unknown>;
};

export function validateGeneratedQrV1(data: Record<string, unknown>): GeneratedQrShape {
  if (data.p !== 'QR_V1' || !Number.isInteger(data.k)) throw new Error('QR envelope 必须使用 QR_V1 与整数 k');
  const kind = QR_BODY_SCHEMA.find((entry) => entry.kind_code === data.k);
  if (!kind) throw new Error(`未知 k: ${String(data.k)}`);
  exactKeys(data, kind.temporary ? ['p', 'k', 'i', 'e', 'b'] : ['p', 'k', 'b'], 'QR envelope');
  let id: string | undefined;
  let expiresAt: number | undefined;
  if (kind.temporary) {
    if (typeof data.i !== 'string' || data.i.length === 0 || !Number.isInteger(data.e) || (data.e as number) <= 0) {
      throw new Error('临时码必须包含非空 i 与正整数 e');
    }
    id = data.i;
    expiresAt = data.e as number;
  }
  if (!data.b || typeof data.b !== 'object' || Array.isArray(data.b)) throw new Error('缺少 b 对象');
  const body = data.b as Record<string, unknown>;
  validateBody(kind, body);
  return { kindKey: kind.kind_key, kindCode: kind.kind_code, temporary: kind.temporary, id, expiresAt, body };
}

export function decodeGeneratedBase64Url(input: string, field: string): string {
  if (!/^[A-Za-z0-9_-]+$/.test(input)) throw new Error(`${field} 必须为无填充 base64url`);
  const padded = input.padEnd(input.length + ((4 - input.length % 4) % 4), '=');
  let binary: string;
  try { binary = atob(padded.replace(/-/g, '+').replace(/_/g, '/')); }
  catch { throw new Error(`${field} 必须为无填充 base64url`); }
  const canonical = btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  if (canonical !== input) throw new Error(`${field} 必须为规范无填充 base64url`);
  return `0x${Array.from(binary, (ch) => ch.charCodeAt(0).toString(16).padStart(2, '0')).join('')}`;
}

function validateBody(kind: (typeof QR_BODY_SCHEMA)[number], body: Record<string, unknown>): void {
  const allowed = new Set<string>(kind.fields.map((field) => field.wire_key));
  const required = kind.fields.filter((field) => field.required).map((field) => field.wire_key);
  if (Object.keys(body).some((key) => !allowed.has(key)) || required.some((key) => !(key in body))) {
    throw new Error(`${kind.kind_key}.b 字段集合不符合 QR_V1`);
  }
  for (const pair of kind.optional_pairs) {
    if ((pair[0] in body) !== (pair[1] in body)) throw new Error(`b.${pair[0]} 与 b.${pair[1]} 必须成对出现`);
  }
  for (const field of kind.fields) {
    if (!(field.wire_key in body)) continue;
    validateField(body[field.wire_key], field, body);
  }
}

function validateField(value: unknown, field: (typeof QR_BODY_SCHEMA)[number]['fields'][number], body: Record<string, unknown>): void {
  const spec = field as typeof field & { allowed_ints: readonly number[]; exact_bytes: number | null; min_bytes: number | null; empty_for_action_codes: readonly number[] };
  const key = field.wire_key;
  if (spec.constraint === 'positive_int' && (!Number.isInteger(value) || (value as number) <= 0)) throw new Error(`b.${key} 必须为正整数`);
  if (spec.constraint === 'enum_int' && (!Number.isInteger(value) || !spec.allowed_ints.includes(value as number))) throw new Error(`b.${key} 不在允许值中`);
  if (spec.constraint === 'signer_public_key') {
    if (typeof value !== 'string') throw new Error(`b.${key} 必须为字符串`);
    if (spec.empty_for_action_codes.includes(body.a as number)) { if (value !== '') throw new Error(`b.${key} 在当前动作必须留空`); }
    else if ((decodeGeneratedBase64Url(value, `b.${key}`).length - 2) / 2 !== 32) throw new Error(`b.${key} 必须解码为 32 字节`);
  }
  if (spec.constraint === 'b64u_bytes') {
    if (typeof value !== 'string') throw new Error(`b.${key} 必须为字符串`);
    const length = (decodeGeneratedBase64Url(value, `b.${key}`).length - 2) / 2;
    if (spec.exact_bytes !== null && length !== spec.exact_bytes) throw new Error(`b.${key} 字节长度错误`);
    if (spec.min_bytes !== null && length < spec.min_bytes) throw new Error(`b.${key} 字节长度不足`);
  }
  if (spec.constraint === 'cid_number' && (typeof value !== 'string' || !/^[A-Za-z0-9-]{1,32}$/.test(value))) throw new Error(`b.${key} CID 格式错误`);
  if (spec.constraint === 'account_id' && (typeof value !== 'string' || !/^0x[0-9a-f]{64}$/.test(value))) throw new Error(`b.${key} account_id 格式错误`);
  if (spec.constraint === 'text_nonempty' && (typeof value !== 'string' || value.length === 0)) throw new Error(`b.${key} 必须为非空字符串`);
  if (spec.constraint === 'text' && typeof value !== 'string') throw new Error(`b.${key} 必须为字符串`);
}

function exactKeys(data: Record<string, unknown>, expected: readonly string[], context: string): void {
  const actual = Object.keys(data);
  if (actual.length !== expected.length || actual.some((key) => !expected.includes(key))) throw new Error(`${context} 字段集合不符合 QR_V1`);
}
