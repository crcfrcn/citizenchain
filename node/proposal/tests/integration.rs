//! 验证多签提案的摘要及详情变体保持一致。

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

// 读取同一最终块的原始SCALE夹具，不连接运行链、不回写旧投票记录。
fn frozen_fixture(
    kind: u8,
    versions: bool,
) -> (
    crate::types::FrozenVoteView,
    crate::types::FrozenVoteBinding,
) {
    use codec::{DecodeAll, Encode};
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../runtime/primitives/tests/fixtures/role_permission.json"
    ))
    .unwrap();
    let frozen = &fixture["frozen_institution_voters"];
    let cid = frozen["cid_number"].as_str().unwrap();
    let role = frozen["role_code"].as_str().unwrap();
    let plan_hex = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "vote_plan_resolution_issuance_joint")
        .unwrap()["encoded_hex"]
        .as_str()
        .unwrap();
    let mut plan = engingcore::types::VotePlanOf::<sp_runtime::AccountId32>::decode_all(
        &mut &hex::decode(plan_hex).unwrap()[..],
    )
    .unwrap();
    let subject = plan.voter_subjects[0].clone();
    plan.voter_subjects = vec![subject.clone()].try_into().unwrap();
    let proposal = engingcore::types::Proposal::<u32, sp_runtime::AccountId32> {
        kind,
        stage: kind,
        status: 0,
        internal_code: None,
        actor_cid_number: Some(cid.as_bytes().to_vec().try_into().unwrap()),
        execution_account_id: None,
        subject_cid_numbers: vec![cid.as_bytes().to_vec().try_into().unwrap()]
            .try_into()
            .unwrap(),
        start: 1,
        end: 100,
    };
    let mut map = std::collections::HashMap::new();
    map.insert(
        nodeclient::query::map_key("VotingEngine", "Proposals", &7u128.to_le_bytes()),
        format!("0x{}", hex::encode(proposal.encode())),
    );
    map.insert(
        nodeclient::query::map_key("VotingEngine", "ProposalVotePlans", &7u128.to_le_bytes()),
        format!("0x{}", hex::encode(plan.encode())),
    );
    map.insert(
        nodeclient::query::double_map_key(
            "VotingEngine",
            "VoterSnapshot",
            &7u128.to_le_bytes(),
            &subject.encode(),
        ),
        frozen["voters_hex"].as_str().unwrap().into(),
    );
    if versions {
        map.insert(
            nodeclient::query::double_map_key(
                "VotingEngine",
                "FrozenWalletVersions",
                &7u128.to_le_bytes(),
                &subject.encode(),
            ),
            frozen["versions_hex"].as_str().unwrap().into(),
        );
    }
    let wallet = [0x11; 32];
    let key = crate::tally::institution_vote_key(7, kind, cid, role, &wallet).unwrap();
    map.insert(key, "0x00".into());
    // 现任目录读取会使测试失败：退休钱包仍留在原名单中。
    let view = crate::query::read_frozen_vote_view(7, "0xfinalized", 5, "0xgenesis", |key| {
        assert!(!key.starts_with(&nodeclient::query::value_key(
            "PublicManage",
            "AdminAccounts"
        )));
        Ok(map.get(key).cloned())
    })
    .unwrap();
    let binding = crate::types::FrozenVoteBinding {
        genesis_hash: "0xgenesis".into(),
        proposal_id: "7".into(),
        cid_number: cid.into(),
        role_code: role.into(),
        account_id: format!("0x{}", hex::encode(wallet)),
        wallet_revision: "0".into(),
    };
    (view, binding)
}

#[test]
fn frozen_original_wallet_and_false_vote_survive_current_roster_changes() {
    for kind in [0, 1] {
        let (view, binding) = frozen_fixture(kind, true);
        assert_eq!(view.block_hash, "0xfinalized");
        assert_eq!(view.roles[0].voters.len(), 2);
        assert_eq!(
            crate::vote::frozen_ticket(&view, &binding).unwrap().vote,
            Some(false)
        );
        assert_eq!(
            view.roles[0].voters[1].wallet_revision.as_deref(),
            Some("6")
        );
        assert_eq!(view.roles[0].voters[1].vote, None);
    }
}
#[test]
fn missing_frozen_versions_preserve_recorded_fact_but_never_default_to_zero() {
    let (view, binding) = frozen_fixture(0, false);
    assert_eq!(view.roles[0].voters[0].vote, Some(false));
    assert!(view.roles[0].voters[0].wallet_revision.is_none());
    assert!(crate::vote::frozen_ticket(&view, &binding).is_err());
}
#[test]
fn frozen_identity_rejects_new_wallet_revision_other_role_cid_and_chain() {
    let (view, binding) = frozen_fixture(0, true);
    for field in ["wallet", "revision", "role", "cid", "chain"] {
        let mut bad = binding.clone();
        match field {
            "wallet" => bad.account_id = format!("0x{}", hex::encode([0x33; 32])),
            "revision" => bad.wallet_revision = "9".into(),
            "role" => bad.role_code = "OTHER".into(),
            "cid" => bad.cid_number = "OTHER".into(),
            _ => bad.genesis_hash = "OTHER".into(),
        };
        assert!(crate::vote::frozen_ticket(&view, &bad).is_err(), "{field}");
    }
}
#[test]
fn node_institution_ticket_keys_keep_original_encoding() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../runtime/primitives/tests/fixtures/role_permission.json"
    ))
    .unwrap();
    let f = &f["frozen_institution_voters"];
    let ticket = hex::decode(
        f["original_ticket_hex"]
            .as_str()
            .unwrap()
            .trim_start_matches("0x"),
    )
    .unwrap();
    for kind in [0, 1] {
        let mut raw = if kind == 0 { vec![1] } else { vec![] };
        raw.extend(&ticket);
        assert_eq!(
            crate::tally::institution_vote_key(
                7,
                kind,
                f["cid_number"].as_str().unwrap(),
                f["role_code"].as_str().unwrap(),
                &[0x11; 32]
            )
            .unwrap(),
            nodeclient::query::double_map_key(
                if kind == 0 {
                    "InternalVote"
                } else {
                    "JointVote"
                },
                if kind == 0 {
                    "InternalVotesByTicket"
                } else {
                    "JointVotesByTicket"
                },
                &7u128.to_le_bytes(),
                &raw
            )
        );
    }
}
#[test]
fn malformed_votes_are_errors_not_unvoted_or_opposition() {
    assert_eq!(crate::tally::decode_option_vote(None).unwrap(), None);
    assert_eq!(
        crate::tally::decode_option_vote(Some("0x00")).unwrap(),
        Some(false)
    );
    assert_eq!(
        crate::tally::decode_option_vote(Some("0x01")).unwrap(),
        Some(true)
    );
    for raw in ["0x", "0x02", "0x0001", "nothex"] {
        assert!(crate::tally::decode_option_vote(Some(raw)).is_err());
    }
}
#[test]
fn new_signature_requires_original_stage_deadline_and_status() {
    let (mut view, _) = frozen_fixture(0, true);
    assert!(crate::vote::ensure_open(&view).is_ok());
    view.status = 3;
    assert!(crate::vote::ensure_open(&view).is_err());
    view.status = 0;
    view.block_number = "101".into();
    assert!(crate::vote::ensure_open(&view).is_err());
    view.block_number = "100".into();
    assert!(crate::vote::ensure_open(&view).is_ok());
    view.stage = 1;
    assert!(crate::vote::ensure_open(&view).is_err());
}
#[test]
fn vote_entry_accepts_only_exact_original_vote_call() {
    use codec::Encode;
    let mut call = vec![20, 0];
    call.extend(7u128.to_le_bytes());
    call.push(1);
    call.extend(b"COMMITTEE_MEMBER".to_vec().encode());
    call.push(0);
    let decoded = crate::vote::decode_vote_call(&call).unwrap();
    assert_eq!(decoded.0, 7);
    assert!(!decoded.3);
    call.push(0);
    assert!(crate::vote::decode_vote_call(&call).is_err());
    call[0] = 30;
    assert!(crate::vote::decode_vote_call(&call).is_err());
    assert!(crate::vote::decode_vote_call(&[]).is_err());
}
