//! QR_V1 载荷字段编解码唯一真源。
//!
//! QR body 中的公钥、签名等定长字节字段统一用 base64url(no padding)承载;
//! 进入 Rust 侧后一律转成 ADR-040 规范文本(小写 `0x` + 十六进制)。
//!
//! 本模块是四端 host 侧的唯一实现:`node`(桌面端验签)与 `onchina`(控制台扫码)
//! 必须复用这里的函数,禁止各自再写一份解码。两份实现一旦在长度校验或大小写
//! 上产生分毫差异,同一个二维码就会在一端通过、另一端拒绝。

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde_json::{Map, Value};

use crate::registry::{action_by_key, kind_by_code, QrFieldConstraint, QrKindEntry};

/// 全仓唯一二维码协议标识。
pub const QR_V1: &str = "QR_V1";

/// base64url 定长字段的解码失败原因。
///
/// 只描述"哪个字段、错在哪",不绑定任何调用方的错误类型;
/// `node` 映射成 `String`,`onchina` 映射成 `QrParseError::BadField`。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodecError {
    /// 字段不是合法的 base64url(no padding)。
    #[error("{field} 必须为 base64url(no padding)")]
    NotBase64Url { field: String },
    /// 字段解码后字节长度与协议约定不符。
    #[error("{field} 长度无效:期望 {expected} 字节,实际 {actual} 字节")]
    BadLength {
        field: String,
        expected: usize,
        actual: usize,
    },
    /// envelope/body 结构或声明约束不成立。
    #[error("{0}")]
    Schema(String),
}

/// 按 `registry/kinds.yaml` 严格校验一个 QR_V1 JSON value。
///
/// 本函数只判断协议结构，不决定当前业务入口是否允许该码型。交易、通讯录、登录、签名
/// 等入口必须在拿到 `kind_key` 后自行决定接受或给出什么反馈。
pub fn validate_qr_value(value: &Value) -> Result<QrKindEntry, CodecError> {
    let envelope = value
        .as_object()
        .ok_or_else(|| schema_error("QR 内容不是 JSON 对象"))?;
    if envelope.get("p").and_then(Value::as_str) != Some(QR_V1) {
        return Err(schema_error("p 必须为 QR_V1"));
    }
    let kind_code = envelope
        .get("k")
        .and_then(Value::as_u64)
        .and_then(|code| u8::try_from(code).ok())
        .ok_or_else(|| schema_error("k 必须为 u8 整数"))?;
    let kind = kind_by_code(kind_code).map_err(|error| schema_error(error.to_string()))?;

    let expected_envelope_keys: &[&str] = if kind.temporary {
        &["p", "k", "i", "e", "b"]
    } else {
        &["p", "k", "b"]
    };
    require_exact_keys(envelope, expected_envelope_keys, "QR envelope")?;
    if kind.temporary {
        let id = envelope
            .get("i")
            .and_then(Value::as_str)
            .ok_or_else(|| schema_error("临时码 i 必须为非空字符串"))?;
        if id.is_empty() {
            return Err(schema_error("临时码 i 必须为非空字符串"));
        }
        let expires_at = envelope
            .get("e")
            .and_then(Value::as_i64)
            .ok_or_else(|| schema_error("临时码 e 必须为正整数"))?;
        if expires_at <= 0 {
            return Err(schema_error("临时码 e 必须为正整数"));
        }
    }

    let body = envelope
        .get("b")
        .and_then(Value::as_object)
        .ok_or_else(|| schema_error("b 必须为 JSON 对象"))?;
    validate_body(&kind, body)?;
    Ok(kind)
}

fn validate_body(kind: &QrKindEntry, body: &Map<String, Value>) -> Result<(), CodecError> {
    let allowed = kind
        .fields
        .iter()
        .map(|field| field.wire_key.as_str())
        .collect::<Vec<_>>();
    for key in body.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(schema_error(format!(
                "{}.b 包含未知字段 {key}",
                kind.kind_key
            )));
        }
    }
    for field in kind.fields.iter().filter(|field| field.required) {
        if !body.contains_key(field.wire_key.as_str()) {
            return Err(schema_error(format!(
                "{}.b 缺少字段 {}",
                kind.kind_key, field.wire_key
            )));
        }
    }
    for [left, right] in &kind.optional_pairs {
        if body.contains_key(left) != body.contains_key(right) {
            return Err(schema_error(format!(
                "{}.b 的 {left}/{right} 必须同时出现或同时省略",
                kind.kind_key
            )));
        }
    }

    for field in &kind.fields {
        let Some(value) = body.get(field.wire_key.as_str()) else {
            continue;
        };
        let field_path = format!("{}.b.{}", kind.kind_key, field.wire_key);
        match field.constraint {
            QrFieldConstraint::PositiveInt => {
                if value.as_i64().is_none_or(|number| number <= 0) {
                    return Err(schema_error(format!("{field_path} 必须为正整数")));
                }
            }
            QrFieldConstraint::EnumInt => {
                let number = value
                    .as_i64()
                    .ok_or_else(|| schema_error(format!("{field_path} 必须为整数")))?;
                if !field.allowed_ints.contains(&number) {
                    return Err(schema_error(format!("{field_path} 不在允许枚举中")));
                }
            }
            QrFieldConstraint::SignerPublicKey => {
                let text = value
                    .as_str()
                    .ok_or_else(|| schema_error(format!("{field_path} 必须为字符串")))?;
                let action = body
                    .get("a")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| schema_error("sign_request.b.a 必须为整数"))?;
                let empty_action_codes = field
                    .empty_for_actions
                    .iter()
                    .map(|action_key| {
                        action_by_key(action_key)
                            .map(|entry| i64::from(entry.action_code))
                            .map_err(|error| schema_error(error.to_string()))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if empty_action_codes.contains(&action) {
                    if !text.is_empty() {
                        return Err(schema_error(format!(
                            "{field_path} 在自填账户动作中必须留空"
                        )));
                    }
                } else {
                    validate_b64u(text, Some(PUBLIC_KEY_BYTES), None, &field_path)?;
                }
            }
            QrFieldConstraint::B64uBytes => {
                let text = value
                    .as_str()
                    .ok_or_else(|| schema_error(format!("{field_path} 必须为字符串")))?;
                validate_b64u(text, field.exact_bytes, field.min_bytes, &field_path)?;
            }
            QrFieldConstraint::CidNumber => {
                let text = value
                    .as_str()
                    .ok_or_else(|| schema_error(format!("{field_path} 必须为字符串")))?;
                if !is_cid_number(text) {
                    return Err(schema_error(format!(
                        "{field_path} 必须为 1 到 32 位 ASCII 字母数字与连字符"
                    )));
                }
            }
            QrFieldConstraint::AccountId => {
                let text = value
                    .as_str()
                    .ok_or_else(|| schema_error(format!("{field_path} 必须为字符串")))?;
                if !is_account_id(text) {
                    return Err(schema_error(format!(
                        "{field_path} 必须为小写 0x 加 64 位十六进制"
                    )));
                }
            }
            QrFieldConstraint::TextNonempty => {
                if value.as_str().is_none_or(str::is_empty) {
                    return Err(schema_error(format!("{field_path} 必须为非空字符串")));
                }
            }
            QrFieldConstraint::Text => {
                if !value.is_string() {
                    return Err(schema_error(format!("{field_path} 必须为字符串")));
                }
            }
        }
    }
    Ok(())
}

fn require_exact_keys(
    object: &Map<String, Value>,
    expected: &[&str],
    context: &str,
) -> Result<(), CodecError> {
    if object.len() != expected.len() || object.keys().any(|key| !expected.contains(&key.as_str()))
    {
        return Err(schema_error(format!("{context} 字段集合不符合 QR_V1")));
    }
    Ok(())
}

fn validate_b64u(
    value: &str,
    exact_bytes: Option<usize>,
    min_bytes: Option<usize>,
    field: &str,
) -> Result<(), CodecError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| schema_error(format!("{field} 必须为规范无填充 base64url")))?;
    if URL_SAFE_NO_PAD.encode(&bytes) != value {
        return Err(schema_error(format!("{field} 必须为规范无填充 base64url")));
    }
    if exact_bytes.is_some_and(|expected| bytes.len() != expected) {
        return Err(schema_error(format!(
            "{field} 必须解码为 {} 字节",
            exact_bytes.unwrap_or_default()
        )));
    }
    if min_bytes.is_some_and(|minimum| bytes.len() < minimum) {
        return Err(schema_error(format!(
            "{field} 解码后不得少于 {} 字节",
            min_bytes.unwrap_or_default()
        )));
    }
    Ok(())
}

fn is_account_id(value: &str) -> bool {
    value.len() == 66
        && value.starts_with("0x")
        && value[2..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_cid_number(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn schema_error(message: impl Into<String>) -> CodecError {
    CodecError::Schema(message.into())
}

/// base64url(no padding)定长字段 → ADR-040 规范文本(小写 `0x` + 十六进制)。
///
/// `expected_len` 是解码后的**字节**数(公钥 32、签名 64);`field` 用于错误信息定位,
/// 传 QR body 中的字段名(如 `b.u`、`b.s`)。
pub fn b64_to_prefixed_hex(
    value: &str,
    expected_len: usize,
    field: &str,
) -> Result<String, CodecError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| CodecError::NotBase64Url {
            field: field.to_string(),
        })?;
    if bytes.len() != expected_len {
        return Err(CodecError::BadLength {
            field: field.to_string(),
            expected: expected_len,
            actual: bytes.len(),
        });
    }
    Ok(format!("0x{}", hex::encode(bytes)))
}

/// 字节 → base64url(no padding)。QR body 写入侧的唯一编码入口。
pub fn bytes_to_b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// 32 字节公钥 → base64url(no padding);长度不符即拒绝,不做截断或补齐。
pub fn public_key_b64(public_key_bytes: &[u8], field: &str) -> Result<String, CodecError> {
    if public_key_bytes.len() != PUBLIC_KEY_BYTES {
        return Err(CodecError::BadLength {
            field: field.to_string(),
            expected: PUBLIC_KEY_BYTES,
            actual: public_key_bytes.len(),
        });
    }
    Ok(bytes_to_b64(public_key_bytes))
}

/// 公钥字节长度(sr25519 / ed25519 均为 32)。
pub const PUBLIC_KEY_BYTES: usize = 32;
/// 签名字节长度(sr25519 / ed25519 均为 64)。
pub const SIGNATURE_BYTES: usize = 64;

#[cfg(test)]
// 编解码夹具异常必须立即中止测试,断言式解包仅限本测试模块。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn decodes_public_key_to_lowercase_prefixed_hex() {
        let raw = [0xABu8; PUBLIC_KEY_BYTES];
        let encoded = bytes_to_b64(&raw);
        let hex_text = b64_to_prefixed_hex(&encoded, PUBLIC_KEY_BYTES, "b.u")
            .expect("32 字节公钥必须解码成功");
        assert_eq!(hex_text, format!("0x{}", "ab".repeat(PUBLIC_KEY_BYTES)));
    }

    #[test]
    fn round_trips_signature_length() {
        let raw = [0x01u8; SIGNATURE_BYTES];
        let encoded = bytes_to_b64(&raw);
        assert!(b64_to_prefixed_hex(&encoded, SIGNATURE_BYTES, "b.s").is_ok());
    }

    #[test]
    fn rejects_wrong_length_without_truncating() {
        let encoded = bytes_to_b64(&[0u8; 31]);
        assert_eq!(
            b64_to_prefixed_hex(&encoded, PUBLIC_KEY_BYTES, "b.u"),
            Err(CodecError::BadLength {
                field: "b.u".to_string(),
                expected: 32,
                actual: 31,
            })
        );
    }

    #[test]
    fn rejects_non_base64url_input() {
        // 标准 base64 的 `+` `/` 与 padding `=` 都不属于 base64url(no padding)。
        assert_eq!(
            b64_to_prefixed_hex("++//", 32, "b.u"),
            Err(CodecError::NotBase64Url {
                field: "b.u".to_string()
            })
        );
    }

    #[test]
    fn public_key_b64_rejects_wrong_length() {
        assert!(public_key_b64(&[0u8; 33], "b.u").is_err());
        assert!(public_key_b64(&[0u8; PUBLIC_KEY_BYTES], "b.u").is_ok());
    }

    #[test]
    fn schema_rejects_unknown_keys_and_noncanonical_account_id() {
        let valid = serde_json::json!({
            "p": QR_V1,
            "k": 5,
            "b": { "n": format!("0x{}", "11".repeat(32)) }
        });
        assert_eq!(
            validate_qr_value(&valid)
                .expect("规范账户码必须通过")
                .kind_key,
            "account_id_code"
        );

        let mut extra = valid.clone();
        extra["b"]["account"] = Value::String("forbidden".into());
        assert!(validate_qr_value(&extra).is_err());

        let mut uppercase = valid;
        uppercase["b"]["n"] = Value::String(format!("0x{}", "AA".repeat(32)));
        assert!(validate_qr_value(&uppercase).is_err());
    }

    #[test]
    fn schema_enforces_signer_public_key_action_condition() {
        let payload = bytes_to_b64(&[0x22]);
        let self_account = serde_json::json!({
            "p": QR_V1,
            "k": 1,
            "i": "request-1",
            "e": 1_800_000_000,
            "b": { "a": 10, "g": 1, "u": "", "d": payload }
        });
        assert!(validate_qr_value(&self_account).is_ok());

        let mut invalid = self_account;
        invalid["b"]["u"] = Value::String(bytes_to_b64(&[0x11; PUBLIC_KEY_BYTES]));
        assert!(validate_qr_value(&invalid).is_err());
    }
}
