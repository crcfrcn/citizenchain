use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

const ACTIONS_YAML: &str = include_str!("../registry/actions.yaml");
const FIELDS_YAML: &str = include_str!("../registry/fields.yaml");
const KINDS_YAML: &str = include_str!("../registry/kinds.yaml");
const REJECT_REASONS_YAML: &str = include_str!("../registry/reject_reasons.yaml");

/// action registry 读取和一致性错误。
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("registry yaml 解析失败: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("registry json 导出失败: {0}")]
    Json(#[from] serde_json::Error),
    #[error("未登记 action_code: {0}")]
    UnknownActionCode(u16),
    #[error("未登记 action_key: {0}")]
    UnknownActionKey(String),
    #[error("未登记字段中文名: {0}")]
    UnknownField(String),
    #[error("未登记拒绝原因: {0}")]
    UnknownRejectReason(String),
    #[error("未登记 QR kind: {0}")]
    UnknownKind(u8),
}

/// QR action 类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    ChainCall,
    OffchainSign,
    HashOnly,
}

/// 统一签名字节规则分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SigningCategory {
    ChainTx,
    OffchainAuth,
    CitizenIdentity,
    OnchinaAdmin,
    AdminActivation,
    AdminDecrypt,
    RuntimeUpgrade,
    SquareAccount,
    CitizenOccupy,
    CitizenRebind,
    SwitchDefaultAccount,
    SquareDeviceBind,
    AccountDataKeyProvision,
    Publish,
}

/// 单个 QR action 登记项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionEntry {
    pub action_key: String,
    #[serde(deserialize_with = "deserialize_action_code")]
    pub action_code: u16,
    pub action_label_zh: String,
    pub kind: ActionKind,
    pub qr_kind: String,
    pub pallet: Option<String>,
    pub call: Option<String>,
    pub decoder: String,
    pub hash_only_allowed: bool,
    pub signing_category: SigningCategory,
    #[serde(default)]
    pub required_fields: Vec<String>,
}

/// 字段中文名登记项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldEntry {
    pub field_key: String,
    pub field_label_zh: String,
    #[serde(default)]
    pub field_value_zh: Option<String>,
}

/// 红色拒绝原因登记项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectReasonEntry {
    pub reject_reason_key: String,
    pub reject_reason_zh: String,
}

/// QR body 字段约束的有限词汇表。
///
/// 这里只允许三端能逐字节等价实现的原语；禁止塞入正则文本或通用表达式语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QrFieldConstraint {
    PositiveInt,
    EnumInt,
    SignerPublicKey,
    B64uBytes,
    CidNumber,
    AccountId,
    TextNonempty,
    Text,
}

/// 单个 QR body wire 字段声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QrFieldEntry {
    pub wire_key: String,
    pub field_key: String,
    pub constraint: QrFieldConstraint,
    #[serde(default = "default_true")]
    pub required: bool,
    #[serde(default)]
    pub allowed_ints: Vec<i64>,
    #[serde(default)]
    pub exact_bytes: Option<usize>,
    #[serde(default)]
    pub min_bytes: Option<usize>,
    #[serde(default)]
    pub empty_for_actions: Vec<String>,
}

/// QR_V1 单个码型声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QrKindEntry {
    pub kind_key: String,
    pub kind_code: u8,
    pub temporary: bool,
    pub fields: Vec<QrFieldEntry>,
    #[serde(default)]
    pub optional_pairs: Vec<[String; 2]>,
}

const fn default_true() -> bool {
    true
}

pub fn actions() -> Result<Vec<ActionEntry>, RegistryError> {
    Ok(serde_yaml::from_str(ACTIONS_YAML)?)
}

pub fn fields() -> Result<Vec<FieldEntry>, RegistryError> {
    Ok(serde_yaml::from_str(FIELDS_YAML)?)
}

pub fn reject_reasons() -> Result<Vec<RejectReasonEntry>, RegistryError> {
    Ok(serde_yaml::from_str(REJECT_REASONS_YAML)?)
}

pub fn kinds() -> Result<Vec<QrKindEntry>, RegistryError> {
    Ok(serde_yaml::from_str(KINDS_YAML)?)
}

pub fn kind_by_code(kind_code: u8) -> Result<QrKindEntry, RegistryError> {
    kinds()?
        .into_iter()
        .find(|kind| kind.kind_code == kind_code)
        .ok_or(RegistryError::UnknownKind(kind_code))
}

pub fn action_by_code(action_code: u16) -> Result<ActionEntry, RegistryError> {
    actions()?
        .into_iter()
        .find(|action| action.action_code == action_code)
        .ok_or(RegistryError::UnknownActionCode(action_code))
}

pub fn action_by_key(action_key: &str) -> Result<ActionEntry, RegistryError> {
    actions()?
        .into_iter()
        .find(|action| action.action_key == action_key)
        .ok_or_else(|| RegistryError::UnknownActionKey(action_key.to_owned()))
}

pub fn field_label_zh(field_key: &str) -> Result<String, RegistryError> {
    let field_map: HashMap<_, _> = fields()?
        .into_iter()
        .map(|field| (field.field_key, field.field_label_zh))
        .collect();
    field_map
        .get(field_key)
        .cloned()
        .ok_or_else(|| RegistryError::UnknownField(field_key.to_owned()))
}

pub fn reject_reason_zh(reject_reason_key: &str) -> Result<String, RegistryError> {
    let reason_map: HashMap<_, _> = reject_reasons()?
        .into_iter()
        .map(|reason| (reason.reject_reason_key, reason.reject_reason_zh))
        .collect();
    reason_map
        .get(reject_reason_key)
        .cloned()
        .ok_or_else(|| RegistryError::UnknownRejectReason(reject_reason_key.to_owned()))
}

fn deserialize_action_code<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_yaml::Value::deserialize(deserializer)?;
    match value {
        serde_yaml::Value::Number(number) => number
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .ok_or_else(|| serde::de::Error::custom("action_code 超出 u16 范围")),
        serde_yaml::Value::String(text) => {
            let trimmed = text.trim();
            let parsed = if let Some(hex) = trimmed
                .strip_prefix("0x")
                .or_else(|| trimmed.strip_prefix("0X"))
            {
                u16::from_str_radix(hex, 16)
            } else {
                trimmed.parse::<u16>()
            };
            parsed.map_err(|_| serde::de::Error::custom("action_code 不是合法数字"))
        }
        _ => Err(serde::de::Error::custom("action_code 必须是数字或字符串")),
    }
}
