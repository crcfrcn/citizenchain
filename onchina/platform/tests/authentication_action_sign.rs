//! 原所属模块回归，断言与输入保持。
use super::*;

#[test]
fn canonical_admin_payload_includes_actor_cid_number_in_fixed_order() {
    let text = signed_payload_text(AdminSignedPayload {
        domain: "onchina_admin_governance",
        qr_proto: "QR_V1",
        action_id: "action-1",
        action_type: "INSTITUTION_CREATE_ACCOUNT",
        account_id: "0x1111111111111111111111111111111111111111111111111111111111111111",
        actor_cid_number: "LN001-FRG0G-000000001-2026",
        actor_province_name: "北京市",
        target: "target",
        request_hash: "0xaa",
        before_hash: "0xbb",
        after_hash: "0xcc",
        expires_at: 123,
    });
    assert_eq!(
            text,
            "{\"domain\":\"onchina_admin_governance\",\"qr_proto\":\"QR_V1\",\"action_id\":\"action-1\",\"action_type\":\"INSTITUTION_CREATE_ACCOUNT\",\"account_id\":\"0x1111111111111111111111111111111111111111111111111111111111111111\",\"actor_cid_number\":\"LN001-FRG0G-000000001-2026\",\"actor_province_name\":\"北京市\",\"target\":\"target\",\"request_hash\":\"0xaa\",\"before_hash\":\"0xbb\",\"after_hash\":\"0xcc\",\"expires_at\":123}"
        );
}
