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
use std::net::IpAddr;

use crate::registry::{action_by_key, kind_by_code, QrFieldConstraint, QrKindEntry};

/// 全仓唯一二维码协议标识。
pub const QR_V1: &str = "QR_V1";

/// 节点部署批准只在链下验签；前缀与现有 GMB 单字节 op_tag 域绝不重叠。
pub const NODE_DEPLOY_SIGNING_DOMAIN: &[u8] = b"GMB\0TATA_NODE_DEPLOY_V1\0";
const NODE_DEPLOY_PRODUCT: &[u8] = b"citizenchain";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeDeployKind {
    Authority,
    Stake,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeDeployIntent {
    pub kind: NodeDeployKind,
    pub node_id: String,
    pub server_ip: String,
    pub platform: String,
    pub version: String,
    pub artifact_sha256: [u8; 32],
    pub task_id: String,
    pub expires_at: u64,
    pub nonce: [u8; 32],
}

fn node_deploy_error() -> CodecError {
    CodecError::Schema("节点部署批准载荷无效".to_string())
}

fn node_deploy_text(value: &str, limit: usize, first_alnum: bool) -> Result<(), CodecError> {
    let bytes = value.as_bytes();
    if bytes.is_empty()
        || bytes.len() > limit
        || (first_alnum && !bytes[0].is_ascii_alphanumeric())
        || !bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return Err(node_deploy_error());
    }
    Ok(())
}

fn push_node_deploy_text(out: &mut Vec<u8>, value: &str, limit: usize) -> Result<(), CodecError> {
    node_deploy_text(value, limit, true)?;
    out.push(u8::try_from(value.len()).map_err(|_| node_deploy_error())?);
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

/// 固定版本与长度前缀编码；身份、目标、版本、资产和一次性任务共同进入签名字节。
pub fn encode_node_deploy_intent(intent: &NodeDeployIntent) -> Result<Vec<u8>, CodecError> {
    node_deploy_text(&intent.node_id, 64, true)?;
    node_deploy_text(&intent.platform, 32, true)?;
    if intent.version.is_empty()
        || intent.version.len() > 255
        || intent.version.chars().any(char::is_control)
    {
        return Err(node_deploy_error());
    }
    let ip: IpAddr = intent.server_ip.parse().map_err(|_| node_deploy_error())?;
    if ip.to_string() != intent.server_ip
        || intent.task_id.len() != 9
        || intent.task_id.as_bytes()[0] == b'0'
        || !intent.task_id.bytes().all(|b| b.is_ascii_digit())
        || intent.expires_at == 0
        || intent.nonce.iter().all(|b| *b == 0)
    {
        return Err(node_deploy_error());
    }
    let mut out = Vec::with_capacity(256);
    out.push(2);
    out.extend_from_slice(NODE_DEPLOY_PRODUCT);
    out.push(match intent.kind {
        NodeDeployKind::Authority => 1,
        NodeDeployKind::Stake => 2,
    });
    push_node_deploy_text(&mut out, &intent.node_id, 64)?;
    out.push(u8::try_from(intent.server_ip.len()).map_err(|_| node_deploy_error())?);
    out.extend_from_slice(intent.server_ip.as_bytes());
    push_node_deploy_text(&mut out, &intent.platform, 32)?;
    out.push(u8::try_from(intent.version.len()).map_err(|_| node_deploy_error())?);
    out.extend_from_slice(intent.version.as_bytes());
    out.extend_from_slice(&intent.artifact_sha256);
    out.extend_from_slice(intent.task_id.as_bytes());
    out.extend_from_slice(&intent.expires_at.to_le_bytes());
    out.extend_from_slice(&intent.nonce);
    Ok(out)
}

fn take_node_deploy<'a>(
    bytes: &'a [u8],
    pos: &mut usize,
    len: usize,
) -> Result<&'a [u8], CodecError> {
    let end = pos.checked_add(len).ok_or_else(node_deploy_error)?;
    let part = bytes.get(*pos..end).ok_or_else(node_deploy_error)?;
    *pos = end;
    Ok(part)
}

fn read_node_deploy_text(
    bytes: &[u8],
    pos: &mut usize,
    limit: usize,
) -> Result<String, CodecError> {
    let len = usize::from(take_node_deploy(bytes, pos, 1)?[0]);
    if len == 0 || len > limit {
        return Err(node_deploy_error());
    }
    let text =
        std::str::from_utf8(take_node_deploy(bytes, pos, len)?).map_err(|_| node_deploy_error())?;
    Ok(text.to_string())
}

/// 离线审阅端必须消费全部字节，拒绝尾随数据与非规范 IP 文本。
pub fn decode_node_deploy_intent(bytes: &[u8]) -> Result<NodeDeployIntent, CodecError> {
    let mut pos = 0;
    if take_node_deploy(bytes, &mut pos, 1)? != [2]
        || take_node_deploy(bytes, &mut pos, NODE_DEPLOY_PRODUCT.len())? != NODE_DEPLOY_PRODUCT
    {
        return Err(node_deploy_error());
    }
    let kind = match take_node_deploy(bytes, &mut pos, 1)?[0] {
        1 => NodeDeployKind::Authority,
        2 => NodeDeployKind::Stake,
        _ => return Err(node_deploy_error()),
    };
    let node_id = read_node_deploy_text(bytes, &mut pos, 64)?;
    let server_ip = read_node_deploy_text(bytes, &mut pos, 45)?;
    let platform = read_node_deploy_text(bytes, &mut pos, 32)?;
    let version = read_node_deploy_text(bytes, &mut pos, 255)?;
    let artifact_sha256: [u8; 32] = take_node_deploy(bytes, &mut pos, 32)?
        .try_into()
        .map_err(|_| node_deploy_error())?;
    let task_id = std::str::from_utf8(take_node_deploy(bytes, &mut pos, 9)?)
        .map_err(|_| node_deploy_error())?
        .to_string();
    let expires_at = u64::from_le_bytes(
        take_node_deploy(bytes, &mut pos, 8)?
            .try_into()
            .map_err(|_| node_deploy_error())?,
    );
    let nonce: [u8; 32] = take_node_deploy(bytes, &mut pos, 32)?
        .try_into()
        .map_err(|_| node_deploy_error())?;
    if pos != bytes.len() {
        return Err(node_deploy_error());
    }
    let result = NodeDeployIntent {
        kind,
        node_id,
        server_ip,
        platform,
        version,
        artifact_sha256,
        task_id,
        expires_at,
        nonce,
    };
    if encode_node_deploy_intent(&result)? != bytes {
        return Err(node_deploy_error());
    }
    Ok(result)
}

pub fn node_deploy_signing_preimage(payload: &[u8]) -> Result<Vec<u8>, CodecError> {
    decode_node_deploy_intent(payload)?;
    let mut value = Vec::with_capacity(NODE_DEPLOY_SIGNING_DOMAIN.len() + payload.len());
    value.extend_from_slice(NODE_DEPLOY_SIGNING_DOMAIN);
    value.extend_from_slice(payload);
    Ok(value)
}

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
