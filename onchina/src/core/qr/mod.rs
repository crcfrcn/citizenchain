//! QR_V1 统一二维码协议 envelope。
//!
//! 唯一事实源：`citizenchain/crates/qr-protocol/registry.json`。
//! 本模块只保留 OnChina 后端需要的紧凑签名请求/响应结构。

mod generated;
mod sign_request;

pub use generated::QrKind;
pub(crate) use sign_request::{
    build_domain_sign_request_bytes, build_sign_request, build_sign_request_bytes,
};

use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

pub const QR_V1: &str = "QR_V1";

fn registry_action_code(action_key: &str) -> u16 {
    qr_protocol::action_by_key(action_key)
        .unwrap_or_else(|error| panic!("QR action registry 缺少 {action_key}: {error}"))
        .action_code
}

pub(crate) fn action_label_zh(action_key: &str) -> String {
    qr_protocol::action_by_key(action_key)
        .unwrap_or_else(|error| panic!("QR action registry 缺少 {action_key}: {error}"))
        .action_label_zh
}

static ACTION_LOGIN_CODE: LazyLock<u16> = LazyLock::new(|| registry_action_code("login"));
static ACTION_CITIZEN_IDENTITY_CODE: LazyLock<u16> =
    LazyLock::new(|| registry_action_code("citizen_identity"));
static ACTION_ONCHINA_ADMIN_CODE: LazyLock<u16> =
    LazyLock::new(|| registry_action_code("onchina_admin_action"));
static ACTION_CITIZEN_OCCUPY_CODE: LazyLock<u16> =
    LazyLock::new(|| registry_action_code("citizen_occupy"));
static ACTION_CITIZEN_REBIND_CODE: LazyLock<u16> =
    LazyLock::new(|| registry_action_code("citizen_rebind"));

pub(crate) fn action_login() -> u16 {
    *ACTION_LOGIN_CODE
}

/// 公民链上身份 payload 确认(非链交易,b.d=VotingIdentityPayload SCALE bytes),
/// 公民钱包对 `signing_message(OP_SIGN_CITIZEN_IDENTITY, b.d)` 签名。
pub(crate) fn action_citizen_identity() -> u16 {
    *ACTION_CITIZEN_IDENTITY_CODE
}

/// 注册局管理员治理文本确认(非链动作,b.d=onchina_admin_governance canonical JSON),
/// 对应 qr-action-registry.md 非链动作码 a=3。
pub(crate) fn action_onchina_admin() -> u16 {
    *ACTION_ONCHINA_ADMIN_CODE
}

/// 注册局代办首次绑定的公民域签名。b.d 是含零 account_id 槽的
/// CidOccupyAuthorization 模板，公民钱包替换该槽后按 OP_SIGN_CID_OCCUPY 签名。
pub(crate) fn action_occupy() -> u16 {
    *ACTION_CITIZEN_OCCUPY_CODE
}

/// 注册局代办换绑钱包的新账户域签名。b.d 是含零 new_account_id 槽的
/// CidRebindAuthorization 模板，域为 OP_SIGN_CID_ADMIN_REBIND。
pub(crate) fn action_rebind() -> u16 {
    *ACTION_CITIZEN_REBIND_CODE
}
// 链交易动作码(机构治理/管理员集合)不在此处发明扁平常量:
// 统一用 `core::institution_call::chain_action_code(pallet,call)` 派生(b.a 与 b.d 同源),
// 旧机构直接创建 call 5 已关闭。机构管理员变更由 entity 治理结果驱动，
// 不存在 public/private admins 的直接集合变更动作。
// 详见 qr-action-registry.md「链交易动作码」。

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignRequestBody {
    /// a:动作码。登录=1,公民绑定=2,链上中国平台管理员动作=3。
    #[serde(rename = "a")]
    pub action: u16,
    /// g:签名算法。1 固定为 sr25519。
    #[serde(rename = "g")]
    pub sig_alg: u8,
    /// u:目标/实际签名者公钥,32B base64url(no padding)。
    #[serde(rename = "u")]
    pub account_id: String,
    /// d:待签 payload bytes 的 base64url(no padding)。
    #[serde(rename = "d")]
    pub payload: String,
}

#[derive(Debug, Clone)]
pub struct SignResponseBody {
    /// 0x + 32B hex 公钥。parse 时由 b.u 解码得到。
    pub account_id: String,
    /// 0x + 64B hex 签名。parse 时由 b.s 解码得到。
    pub signature: String,
    /// 换绑数据交接的当前账户签名；注册局授权换绑本身不依赖这两个可选字段。
    #[allow(dead_code)]
    pub current_account_id: Option<String>,
    #[allow(dead_code)]
    pub current_account_signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrEnvelope<B> {
    /// p:协议版本,固定 QR_V1。
    #[serde(rename = "p")]
    pub proto: String,
    /// k:二维码场景数字码。
    #[serde(rename = "k")]
    pub kind: u8,
    /// i:临时二维码一次性 id。
    #[serde(rename = "i", skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,
    /// e:过期秒级时间戳。
    #[serde(rename = "e", skip_serializing_if = "Option::is_none", default)]
    pub expires_at: Option<i64>,
    /// b:场景 body。
    #[serde(rename = "b")]
    pub body: B,
}

pub type SignRequestEnvelope = QrEnvelope<SignRequestBody>;
pub type SignResponseEnvelope = QrEnvelope<SignResponseBody>;

impl SignRequestEnvelope {
    pub fn new(id: String, _issued_at: i64, expires_at: i64, body: SignRequestBody) -> Self {
        Self {
            proto: QR_V1.to_string(),
            kind: QrKind::SignRequest.code(),
            id: Some(id),
            expires_at: Some(expires_at),
            body,
        }
    }
}

/// 登录签名请求 payload 固定为 `system` 的 UTF-8 字节。
///
/// `u` 必须是钱包码预先确定的目标账户公钥；登录请求不允许存在空目标或任意钱包签名。
pub fn login_request_body(
    system: &str,
    target_account_id: &str,
) -> Result<SignRequestBody, QrParseError> {
    let account_id = public_key_hex_to_b64(target_account_id)
        .ok_or_else(|| QrParseError::BadField("目标 account_id 必须为 32 字节规范账户".into()))?;
    Ok(SignRequestBody {
        action: action_login(),
        sig_alg: 1,
        account_id,
        payload: bytes_to_b64(system.as_bytes()),
    })
}

/// 唯一的签名原文拼接函数。
///
/// 格式(与 Dart/TS 逐字节一致):
/// ```text
/// QR_V1|<k>|<id>|<system 或空>|<expires_at 或 0>|<principal>
/// ```
/// `principal` 去掉 `0x` 前缀,小写。
pub fn build_signature_message(
    kind: QrKind,
    id: &str,
    system: Option<&str>,
    expires_at: Option<i64>,
    principal: &str,
) -> String {
    let sys = system.unwrap_or("");
    let exp = expires_at.unwrap_or(0);
    let pp = normalize_hex_no_prefix(principal);
    format!("{}|{}|{}|{}|{}|{}", QR_V1, kind.code(), id, sys, exp, pp)
}

#[derive(Debug)]
// 四个错误名与对外诊断文案逐项对应，统一前缀用于明确它们都是拒绝原因而非可恢复状态。
#[allow(clippy::enum_variant_names)]
pub enum QrParseError {
    BadJson(String),
    BadProto(String),
    BadKind(String),
    BadField(String),
}

impl std::fmt::Display for QrParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadJson(m) => write!(f, "QR JSON 非法: {}", m),
            Self::BadProto(m) => write!(f, "p 必须为 QR_V1,实际: {}", m),
            Self::BadKind(m) => write!(f, "未知 k: {}", m),
            Self::BadField(m) => write!(f, "字段错误: {}", m),
        }
    }
}

impl std::error::Error for QrParseError {}

// 未知字段一律拒绝:与同文件 AccountIdCodeBody 同口径。k=2 是权限最高的码型之一,
// 此前是本文件唯一没有这道闸的 body,已废止字段能从这里悄悄混进来。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompactResponseBody {
    u: String,
    s: String,
    o: Option<String>,
    r: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountIdCodeEnvelope {
    #[serde(rename = "p")]
    proto: String,
    #[serde(rename = "k")]
    kind: u8,
    #[serde(rename = "b")]
    body: AccountIdCodeBody,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountIdCodeBody {
    /// 单字母键 `n` = `account_id`，全码型统一规则见共享 QR registry。
    #[serde(rename = "n")]
    account_id: String,
}

/// 严格解析 QR_V1/k=5 账户码，返回规范 `account_id`。
///
/// **钱包没有码,账户才有码** —— 一个钱包由多个账户组成,码描述的始终是某一个账户。
///
/// 只接受 `p/k/b.n`；裸 SS58、裸公钥、字段别名、临时码字段（`i`/`e`）和
/// 未知字段全部拒绝。已废止的旧 `k=5 chat_node_pairing` 载荷会因 body 字段集不匹配
/// 被 `deny_unknown_fields` 拒掉，不需要专门的拒绝分支。
///
/// 账户码不携带 CID 与昵称：后端从链上管理员名册读取身份字段，并在同一个 finalized
/// 区块解析带 CID 管理员的当前绑定账户。私权 LR 的 CID 来自机构法定代表人记录；冻结
/// 公权无 CID 管理员和私权非 LR 无 CID 管理员按名册账户处理；个人多签不进入 OnChina。
/// 管理员私钥保管在离线的 CitizenWallet，账户码由它自己就能出示，登录第 1 步不再
/// 需要联网热钱包参与。
pub(crate) fn parse_account_id_code(raw: &str) -> Result<String, QrParseError> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|error| QrParseError::BadJson(error.to_string()))?;
    let kind = generated::validate(&value)?;
    if kind.kind_code != QrKind::AccountIdCode.code() {
        return Err(QrParseError::BadKind(kind.kind_code.to_string()));
    }
    let envelope: AccountIdCodeEnvelope =
        serde_json::from_value(value).map_err(|error| QrParseError::BadJson(error.to_string()))?;
    if envelope.proto != QR_V1 {
        return Err(QrParseError::BadProto(envelope.proto));
    }
    if envelope.kind != QrKind::AccountIdCode.code() {
        return Err(QrParseError::BadKind(envelope.kind.to_string()));
    }
    let account_id = envelope.body.account_id.as_str();
    let normalized = crate::crypto::pubkey::normalize_account_id(account_id)
        .ok_or_else(|| QrParseError::BadField("b.n 必须为小写 0x 加 64 位十六进制".into()))?;
    if normalized != account_id {
        return Err(QrParseError::BadField(
            "b.n 必须为小写 0x 加 64 位十六进制".into(),
        ));
    }
    Ok(normalized)
}

/// 解析 QR_V1/k=2 签名响应。后端收到签名方响应后使用。
pub fn parse_sign_response(raw: &str) -> Result<SignResponseEnvelope, QrParseError> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| QrParseError::BadJson(e.to_string()))?;
    let kind = generated::validate(&value)?;
    if kind.kind_code != QrKind::SignResponse.code() {
        return Err(QrParseError::BadKind(kind.kind_code.to_string()));
    }
    let obj = value
        .as_object()
        .ok_or_else(|| QrParseError::BadJson("不是对象".into()))?;

    match obj.get("p").and_then(|v| v.as_str()) {
        Some(QR_V1) => {}
        other => return Err(QrParseError::BadProto(format!("{:?}", other))),
    }
    match obj.get("k").and_then(|v| v.as_u64()) {
        Some(2) => {}
        other => return Err(QrParseError::BadKind(format!("{:?}", other))),
    }

    let id = obj
        .get("i")
        .and_then(|v| v.as_str())
        .ok_or_else(|| QrParseError::BadField("i 必填".into()))?
        .to_string();
    let expires_at = obj
        .get("e")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| QrParseError::BadField("e 必填整数".into()))?;
    let body_val = obj
        .get("b")
        .ok_or_else(|| QrParseError::BadField("b 必填".into()))?;
    let body: CompactResponseBody = serde_json::from_value(body_val.clone())
        .map_err(|e| QrParseError::BadField(format!("b: {}", e)))?;
    let account_id = b64_to_prefixed_hex(&body.u, 32, "b.u")?;
    let signature = b64_to_prefixed_hex(&body.s, 64, "b.s")?;
    if body.o.is_some() != body.r.is_some() {
        return Err(QrParseError::BadField(
            "b.o / b.r 必须同时出现或同时省略".into(),
        ));
    }
    let current_account_id = body
        .o
        .as_deref()
        .map(|value| b64_to_prefixed_hex(value, 32, "b.o"))
        .transpose()?;
    let current_account_signature = body
        .r
        .as_deref()
        .map(|value| b64_to_prefixed_hex(value, 64, "b.r"))
        .transpose()?;

    Ok(SignResponseEnvelope {
        proto: QR_V1.to_string(),
        kind: QrKind::SignResponse.code(),
        id: Some(id),
        expires_at: Some(expires_at),
        body: SignResponseBody {
            account_id,
            signature,
            current_account_id,
            current_account_signature,
        },
    })
}

pub(crate) fn public_key_hex_to_b64(value: &str) -> Option<String> {
    let cleaned = normalize_hex_no_prefix(value);
    let bytes = hex::decode(cleaned).ok()?;
    qr_protocol::public_key_b64(&bytes, "b.u").ok()
}

pub(crate) fn bytes_to_b64(bytes: &[u8]) -> String {
    qr_protocol::bytes_to_b64(bytes)
}

/// QR body 定长字段解码。实现在 qr-protocol 单源,这里只把错误映射成本模块错误类型;
/// 不得在此重写解码逻辑(与 node 端分裂会导致同一二维码一端过一端拒)。
fn b64_to_prefixed_hex(value: &str, len: usize, field: &str) -> Result<String, QrParseError> {
    qr_protocol::b64_to_prefixed_hex(value, len, field)
        .map_err(|error| QrParseError::BadField(error.to_string()))
}

fn normalize_hex_no_prefix(value: &str) -> String {
    value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value)
        .to_lowercase()
}

#[cfg(test)]
// 二维码协议夹具必须严格成立，断言式解包用于让协议偏差立即失败。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

    const ACCOUNT_ID: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";

    fn account_id_code_json() -> String {
        serde_json::json!({
            "p": QR_V1,
            "k": QrKind::AccountIdCode.code(),
            "b": { "n": ACCOUNT_ID }
        })
        .to_string()
    }

    #[test]
    fn account_id_code_parser_returns_canonical_account_id() {
        let account_id = parse_account_id_code(&account_id_code_json()).expect("完整账户码应通过");
        assert_eq!(account_id, ACCOUNT_ID);
    }

    #[test]
    fn account_id_code_parser_rejects_aliases_extra_and_temporary_fields() {
        // 字段别名。
        let mut alias: serde_json::Value =
            serde_json::from_str(&account_id_code_json()).expect("测试 JSON");
        let body = alias["b"].as_object_mut().expect("测试 body");
        let account_id = body.remove("n").expect("测试账户");
        body.insert("account".into(), account_id);
        assert!(parse_account_id_code(&alias.to_string()).is_err());

        // 固定码不得带时效字段。
        let mut temporary: serde_json::Value =
            serde_json::from_str(&account_id_code_json()).expect("测试 JSON");
        temporary["i"] = serde_json::json!("forbidden");
        assert!(parse_account_id_code(&temporary.to_string()).is_err());

        // 钱包码只声明账户,不得夹带身份字段。
        let mut extra: serde_json::Value =
            serde_json::from_str(&account_id_code_json()).expect("测试 JSON");
        extra["b"]["display_name"] = serde_json::json!("测试管理员");
        assert!(parse_account_id_code(&extra.to_string()).is_err());

        let mut with_cid: serde_json::Value =
            serde_json::from_str(&account_id_code_json()).expect("测试 JSON");
        with_cid["b"]["cid_number"] = serde_json::json!("CN001-CTZN-000000001-2026");
        assert!(parse_account_id_code(&with_cid.to_string()).is_err());
    }

    #[test]
    fn account_id_code_parser_rejects_noncanonical_account_id() {
        // SS58 不是授权主键,钱包码只收 account_id。
        let ss58_address =
            crate::crypto::pubkey::account_id_to_ss58(ACCOUNT_ID).expect("测试账户可转 SS58");
        let mut as_ss58: serde_json::Value =
            serde_json::from_str(&account_id_code_json()).expect("测试 JSON");
        as_ss58["b"]["n"] = serde_json::json!(ss58_address);
        assert!(parse_account_id_code(&as_ss58.to_string()).is_err());

        // 大写 hex、缺 0x、首尾空格全部拒绝。
        for bad in [
            ACCOUNT_ID.to_uppercase(),
            ACCOUNT_ID.trim_start_matches("0x").to_string(),
            format!(" {ACCOUNT_ID}"),
        ] {
            let mut value: serde_json::Value =
                serde_json::from_str(&account_id_code_json()).expect("测试 JSON");
            value["b"]["n"] = serde_json::json!(bad);
            assert!(parse_account_id_code(&value.to_string()).is_err());
        }
    }

    #[test]
    fn account_id_code_parser_rejects_legacy_chat_node_pairing_payload() {
        // k=5 已从已废止的 chat_node_pairing 回收给钱包码;旧载荷靠 body 字段集拒绝。
        let legacy = serde_json::json!({
            "p": QR_V1,
            "k": 5,
            "b": {
                "node_peer_id": "12D3Koo",
                "node_multiaddr": "/ip4/1.2.3.4/tcp/30333",
                "endpoint_kind": "ip4"
            }
        })
        .to_string();
        assert!(parse_account_id_code(&legacy).is_err());
    }

    #[test]
    fn account_id_code_parser_rejects_other_kinds() {
        for kind in [
            QrKind::SignRequest.code(),
            QrKind::SignResponse.code(),
            QrKind::UserContact.code(),
            QrKind::UserTransfer.code(),
        ] {
            let mut value: serde_json::Value =
                serde_json::from_str(&account_id_code_json()).expect("测试 JSON");
            value["k"] = serde_json::json!(kind);
            assert!(parse_account_id_code(&value.to_string()).is_err());
        }
    }

    #[test]
    fn login_request_always_targets_account_id_code_account() {
        let body = login_request_body("onchina", ACCOUNT_ID).expect("规范账户应生成登录请求");
        assert_eq!(
            b64_to_prefixed_hex(&body.account_id, 32, "b.u").expect("b.u 应可解码"),
            ACCOUNT_ID
        );
    }

    #[test]
    fn sign_response_parses_paired_current_account_handover_signature() {
        let current_account = [0x22u8; 32];
        let current_account_signature = [0x33u8; 64];
        let raw = serde_json::json!({
            "p": QR_V1,
            "k": QrKind::SignResponse.code(),
            "i": "citizen-rebind-response-1",
            "e": 1_800_000_000i64,
            "b": {
                "u": URL_SAFE_NO_PAD.encode([0x11u8; 32]),
                "s": URL_SAFE_NO_PAD.encode([0x44u8; 64]),
                "o": URL_SAFE_NO_PAD.encode(current_account),
                "r": URL_SAFE_NO_PAD.encode(current_account_signature),
            }
        })
        .to_string();
        let parsed = parse_sign_response(raw.as_str()).expect("双签响应应通过");
        let current_account_hex = format!("0x{}", hex::encode(current_account));
        let current_account_signature_hex = format!("0x{}", hex::encode(current_account_signature));
        assert_eq!(
            parsed.body.current_account_id.as_deref(),
            Some(current_account_hex.as_str())
        );
        assert_eq!(
            parsed.body.current_account_signature.as_deref(),
            Some(current_account_signature_hex.as_str())
        );

        let mut missing_current_account_signature: serde_json::Value =
            serde_json::from_str(raw.as_str()).expect("测试 JSON");
        missing_current_account_signature["b"]
            .as_object_mut()
            .expect("测试 body")
            .remove("r");
        assert!(
            parse_sign_response(missing_current_account_signature.to_string().as_str()).is_err()
        );
    }

    #[test]
    fn domain_sign_request_leaves_account_empty_and_carries_exact_authorization_template() {
        // b.u 留空；b.d 必须原样携带完整授权模板，构造器不得重排或补写业务字段。
        let mut authorization_template = vec![0xaau8; 32];
        authorization_template.extend_from_slice(b"\x68CN220-CTZN2-198805200-2026");
        authorization_template.extend_from_slice(&[0u8; 32]);
        authorization_template.extend_from_slice(&0u64.to_le_bytes());
        authorization_template.extend_from_slice(&1_800_000_000u64.to_le_bytes());
        let json = build_domain_sign_request_bytes(
            "citizen-occupy-req-1",
            1_800_000_000,
            &authorization_template,
            action_occupy(),
        )
        .expect("域签名请求应生成");
        let value: serde_json::Value = serde_json::from_str(&json).expect("生成的应是合法 JSON");
        assert_eq!(value["p"], QR_V1);
        assert_eq!(value["k"], 1);
        assert_eq!(value["b"]["u"], "");
        assert_eq!(
            value["b"]["a"].as_u64().expect("动作码"),
            u64::from(action_occupy())
        );
        let encoded_payload = value["b"]["d"].as_str().expect("b.d 应是字符串");
        assert_eq!(
            URL_SAFE_NO_PAD
                .decode(encoded_payload)
                .expect("b.d 应可解码"),
            authorization_template,
        );
    }

    #[test]
    fn occupy_and_rebind_actions_are_distinct_registered_codes() {
        assert_ne!(action_occupy(), action_rebind());
        assert_eq!(action_occupy(), 10);
        assert_eq!(action_rebind(), 11);
    }
}
