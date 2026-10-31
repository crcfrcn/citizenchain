//! 管理员敏感动作的扫码签名挑战与一次性安全授权模型。
//!
//! 这些结构服务统一管理员安全动作，因此由平台 authentication 持有。
//! step-up = 会话 + 冷钱包扫码签名。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::authentication::operation_auth::AdminOperationAuth;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct AdminActionChallenge {
    pub action_id: String,
    pub action_type: String,
    pub actor_account_id: String,
    pub actor_institution_code: String,
    pub actor_cid_number: String,
    pub actor_province_name: String,
    #[serde(default)]
    pub actor_city_name: Option<String>,
    pub auth_type: AdminOperationAuth,
    pub target: String,
    pub payload_text: String,
    pub payload_hash: String,
    pub before_hash: String,
    pub after_hash: String,
    pub request_payload: serde_json::Value,
    pub issued_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub expires_at: DateTime<Utc>,
    pub consumed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminSecurityGrant {
    pub(crate) grant_id: String,
    pub(crate) action_type: String,
    pub(crate) actor_account_id: String,
    pub(crate) actor_institution_code: String,
    pub(crate) actor_cid_number: String,
    pub(crate) actor_province_name: String,
    #[serde(default)]
    pub(crate) actor_city_name: Option<String>,
    pub(crate) auth_type: AdminOperationAuth,
    pub(crate) target: String,
    pub(crate) payload_hash: String,
    pub(crate) issued_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub(crate) expires_at: DateTime<Utc>,
    pub(crate) consumed: bool,
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/authentication_security_model.rs"]
mod tests;
