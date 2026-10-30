use crate::{decode::decode_resolution_issuance_action, summary::format_issuance_summary};

#[test]
fn issuance_decode_summary_and_json_keep_the_same_amount_and_reason() {
    let reason = "中".repeat(31);
    let detail = decode_resolution_issuance_action(9, &super::unit::payload(&reason, 111)).unwrap();
    assert_eq!(
        format_issuance_summary(&detail),
        format!("决议发行 1.11 元（1条分配）：{}…", "中".repeat(30))
    );
    assert_eq!(serde_json::to_value(&detail).unwrap()["reason"], reason);
    assert_eq!(detail.allocations.len(), 1);
}
