use crate::decode::decode_resolution_issuance_action;
use codec::Encode;

pub(super) fn payload(reason: &str, amount: u128) -> Vec<u8> {
    let mut bytes = b"res-iss".to_vec();
    bytes.extend(b"44".to_vec().encode());
    bytes.extend(b"NRC-MEMBER".to_vec().encode());
    bytes.extend([17u8; 32]);
    bytes.extend(reason.as_bytes().to_vec().encode());
    bytes.extend(amount.encode());
    bytes.extend(codec::Compact(1u32).encode());
    bytes.extend([34u8; 32]);
    bytes.extend(amount.encode());
    bytes
}

#[test]
fn decode_rejects_every_truncation_wrong_tag_and_trailing_bytes() {
    let bytes = payload("公开测试原因", 111);
    for n in 0..bytes.len() {
        assert!(
            decode_resolution_issuance_action(7, &bytes[..n]).is_none(),
            "truncation {n}"
        );
    }
    let mut wrong = bytes.clone();
    wrong[0] = 0;
    assert!(decode_resolution_issuance_action(7, &wrong).is_none());
    let mut extra = bytes;
    extra.push(0);
    assert!(decode_resolution_issuance_action(7, &extra).is_none());
}

#[test]
fn issuance_json_preserves_maximum_amount_and_owner_fields() {
    let detail = decode_resolution_issuance_action(7, &payload("公开原因", u128::MAX)).unwrap();
    let json = serde_json::to_value(detail).unwrap();
    assert_eq!(json["proposalId"], 7);
    assert_eq!(json["actorCidNumber"], "44");
    assert_eq!(json["proposerRoleCode"], "NRC-MEMBER");
    assert_eq!(json["totalAmountFen"], u128::MAX.to_string());
    assert_eq!(json["allocations"][0]["amountFen"], u128::MAX.to_string());
    assert_eq!(
        json["allocations"][0]["recipientAccountId"],
        format!("0x{}", "22".repeat(32))
    );
    assert!(json.get("amount").is_none());
    assert_eq!(json.as_object().unwrap().len(), 7);
}
