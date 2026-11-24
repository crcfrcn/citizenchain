//! 敏感动作必须保留完整人员绑定、机构和所选岗位；旧形状不能恢复。
use super::*;
#[test]
fn challenge_requires_each_identity_and_role_field() {
    let now = Utc::now();
    let challenge = AdminActionChallenge {
        action_id: "action-1".into(),
        action_type: "INSTITUTION_CREATE_ACCOUNT".into(),
        actor_account_id: format!("0x{}", "11".repeat(32)),
        person_cid: crate::testing::person_cid(),
        role_code: "R1".into(),
        binding_revision: "1".into(),
        actor_institution_code: "FRG".into(),
        actor_cid_number: "institution".into(),
        actor_province_name: "广东省".into(),
        actor_city_name: None,
        auth_type: AdminOperationAuth::PasskeyColdSign,
        target: "target".into(),
        payload_text: "{}".into(),
        payload_hash: "0xaa".into(),
        before_hash: "0xbb".into(),
        after_hash: "0xcc".into(),
        request_payload: serde_json::json!({}),
        issued_at: now,
        expires_at: now,
        consumed: false,
    };
    let value = serde_json::to_value(challenge).unwrap();
    assert!(serde_json::from_value::<AdminActionChallenge>(value.clone()).is_ok());
    for field in ["personCid", "roleCode", "bindingRevision", "actorCidNumber"] {
        let mut invalid = value.clone();
        invalid.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<AdminActionChallenge>(invalid).is_err(),
            "{field}"
        );
    }
}
