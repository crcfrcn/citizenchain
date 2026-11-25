//! 定义正常轮换、紧急恢复及本机待更换记录。

use grandpakey::GrandpaKeyChangeKind;

use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// 本机换钥流程的原有正常轮换和紧急恢复类型。
pub enum LocalChangeKind {
    /// 正常密钥轮换。
    RoutineRotation,
    /// 紧急密钥恢复。
    EmergencyRecovery,
}

impl From<LocalChangeKind> for GrandpaKeyChangeKind {
    fn from(value: LocalChangeKind) -> Self {
        match value {
            crate::key::types::LocalChangeKind::RoutineRotation => Self::RoutineRotation,
            crate::key::types::LocalChangeKind::EmergencyRecovery => Self::EmergencyRecovery,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
/// 持久化本机待生效换钥的公开状态，不含私钥。
pub struct PendingKeyChange {
    /// 发起换钥的机构CID。
    pub actor_cid_number: String,
    /// 所属机构的规范简称。
    pub cid_short_name: String,
    /// 原GRANDPA公开密钥。
    pub old_public_key: String,
    /// 候选GRANDPA公开密钥。
    pub new_public_key: String,
    /// 原正常轮换或紧急恢复类型。
    pub change_kind: LocalChangeKind,
    /// 链上持钥证明的一次性nonce。
    pub proof_nonce: u64,
    /// 持钥证明的原区块高度期限。
    pub proof_expires_at: u32,
    /// 已提交换钥交易的哈希。
    pub tx_hash: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
/// 提供原桌面冷签请求及其准确签名上下文。
pub struct GrandpaKeyChangeRequest {
    /// 原QR_V1冷签请求文本。
    pub request_json: String,
    /// 本次冷签请求标识。
    pub request_id: String,
    /// 原待签载荷哈希。
    pub expected_payload_hash: String,
    /// 原链交易nonce。
    pub sign_nonce: u32,
    /// 原链交易签名上下文区块高度。
    pub sign_block_number: u64,
    /// 原GRANDPA公开密钥。
    pub old_public_key: String,
    /// 候选GRANDPA公开密钥。
    pub new_public_key: String,
    /// 链上持钥证明的一次性nonce。
    pub proof_nonce: u64,
    /// 持钥证明的原区块高度期限。
    pub proof_expires_at: u32,
    /// 原正常轮换或紧急恢复类型。
    pub change_kind: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
/// 提供原桌面待生效换钥状态。
pub struct GrandpaKeyChangeStatus {
    /// 是否存在待生效换钥。
    pub pending: bool,
    /// 发起换钥的机构CID。
    pub actor_cid_number: Option<String>,
    /// 原GRANDPA公开密钥。
    pub old_public_key: Option<String>,
    /// 候选GRANDPA公开密钥。
    pub new_public_key: Option<String>,
    /// 原正常轮换或紧急恢复类型。
    pub change_kind: Option<String>,
    /// 已提交换钥交易的哈希。
    pub tx_hash: Option<String>,
}
