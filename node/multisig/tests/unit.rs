use crate::proposal::decode_transfer_action;
use crate::proposal::format_sweep_summary;
use crate::proposal::format_transfer_summary;
use crate::proposal::TAG_TRANSFER;
use crate::types::SafetyFundProposalDetail;
use crate::types::SweepProposalDetail;
use crate::types::TransferProposalDetail;

#[test]
fn transfer_summary_truncates_long_remark() {
    let detail = TransferProposalDetail {
        proposal_id: 1,
        actor_cid_number: None,
        funding_account_id: String::new(),
        beneficiary_account_id: String::new(),
        amount_fen: "12345".to_string(),
        remark: "一二三四五六七八九十".repeat(4),
        proposer_account_id: String::new(),
    };
    let summary = format_transfer_summary(&detail, |_| None);
    assert!(summary.starts_with("个人多签转账 123.45 元："));
    assert!(summary.contains('…'));
}

#[test]
fn transfer_action_decodes_actor_cid_and_funding_account_id() {
    let actor_cid_number = "CHN-ORG-001";
    let remark = "机构账户转账";
    let mut data = TAG_TRANSFER.to_vec();
    data.push(1);
    data.push((actor_cid_number.len() as u8) << 2);
    data.extend_from_slice(actor_cid_number.as_bytes());
    data.extend_from_slice(&[0x11; 32]);
    data.extend_from_slice(&[0x22; 32]);
    data.extend_from_slice(&12345u128.to_le_bytes());
    data.push((remark.len() as u8) << 2);
    data.extend_from_slice(remark.as_bytes());
    data.extend_from_slice(&[0x33; 32]);

    let detail = decode_transfer_action(7, &data).expect("应按 runtime 新布局解码");
    assert_eq!(detail.proposal_id, 7);
    assert_eq!(detail.actor_cid_number.as_deref(), Some(actor_cid_number));
    assert_eq!(detail.funding_account_id, format!("0x{}", "11".repeat(32)));
    assert_eq!(
        detail.beneficiary_account_id,
        format!("0x{}", "22".repeat(32))
    );
    assert_eq!(detail.amount_fen, "12345");
    assert_eq!(detail.remark, remark);
    assert_eq!(detail.proposer_account_id, format!("0x{}", "33".repeat(32)));
}

#[test]
fn sweep_summary_uses_unknown_institution_fallback() {
    let detail = SweepProposalDetail {
        proposal_id: 2,
        actor_cid_number: "CHN-UNKNOWN".to_string(),
        institution_account_id: format!("0x{}", "00".repeat(32)),
        amount_fen: "999900".to_string(),
    };
    assert_eq!(
        format_sweep_summary(&detail, |_| None),
        "手续费划转 9,999.00 元：CHN-UNKNOWN"
    );
}

#[test]
fn action_into_details_maps_each_field() {
    let detail = SafetyFundProposalDetail {
        proposal_id: 3,
        actor_cid_number: "CHN-NRC".to_string(),
        institution_account_id: String::new(),
        beneficiary_account_id: String::new(),
        amount_fen: "100".to_string(),
        remark: String::new(),
    };
    let details = crate::types::ProposalAction::SafetyFund(Box::new(detail)).into_details();
    assert!(details.transfer_detail.is_none());
    assert!(details.safety_fund_detail.is_some());
    assert!(details.sweep_detail.is_none());
}
