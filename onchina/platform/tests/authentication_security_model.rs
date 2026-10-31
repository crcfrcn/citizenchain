//! 原所属模块回归，断言与输入保持。
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
        .remove("actorCidNumber");
    assert!(serde_json::from_value::<AdminActionChallenge>(value).is_err());
}
