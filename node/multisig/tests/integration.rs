use crate::proposal::decode_proposal_data_action;
use crate::signing::{build_propose_transfer_sign_request, build_safety_fund_call_data};

#[test]
fn institution_transfer_entry_points_validate_ethereum_before_accessing_rpc() {
    let signer = format!("0x{}", "11".repeat(32));
    let source = format!("0x{}", "22".repeat(32));
    let cid = "CHN-ORG-001";
    let role = "COMMITTEE_MEMBER";
    for (address, expected) in [
        ("0x1234", "以太坊地址必须为 0x 加 40 位十六进制字符"),
        (
            "0x5AAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
            "以太坊地址大小写校验和无效",
        ),
    ] {
        let transfer_error =
            build_propose_transfer_sign_request(&signer, cid, role, &source, address, 2.0, "")
                .err()
                .expect("普通机构转账应拒绝无效以太坊地址");
        assert_eq!(transfer_error, expected);
        let safety_error = build_safety_fund_call_data(cid, role, &source, address, 2.0, "")
            .expect_err("安全基金签名和提交应拒绝无效以太坊地址");
        assert_eq!(safety_error, expected);
    }
}

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
