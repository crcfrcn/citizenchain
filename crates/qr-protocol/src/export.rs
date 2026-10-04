use crate::registry::{actions, fields, kinds, reject_reasons, RegistryError};
use serde_json::{json, Value};

/// 导出 registry JSON，供后续生成 Dart/TypeScript 产物。
///
/// JSON 主要用于人工审计；移动端实际消费的 Dart 常量由
/// [export_registry_dart] 生成，避免 App / Wallet 各自维护第二套动作表。
pub fn export_registry_json() -> Result<String, RegistryError> {
    let value = serde_json::json!({
        "actions": actions()?,
        "fields": fields()?,
        "reject_reasons": reject_reasons()?,
        "kinds": normalized_kinds()?,
    });
    Ok(serde_json::to_string_pretty(&value)?)
}

/// 导出 Dart QR_V1 envelope/body 结构校验器。
///
/// 生成文件只承载有限约束词汇表的解释器；具体页面仍负责决定允许接收哪一种 `k`。
pub fn export_qr_bodies_dart() -> Result<String, RegistryError> {
    let schema = serde_json::to_string_pretty(&normalized_kinds()?)?;
    let kind_enum = export_dart_kind_enum()?;
    Ok(format!(
        r#"// 本文件由 citizenchain/crates/qr-protocol/registry/kinds.yaml 生成，禁止手改。
import 'dart:convert';
import 'dart:typed_data';

{kind_enum}
class GeneratedQrBodySchema {{
  const GeneratedQrBodySchema._();

  static final List<Map<String, dynamic>> _kinds =
      (jsonDecode(r'''{schema}''') as List<dynamic>)
          .cast<Map<String, dynamic>>();

  static Map<String, dynamic> validateEnvelope(Map<String, dynamic> data) {{
    final kindCode = data['k'];
    if (data['p'] != 'QR_V1' || kindCode is! int) {{
      throw const FormatException('QR envelope 必须使用 QR_V1 与整数 k');
    }}
    final kind = _kinds.cast<Map<String, dynamic>?>().firstWhere(
          (item) => item?['kind_code'] == kindCode,
          orElse: () => null,
        );
    if (kind == null) throw FormatException('未知 k: $kindCode');
    final temporary = kind['temporary'] as bool;
    _exactKeys(data, temporary ? const {{'p', 'k', 'i', 'e', 'b'}} : const {{'p', 'k', 'b'}}, 'QR envelope');
    if (temporary) {{
      if (data['i'] is! String || (data['i'] as String).isEmpty ||
          data['e'] is! int || (data['e'] as int) <= 0) {{
        throw const FormatException('临时码必须包含非空 i 与正整数 e');
      }}
    }}
    final body = data['b'];
    if (body is! Map<String, dynamic>) throw const FormatException('缺少 b 对象');
    validateBody(kindCode, body);
    return kind;
  }}

  static void validateBody(int kindCode, Map<String, dynamic> body) {{
    final kind = _kinds.firstWhere(
      (item) => item['kind_code'] == kindCode,
      orElse: () => throw FormatException('未知 k: $kindCode'),
    );
    final fields = (kind['fields'] as List<dynamic>).cast<Map<String, dynamic>>();
    final allowed = fields.map((field) => field['wire_key'] as String).toSet();
    final required = fields
        .where((field) => field['required'] == true)
        .map((field) => field['wire_key'] as String)
        .toSet();
    if (body.keys.any((key) => !allowed.contains(key)) ||
        required.any((key) => !body.containsKey(key))) {{
      throw FormatException('${{kind['kind_key']}}.b 字段集合不符合 QR_V1');
    }}
    for (final pairRaw in kind['optional_pairs'] as List<dynamic>) {{
      final pair = (pairRaw as List<dynamic>).cast<String>();
      if (body.containsKey(pair[0]) != body.containsKey(pair[1])) {{
        throw FormatException('b.${{pair[0]}} 与 b.${{pair[1]}} 必须成对出现');
      }}
    }}
    for (final field in fields) {{
      final key = field['wire_key'] as String;
      if (!body.containsKey(key)) continue;
      _validateField(body[key], field, body);
    }}
  }}

  static Uint8List decodeBase64Url(String input, String field) {{
    if (input.isEmpty || input.contains('=') ||
        !RegExp(r'^[A-Za-z0-9_-]+$').hasMatch(input)) {{
      throw FormatException('$field 必须为无填充 base64url');
    }}
    final padded = input.padRight(input.length + ((4 - input.length % 4) % 4), '=');
    try {{
      final bytes = Uint8List.fromList(base64Url.decode(padded));
      if (base64Url.encode(bytes).replaceAll('=', '') != input) throw const FormatException();
      return bytes;
    }} catch (_) {{
      throw FormatException('$field 必须为规范无填充 base64url');
    }}
  }}

  static void _validateField(Object? value, Map<String, dynamic> field, Map<String, dynamic> body) {{
    final key = field['wire_key'] as String;
    switch (field['constraint']) {{
      case 'positive_int':
        if (value is! int || value <= 0) throw FormatException('b.$key 必须为正整数');
      case 'enum_int':
        if (value is! int || !(field['allowed_ints'] as List<dynamic>).contains(value)) {{
          throw FormatException('b.$key 不在允许值中');
        }}
      case 'signer_public_key':
        if (value is! String) throw FormatException('b.$key 必须为字符串');
        final emptyActions = (field['empty_for_action_codes'] as List<dynamic>).cast<int>();
        if (emptyActions.contains(body['a'])) {{
          if (value.isNotEmpty) throw FormatException('b.$key 在当前动作必须留空');
        }} else if (decodeBase64Url(value, 'b.$key').length != 32) {{
          throw FormatException('b.$key 必须解码为 32 字节');
        }}
      case 'b64u_bytes':
        if (value is! String) throw FormatException('b.$key 必须为字符串');
        final bytes = decodeBase64Url(value, 'b.$key');
        final exact = field['exact_bytes'] as int?;
        final min = field['min_bytes'] as int?;
        if (exact != null && bytes.length != exact) throw FormatException('b.$key 必须解码为 $exact 字节');
        if (min != null && bytes.length < min) throw FormatException('b.$key 至少解码为 $min 字节');
      case 'cid_number':
        if (value is! String || !RegExp(r'^[A-Za-z0-9-]{{1,32}}$').hasMatch(value)) {{
          throw FormatException('b.$key 必须为 1 到 32 位字母数字与连字符');
        }}
      case 'account_id':
        if (value is! String || !RegExp(r'^0x[0-9a-f]{{64}}$').hasMatch(value)) {{
          throw FormatException('b.$key 必须为小写 0x 加 64 位十六进制');
        }}
      case 'text_nonempty':
        if (value is! String || value.isEmpty) throw FormatException('b.$key 必须为非空字符串');
      case 'text':
        if (value is! String) throw FormatException('b.$key 必须为字符串');
      default:
        throw FormatException('b.$key 使用未知约束');
    }}
  }}

  static void _exactKeys(Map<String, dynamic> data, Set<String> expected, String context) {{
    final actual = data.keys.toSet();
    if (actual.length != expected.length || !actual.containsAll(expected) || !expected.containsAll(actual)) {{
      throw FormatException('$context 字段集合不符合 QR_V1');
    }}
  }}
}}
"#
    ))
}

/// 导出 React/TypeScript 共用的 QR_V1 结构校验器。
pub fn export_qr_bodies_typescript() -> Result<String, RegistryError> {
    let schema = serde_json::to_string_pretty(&normalized_kinds()?)?;
    Ok(format!(
        r#"// 本文件由 citizenchain/crates/qr-protocol/registry/kinds.yaml 生成，禁止手改。
export const QR_BODY_SCHEMA = {schema} as const;

export type GeneratedQrKind = (typeof QR_BODY_SCHEMA)[number]['kind_key'];
export const GENERATED_QR_KIND_CODE = Object.fromEntries(
  QR_BODY_SCHEMA.map((kind) => [kind.kind_key, kind.kind_code]),
) as Record<GeneratedQrKind, number>;
export const GENERATED_FIXED_KINDS = QR_BODY_SCHEMA
  .filter((kind) => !kind.temporary)
  .map((kind) => kind.kind_key) as readonly GeneratedQrKind[];

export type GeneratedQrShape = {{
  kindKey: string;
  kindCode: number;
  temporary: boolean;
  id?: string;
  expiresAt?: number;
  body: Record<string, unknown>;
}};

export function validateGeneratedQrV1(data: Record<string, unknown>): GeneratedQrShape {{
  if (data.p !== 'QR_V1' || !Number.isInteger(data.k)) throw new Error('QR envelope 必须使用 QR_V1 与整数 k');
  const kind = QR_BODY_SCHEMA.find((entry) => entry.kind_code === data.k);
  if (!kind) throw new Error(`未知 k: ${{String(data.k)}}`);
  exactKeys(data, kind.temporary ? ['p', 'k', 'i', 'e', 'b'] : ['p', 'k', 'b'], 'QR envelope');
  let id: string | undefined;
  let expiresAt: number | undefined;
  if (kind.temporary) {{
    if (typeof data.i !== 'string' || data.i.length === 0 || !Number.isInteger(data.e) || (data.e as number) <= 0) {{
      throw new Error('临时码必须包含非空 i 与正整数 e');
    }}
    id = data.i;
    expiresAt = data.e as number;
  }}
  if (!data.b || typeof data.b !== 'object' || Array.isArray(data.b)) throw new Error('缺少 b 对象');
  const body = data.b as Record<string, unknown>;
  validateBody(kind, body);
  return {{ kindKey: kind.kind_key, kindCode: kind.kind_code, temporary: kind.temporary, id, expiresAt, body }};
}}

export function decodeGeneratedBase64Url(input: string, field: string): string {{
  if (!/^[A-Za-z0-9_-]+$/.test(input)) throw new Error(`${{field}} 必须为无填充 base64url`);
  const padded = input.padEnd(input.length + ((4 - input.length % 4) % 4), '=');
  let binary: string;
  try {{ binary = atob(padded.replace(/-/g, '+').replace(/_/g, '/')); }}
  catch {{ throw new Error(`${{field}} 必须为无填充 base64url`); }}
  const canonical = btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  if (canonical !== input) throw new Error(`${{field}} 必须为规范无填充 base64url`);
  return `0x${{Array.from(binary, (ch) => ch.charCodeAt(0).toString(16).padStart(2, '0')).join('')}}`;
}}

function validateBody(kind: (typeof QR_BODY_SCHEMA)[number], body: Record<string, unknown>): void {{
  const allowed = new Set<string>(kind.fields.map((field) => field.wire_key));
  const required = kind.fields.filter((field) => field.required).map((field) => field.wire_key);
  if (Object.keys(body).some((key) => !allowed.has(key)) || required.some((key) => !(key in body))) {{
    throw new Error(`${{kind.kind_key}}.b 字段集合不符合 QR_V1`);
  }}
  for (const pair of kind.optional_pairs) {{
    if ((pair[0] in body) !== (pair[1] in body)) throw new Error(`b.${{pair[0]}} 与 b.${{pair[1]}} 必须成对出现`);
  }}
  for (const field of kind.fields) {{
    if (!(field.wire_key in body)) continue;
    validateField(body[field.wire_key], field, body);
  }}
}}

function validateField(value: unknown, field: (typeof QR_BODY_SCHEMA)[number]['fields'][number], body: Record<string, unknown>): void {{
  const spec = field as typeof field & {{ allowed_ints: readonly number[]; exact_bytes: number | null; min_bytes: number | null; empty_for_action_codes: readonly number[] }};
  const key = field.wire_key;
  if (spec.constraint === 'positive_int' && (!Number.isInteger(value) || (value as number) <= 0)) throw new Error(`b.${{key}} 必须为正整数`);
  if (spec.constraint === 'enum_int' && (!Number.isInteger(value) || !spec.allowed_ints.includes(value as number))) throw new Error(`b.${{key}} 不在允许值中`);
  if (spec.constraint === 'signer_public_key') {{
    if (typeof value !== 'string') throw new Error(`b.${{key}} 必须为字符串`);
    if (spec.empty_for_action_codes.includes(body.a as number)) {{ if (value !== '') throw new Error(`b.${{key}} 在当前动作必须留空`); }}
    else if ((decodeGeneratedBase64Url(value, `b.${{key}}`).length - 2) / 2 !== 32) throw new Error(`b.${{key}} 必须解码为 32 字节`);
  }}
  if (spec.constraint === 'b64u_bytes') {{
    if (typeof value !== 'string') throw new Error(`b.${{key}} 必须为字符串`);
    const length = (decodeGeneratedBase64Url(value, `b.${{key}}`).length - 2) / 2;
    if (spec.exact_bytes !== null && length !== spec.exact_bytes) throw new Error(`b.${{key}} 字节长度错误`);
    if (spec.min_bytes !== null && length < spec.min_bytes) throw new Error(`b.${{key}} 字节长度不足`);
  }}
  if (spec.constraint === 'cid_number' && (typeof value !== 'string' || !/^[A-Za-z0-9-]{{1,32}}$/.test(value))) throw new Error(`b.${{key}} CID 格式错误`);
  if (spec.constraint === 'account_id' && (typeof value !== 'string' || !/^0x[0-9a-f]{{64}}$/.test(value))) throw new Error(`b.${{key}} account_id 格式错误`);
  if (spec.constraint === 'text_nonempty' && (typeof value !== 'string' || value.length === 0)) throw new Error(`b.${{key}} 必须为非空字符串`);
  if (spec.constraint === 'text' && typeof value !== 'string') throw new Error(`b.${{key}} 必须为字符串`);
}}

function exactKeys(data: Record<string, unknown>, expected: readonly string[], context: string): void {{
  const actual = Object.keys(data);
  if (actual.length !== expected.length || actual.some((key) => !expected.includes(key))) throw new Error(`${{context}} 字段集合不符合 QR_V1`);
}}
"#
    ))
}

/// OnChina Rust 只生成接线层，真正的约束解释器唯一位于共享 crate。
pub fn export_qr_bodies_rust() -> Result<String, RegistryError> {
    let mut entries = kinds()?;
    entries.sort_by_key(|kind| kind.kind_code);
    let mut variants = String::new();
    for kind in entries {
        variants.push_str(&format!(
            "    {} = {},\n",
            rust_upper_camel(&kind.kind_key),
            kind.kind_code
        ));
    }
    Ok(format!(
        r#"// 本文件由 citizenchain/crates/qr-protocol/registry/kinds.yaml 生成，禁止手改。

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrKind {{
{variants}}}

impl QrKind {{
    pub fn code(self) -> u8 {{
        self as u8
    }}
}}

pub(super) fn validate(
    value: &serde_json::Value,
) -> Result<qr_protocol::QrKindEntry, super::QrParseError> {{
    qr_protocol::validate_qr_value(value)
        .map_err(|error| super::QrParseError::BadField(error.to_string()))
}}
"#
    ))
}

fn normalized_kinds() -> Result<Vec<Value>, RegistryError> {
    let action_codes = actions()?
        .into_iter()
        .map(|action| (action.action_key, action.action_code))
        .collect::<std::collections::HashMap<_, _>>();
    kinds()?
        .into_iter()
        .map(|kind| {
            let fields = kind
                .fields
                .into_iter()
                .map(|field| {
                    let empty_codes = field
                        .empty_for_actions
                        .iter()
                        .map(|key| {
                            action_codes
                                .get(key)
                                .copied()
                                .ok_or_else(|| RegistryError::UnknownActionKey(key.clone()))
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(json!({
                        "wire_key": field.wire_key,
                        "field_key": field.field_key,
                        "constraint": field.constraint,
                        "required": field.required,
                        "allowed_ints": field.allowed_ints,
                        "exact_bytes": field.exact_bytes,
                        "min_bytes": field.min_bytes,
                        "empty_for_action_codes": empty_codes,
                    }))
                })
                .collect::<Result<Vec<_>, RegistryError>>()?;
            Ok(json!({
                "kind_key": kind.kind_key,
                "kind_code": kind.kind_code,
                "temporary": kind.temporary,
                "fields": fields,
                "optional_pairs": kind.optional_pairs,
            }))
        })
        .collect()
}

fn export_dart_kind_enum() -> Result<String, RegistryError> {
    let mut entries = kinds()?;
    entries.sort_by_key(|kind| kind.kind_code);
    let mut out = String::from("enum QrKind {\n");
    let last_index = entries.len().saturating_sub(1);
    for (index, kind) in entries.iter().enumerate() {
        out.push_str(&format!(
            "  {}({}, temporary: {}){}\n",
            dart_lower_camel(&kind.kind_key),
            kind.kind_code,
            kind.temporary,
            if index == last_index { ";" } else { "," }
        ));
    }
    out.push_str(
        r#"
  const QrKind(this.code, {required this.temporary});
  final int code;
  final bool temporary;
  bool get fixed => !temporary;

  static QrKind fromWire(Object? wire) {
    if (wire is! int) throw FormatException('k 必须为整数,实际: $wire');
    for (final kind in values) {
      if (kind.code == wire) return kind;
    }
    throw FormatException('未知 k: $wire');
  }
}
"#,
    );
    Ok(out)
}

fn dart_lower_camel(value: &str) -> String {
    let mut parts = value.split('_');
    let mut out = parts.next().unwrap_or_default().to_owned();
    for part in parts {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.extend(chars);
        }
    }
    out
}

fn rust_upper_camel(value: &str) -> String {
    value
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect()
}

/// 导出 Dart registry 生成文件。
///
/// 该产物是 CitizenApp / CitizenWallet 的唯一扫码动作与中文字段表来源：
/// 两端 UI 样式可以不同，但动作码、action_key、中文动作名、字段中文名必须逐字节一致。
pub fn export_registry_dart() -> Result<String, RegistryError> {
    let mut actions = actions()?;
    actions.sort_by_key(|action| action.action_code);

    let mut fields = fields()?;
    fields.sort_by(|left, right| left.field_key.cmp(&right.field_key));

    let mut reject_reasons = reject_reasons()?;
    reject_reasons.sort_by(|left, right| left.reject_reason_key.cmp(&right.reject_reason_key));

    let mut out = String::new();
    out.push_str("// 本文件由 citizenchain/crates/qr-protocol 生成，禁止手改。\n");
    out.push_str(
        "// 扫码签名动作、中文动作名、字段中文名和固定展示值的唯一真源在 registry/*.yaml。\n\n",
    );
    out.push_str("class GeneratedQrActionRegistry {\n");
    out.push_str("  const GeneratedQrActionRegistry._();\n\n");

    out.push_str("  static const Map<int, String> actionKeyByCode = {\n");
    for action in &actions {
        out.push_str(&format!(
            "    {}: {},\n",
            dart_action_code(action.action_code),
            dart_string(&action.action_key)
        ));
    }
    out.push_str("  };\n\n");

    out.push_str("  static const Map<String, int> actionCodeByKey = {\n");
    for action in &actions {
        out.push_str(&format!(
            "    {}: {},\n",
            dart_string(&action.action_key),
            dart_action_code(action.action_code)
        ));
    }
    out.push_str("  };\n\n");

    out.push_str("  static const Map<String, String> actionLabelZhByKey = {\n");
    for action in &actions {
        out.push_str(&format!(
            "    {}: {},\n",
            dart_string(&action.action_key),
            dart_string(&action.action_label_zh)
        ));
    }
    out.push_str("  };\n\n");

    out.push_str("  static const Map<String, String> fieldLabelZhByKey = {\n");
    for field in &fields {
        out.push_str(&format!(
            "    {}: {},\n",
            dart_string(&field.field_key),
            dart_string(&field.field_label_zh)
        ));
    }
    out.push_str("  };\n\n");

    out.push_str("  static const Map<String, String> fieldValueZhByKey = {\n");
    for field in fields.iter().filter(|field| field.field_value_zh.is_some()) {
        // 上方过滤已保证字段值存在；保留防御分支，避免生成器使用断言式解包。
        let Some(field_value_zh) = field.field_value_zh.as_ref() else {
            continue;
        };
        out.push_str(&format!(
            "    {}: {},\n",
            dart_string(&field.field_key),
            dart_string(field_value_zh)
        ));
    }
    out.push_str("  };\n\n");

    out.push_str("  static const Map<String, String> rejectReasonZhByKey = {\n");
    for reason in &reject_reasons {
        out.push_str(&format!(
            "    {}: {},\n",
            dart_string(&reason.reject_reason_key),
            dart_string(&reason.reject_reason_zh)
        ));
    }
    out.push_str("  };\n\n");

    out.push_str("  static const Set<int> hashOnlyActionCodes = {\n");
    for action in actions.iter().filter(|action| action.hash_only_allowed) {
        out.push_str(&format!("    {},\n", dart_action_code(action.action_code)));
    }
    out.push_str("  };\n\n");

    out.push_str("  static String? actionKeyForCode(int actionCode) =>\n");
    out.push_str("      actionKeyByCode[actionCode];\n\n");
    out.push_str(
        "  static int? actionCodeForKey(String actionKey) => actionCodeByKey[actionKey];\n\n",
    );
    out.push_str("  static String? actionLabelForKey(String actionKey) =>\n");
    out.push_str("      actionLabelZhByKey[actionKey];\n\n");
    out.push_str("  static String? actionLabelForCode(int actionCode) {\n");
    out.push_str("    final key = actionKeyForCode(actionCode);\n");
    out.push_str("    if (key == null) return null;\n");
    out.push_str("    return actionLabelForKey(key);\n");
    out.push_str("  }\n\n");
    out.push_str("  static String? fieldLabelForKey(String fieldKey) =>\n");
    out.push_str("      fieldLabelZhByKey[fieldKey];\n\n");
    out.push_str("  static bool hasFieldLabel(String fieldKey) =>\n");
    out.push_str("      fieldLabelForKey(fieldKey) != null;\n\n");
    out.push_str("  static String? fieldValueForKey(\n");
    out.push_str("    String fieldKey,\n");
    out.push_str("    Map<String, String> values,\n");
    out.push_str("  ) {\n");
    out.push_str("    var template = fieldValueZhByKey[fieldKey];\n");
    out.push_str("    if (template == null) return null;\n");
    out.push_str("    for (final entry in values.entries) {\n");
    out.push_str("      template = template!.replaceAll('{${entry.key}}', entry.value);\n");
    out.push_str("    }\n");
    out.push_str("    return template;\n");
    out.push_str("  }\n\n");
    out.push_str("  static String? rejectReasonForKey(String reasonKey) =>\n");
    out.push_str("      rejectReasonZhByKey[reasonKey];\n\n");
    out.push_str("  static bool isHashOnlyAction(int actionCode) =>\n");
    out.push_str("      hashOnlyActionCodes.contains(actionCode);\n");
    out.push_str("}\n");

    Ok(out)
}

fn dart_action_code(action_code: u16) -> String {
    if action_code >= 0x0100 {
        format!("0x{action_code:04x}")
    } else {
        action_code.to_string()
    }
}

fn dart_string(value: &str) -> String {
    let mut out = String::from("'");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '$' => out.push_str("\\$"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('\'');
    out
}
