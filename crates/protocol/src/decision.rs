use serde::{Deserialize, Serialize};

/// 统一扫码签名判定结果。
///
/// 全仓只允许正常和拒绝两种终态；任何“未知但继续签名”“警告但允许签名”
/// 都必须先收敛成 Reject，避免移动端出现第三状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum SignDecision {
    Normal(SignNormal),
    Reject(SignReject),
}

/// 绿色正常态：payload 已解码，动作和字段已完整中文翻译，允许签名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignNormal {
    pub action_key: String,
    pub action_label_zh: String,
    pub fields: Vec<SignDisplayField>,
}

/// 红色拒绝态：任一协议、解码、权限或中文翻译失败，禁止签名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignReject {
    pub reject_reason_key: String,
    pub reject_reason_zh: String,
}

/// 已经通过中文字段表翻译后的展示字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignDisplayField {
    pub field_key: String,
    pub field_label_zh: String,
    pub field_value_zh: String,
}
