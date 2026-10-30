use crate::{decode::decode_resolution_destroy_action, summary::format_destroy_summary};

#[test]
fn destroy_decode_summary_and_json_use_the_same_institution_amount() {
    let detail = decode_resolution_destroy_action(9, &super::unit::payload(111)).unwrap();
    assert_eq!(
        format_destroy_summary(&detail),
        "决议销毁 1.11 元：test-cid"
    );
    let json = serde_json::to_value(&detail).unwrap();
    assert_eq!(json["amountFen"], "111");
    assert_eq!(json["institutionAccountId"], detail.institution_account_id);
}

#[test]
fn scale_cid_length_boundaries_keep_account_amount_and_display_aligned() {
    use codec::Encode;
    // 直接使用SCALE生产者覆盖一字节/两字节长度前缀，不另写编码规则。
    for length in [63, 64] {
        let cid = "A".repeat(length);
        let mut payload = b"res-dst".to_vec();
        payload.extend(cid.as_bytes().to_vec().encode());
        payload.extend([17u8; 32].encode());
        payload.extend(111u128.encode());
        let detail = decode_resolution_destroy_action(9, &payload).unwrap();
        assert_eq!(detail.actor_cid_number, cid);
        assert_eq!(
            detail.institution_account_id,
            format!("0x{}", "11".repeat(32))
        );
        assert_eq!(
            format_destroy_summary(&detail),
            format!("决议销毁 1.11 元：{cid}")
        );
        let json = serde_json::to_value(detail).unwrap();
        assert_eq!(json["amountFen"], "111");
        assert_eq!(json["actorCidNumber"], cid);
        assert_eq!(
            json["institutionAccountId"],
            format!("0x{}", "11".repeat(32))
        );
    }
}
