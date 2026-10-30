use crate::business::{format_summary, ProposalAction};

#[test]
fn proposal_adapter_keeps_multisig_summary_and_detail_variants_together() {
    let action =
        ProposalAction::TransferModule(Box::new(nodemultisig::types::ProposalAction::Transfer(
            Box::new(nodemultisig::types::TransferProposalDetail {
                proposal_id: 7,
                actor_cid_number: None,
                funding_account_id: String::new(),
                beneficiary_account_id: String::new(),
                amount_fen: "111".into(),
                remark: "公开备注".into(),
                proposer_account_id: String::new(),
            }),
        )));
    assert_eq!(
        format_summary(&action, |_| None),
        "个人多签转账 1.11 元：公开备注"
    );
    let details = action.into_details();
    assert_eq!(details.transfer_detail.unwrap().proposal_id, 7);
    assert!(details.sweep_detail.is_none());
    assert!(details.safety_fund_detail.is_none());
}
