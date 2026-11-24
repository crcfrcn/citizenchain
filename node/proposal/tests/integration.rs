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

// 合成finalized状态只经注入读取器验证，不连接运行链。
fn person(index: u8) -> String {
    primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: &format!("proposal-person-{index}"),
            p1: "1",
            province_code: "GD",
            province_name: "广东省",
            city_code: "001",
            city_name: "荔湾市",
            year: 2026,
            institution: "CTZN",
        },
    )
    .unwrap()
}
fn fixture_map(
    kind: u8,
    bound: bool,
    historical: bool,
) -> (
    std::collections::BTreeMap<String, String>,
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
        &mut hex::decode(plan_hex).unwrap().as_slice(),
    )
    .unwrap();
    let subject = plan.voter_subjects[0].clone();
    plan.voter_subjects = vec![subject.clone()].try_into().unwrap();
    let proposal = engingcore::types::Proposal::<u32, sp_runtime::AccountId32> {
        kind,
        stage: kind,
        status: if historical { 3 } else { 0 },
        internal_code: None,
        actor_cid_number: Some(cid.as_bytes().to_vec().try_into().unwrap()),
        execution_account_id: None,
        subject_cid_numbers: vec![cid.as_bytes().to_vec().try_into().unwrap()]
            .try_into()
            .unwrap(),
        start: 1,
        end: 100,
    };
    let mut map = std::collections::BTreeMap::new();
    let hex = |bytes: Vec<u8>| format!("0x{}", hex::encode(bytes));
    map.insert(
        nodeclient::query::map_key("VotingEngine", "Proposals", &7u128.encode()),
        hex(proposal.encode()),
    );
    map.insert(
        nodeclient::query::map_key("VotingEngine", "ProposalVotePlans", &7u128.encode()),
        hex(plan.encode()),
    );
    let first = person(1);
    let mut persons = vec![first.clone().into_bytes(), person(2).into_bytes()];
    persons.sort();
    let entitybase::AuthorizationSubject::Institution(role_subject) = subject.clone() else {
        panic!("institution fixture")
    };
    if historical {
        map.insert(
            nodeclient::query::double_map_key(
                "VotingEngine",
                "VoterSnapshot",
                &7u128.encode(),
                &subject.encode(),
            ),
            frozen["voters_hex"].as_str().unwrap().into(),
        );
        let mut ticket = if kind == 0 { vec![1] } else { vec![] };
        ticket.extend((role_subject, [0x11u8; 32]).encode());
        map.insert(
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
                &7u128.encode(),
                &ticket,
            ),
            "0x00".into(),
        );
    } else {
        map.insert(
            nodeclient::query::double_map_key(
                "VotingEngine",
                "RoleVoters",
                &7u128.encode(),
                &role_subject.encode(),
            ),
            hex(persons.encode()),
        );
        for (index, p) in [first.clone(), person(2)].iter().enumerate() {
            let record = identity::CidRecord::<u32> {
                registrar_cid_number: b"registry".to_vec().try_into().unwrap(),
                commitment: [1; 32],
                residence_province_code: b"GD".to_vec().try_into().unwrap(),
                residence_city_code: b"001".to_vec().try_into().unwrap(),
                status: identity::CidRecordStatus::Active,
                registered_at: 1,
                revoked_at: None,
            };
            let wallet = if index == 0 {
                [0x11u8; 32]
            } else {
                [0x22u8; 32]
            };
            map.insert(
                nodeclient::query::map_key(
                    "Identity",
                    "CidRegistry",
                    &p.as_bytes().to_vec().encode(),
                ),
                hex(record.encode()),
            );
            if bound || index != 0 {
                for (item, key, value) in [
                    (
                        "AccountIdByCid",
                        p.as_bytes().to_vec().encode(),
                        wallet.encode(),
                    ),
                    (
                        "BindingRevisionByCid",
                        p.as_bytes().to_vec().encode(),
                        7u64.encode(),
                    ),
                    (
                        "CidByAccountId",
                        wallet.encode(),
                        p.as_bytes().to_vec().encode(),
                    ),
                ] {
                    map.insert(
                        nodeclient::query::map_key("Identity", item, &key),
                        hex(value),
                    );
                }
            }
        }
        map.insert(
            crate::tally::institution_vote_key(7, kind, cid, role, first.as_bytes()).unwrap(),
            "0x00".into(),
        );
    }
    (
        map,
        crate::types::FrozenVoteBinding {
            genesis_hash: "0xgenesis".into(),
            proposal_id: "7".into(),
            cid_number: cid.into(),
            role_code: role.into(),
            account_id: format!("0x{}", "11".repeat(32)),
            person_cid: first,
            binding_revision: "7".into(),
        },
    )
}
fn frozen_fixture(
    kind: u8,
    bound: bool,
) -> (
    crate::types::FrozenVoteView,
    crate::types::FrozenVoteBinding,
) {
    let (map, binding) = fixture_map(kind, bound, false);
    let view = crate::query::read_frozen_vote_view(7, "0xfinalized", 5, "0xgenesis", |key| {
        Ok(map.get(key).cloned())
    })
    .unwrap();
    (view, binding)
}
#[test]
fn frozen_person_and_false_vote_survive_current_binding_changes() {
    for kind in [0, 1] {
        let (mut view, mut binding) = frozen_fixture(kind, true);
        assert_eq!(view.block_hash, "0xfinalized");
        assert_eq!(view.roles[0].voters.len(), 2);
        assert_eq!(
            crate::vote::frozen_ticket(&view, &binding).unwrap().vote,
            Some(false)
        );
        let a = view.roles[0]
            .voters
            .iter_mut()
            .find(|v| v.person_cid.as_deref() == Some(&binding.person_cid))
            .unwrap();
        a.account_id = Some(format!("0x{}", "33".repeat(32)));
        a.binding_revision = Some("8".into());
        assert!(crate::vote::current_binding(a, &binding).is_err());
        binding.account_id = a.account_id.clone().unwrap();
        binding.binding_revision = "8".into();
        assert!(crate::vote::current_binding(a, &binding).is_ok());
        assert_eq!(
            crate::vote::frozen_ticket(&view, &binding).unwrap().vote,
            Some(false)
        );
    }
}
#[test]
fn missing_current_binding_does_not_erase_frozen_person_or_fabricate_a_version() {
    let (view, binding) = frozen_fixture(0, false);
    let voter = crate::vote::frozen_ticket(&view, &binding).unwrap();
    assert_eq!(voter.vote, Some(false));
    assert!(voter.account_id.is_none());
    assert!(voter.binding_revision.is_none());
    assert!(crate::vote::current_binding(voter, &binding).is_err());
}
#[test]
fn frozen_identity_requires_original_person_role_cid_and_chain() {
    let (view, binding) = frozen_fixture(0, true);
    for dimension in ["proposal", "person", "role", "institution", "chain"] {
        let mut bad = binding.clone();
        match dimension {
            "proposal" => bad.proposal_id = "8".into(),
            "person" => bad.person_cid = person(3),
            "role" => bad.role_code = "OTHER".into(),
            "institution" => bad.cid_number = "OTHER".into(),
            _ => bad.genesis_hash = "OTHER".into(),
        };
        assert!(
            crate::vote::frozen_ticket(&view, &bad).is_err(),
            "{dimension}"
        );
    }
}
#[test]
fn historical_terminal_wallet_facts_are_read_without_inventing_person_or_version() {
    let (map, _) = fixture_map(0, true, true);
    let view = crate::query::read_frozen_vote_view(7, "0xfinalized", 5, "0xgenesis", |key| {
        Ok(map.get(key).cloned())
    })
    .unwrap();
    assert_eq!(view.roles[0].voters[0].vote, Some(false));
    assert!(view.roles[0]
        .voters
        .iter()
        .all(|v| v.person_cid.is_none() && v.binding_revision.is_none()));
    assert!(crate::vote::ensure_open(&view).is_err());
}
#[test]
fn current_ticket_key_uses_exact_shared_person_ticket_and_institution_variant_two() {
    use codec::Encode;
    let (_, binding) = fixture_map(0, true, false);
    let ticket = engingcore::types::InstitutionVoteTicket {
        role_subject: engingcore::types::RoleSubject {
            cid_number: binding.cid_number.as_bytes().to_vec().try_into().unwrap(),
            role_code: binding.role_code.as_bytes().to_vec().try_into().unwrap(),
        },
        person_cid: binding.person_cid.as_bytes().to_vec().try_into().unwrap(),
    };
    let mut bytes = vec![2];
    bytes.extend(ticket.encode());
    assert_eq!(
        crate::tally::institution_vote_key(
            7,
            0,
            &binding.cid_number,
            &binding.role_code,
            binding.person_cid.as_bytes()
        )
        .unwrap(),
        nodeclient::query::double_map_key(
            "InternalVote",
            "InternalVotesByTicket",
            &7u128.encode(),
            &bytes
        )
    );
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
