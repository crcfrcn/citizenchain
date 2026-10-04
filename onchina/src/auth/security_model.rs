//! 管理员敏感动作的扫码签名挑战与一次性安全授权模型。
//!
//! 这些结构服务机构管理员安全动作,因此归属 `admins`。
//! step-up = 会话 + 冷钱包扫码签名。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::operation_auth::AdminOperationAuth;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AdminActionChallenge {
    pub(crate) action_id: String,
    pub(crate) action_type: String,
    pub(crate) actor_account_id: String,
    pub(crate) actor_institution_code: String,
    pub(crate) actor_cid_number: String,
    pub(crate) actor_province_name: String,
    #[serde(default)]
    pub(crate) actor_city_name: Option<String>,
    pub(crate) auth_type: AdminOperationAuth,
    pub(crate) target: String,
    pub(crate) payload_text: String,
    pub(crate) payload_hash: String,
    pub(crate) before_hash: String,
    pub(crate) after_hash: String,
    pub(crate) request_payload: serde_json::Value,
    pub(crate) issued_at: DateTime<Utc>,
    pub(crate) expires_at: DateTime<Utc>,
    #[serde(default)]
    pub(crate) consumed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub(crate) expires_at: DateTime<Utc>,
    #[serde(default)]
    pub(crate) consumed: bool,
}

#[cfg(test)]
// 序列化夹具异常应使测试立即失败，断言式解包仅限本测试模块。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn legacy_challenge_without_actor_cid_number_is_rejected() {
        let now = Utc::now();
        let challenge = AdminActionChallenge {
            action_id: "action-1".to_string(),
            action_type: "INSTITUTION_CREATE_ACCOUNT".to_string(),
            actor_account_id: "0x1111111111111111111111111111111111111111111111111111111111111111"
                .to_string(),
            actor_institution_code: "FRG".to_string(),
            actor_cid_number: "LN001-FRG0G-000000001-2026".to_string(),
            actor_province_name: String::new(),
            actor_city_name: None,
            auth_type: AdminOperationAuth::PasskeyColdSign,
            target: "target".to_string(),
            payload_text: "{}".to_string(),
            payload_hash: "0xaa".to_string(),
            before_hash: "0xbb".to_string(),
            after_hash: "0xcc".to_string(),
            request_payload: serde_json::json!({}),
            issued_at: now,
            expires_at: now,
            consumed: false,
        };
        let mut value = serde_json::to_value(challenge).expect("serialize challenge");
        value
            .as_object_mut()
            .expect("challenge JSON object")
            .remove("actor_cid_number");
        assert!(serde_json::from_value::<AdminActionChallenge>(value).is_err());
    }
}
