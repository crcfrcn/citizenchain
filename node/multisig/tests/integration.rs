use crate::proposal::decode_proposal_data_action;

#[test]
fn multisig_payload_to_public_details_keeps_flattened_ownership() {
    let mut data = b"multisig".to_vec();
    data.push(0);
    data.extend([17u8; 32]);
    data.extend([34u8; 32]);
    data.extend(111u128.to_le_bytes());
    data.push(0);
    data.extend([51u8; 32]);
    let details = decode_proposal_data_action(7, &data)
        .unwrap()
        .into_details();
    let json = serde_json::to_value(details).unwrap();
    assert_eq!(json["transferDetail"]["proposalId"], 7);
    assert_eq!(json["transferDetail"]["amountFen"], "111");
    assert!(json["safetyFundDetail"].is_null());
    assert!(json["sweepDetail"].is_null());
    assert_eq!(
        json["transferDetail"]["beneficiaryAccountId"],
        format!("0x{}", "22".repeat(32))
    );
    assert!(decode_proposal_data_action(7, b"res-iss").is_none());
}
