use super::*;

use crate::guard::national_body_composition::check_full_state;
use crate::guard::national_body_composition::check_imported_state;
use crate::guard::national_body_composition::check_transition;
use crate::guard::national_body_composition::check_vote_state_keys;
use crate::guard::national_body_composition::composition_state;
use crate::guard::national_body_composition::decode_exact;
use crate::guard::national_body_composition::DecodedAdminAccount;
use crate::guard::national_body_composition::DecodedAssignment;
use crate::guard::national_body_composition::DecodedRole;
use adminbase::Admin;
use entitybase::InstitutionAssignmentSource;
use primitives::institution_constraints::member_composition_specs;
use primitives::institution_constraints::MemberCompositionSpec;
use std::collections::BTreeMap;
use votebase::types::ProposalSubjectCidNumbers;
use votebase::types::PROPOSAL_KIND_INTERNAL;
use votebase::types::STAGE_INTERNAL;
use votebase::Proposal;

fn account(index: u32) -> [u8; 32] {
    let mut account = [0u8; 32];
    account[..4].copy_from_slice(&index.to_le_bytes());
    account
}

fn formed_state(spec: &MemberCompositionSpec, count: u32) -> BTreeMap<Vec<u8>, Vec<u8>> {
    let admin_accounts = (0..count).map(account).collect::<Vec<_>>();
    let role = DecodedRole {
        cid_number: spec.institution.cid_number.as_bytes().to_vec(),
        role_code: spec.role_code.to_vec(),
        role_name: spec.role_name.to_vec(),
        term_required: false,
        role_status: InstitutionRoleStatus::Active,
    };
    let assignments = admin_accounts
        .iter()
        .copied()
        .map(|account_id| DecodedAssignment {
            cid_number: spec.institution.cid_number.as_bytes().to_vec(),
            account_id,
            role_code: spec.role_code.to_vec(),
            term_start: 0,
            term_end: 0,
            assignment_source: InstitutionAssignmentSource::PopularElection,
            assignment_source_ref: b"election".to_vec(),
            assignment_status: InstitutionAssignmentStatus::Active,
        })
        .collect::<Vec<_>>();
    let account = DecodedAdminAccount {
        institution_code: spec.institution.code,
        admins: admin_accounts
            .into_iter()
            .map(|account_id| Admin {
                account_id,
                // Phase 1: 公民 CID 与创世一致留空。
                cid_number: Default::default(),
                family_name: "管理".as_bytes().to_vec().try_into().expect("name fits"),
                given_name: "员".as_bytes().to_vec().try_into().expect("name fits"),
            })
            .collect(),
    };
    let [admin_key, role_key, assignments_key] =
        crate::guard::national_body_composition::storage_key::composition_keys(spec);
    BTreeMap::from([
        (admin_key, account.encode()),
        (role_key, role.encode()),
        (assignments_key, assignments.encode()),
    ])
}

#[test]
fn unconstituted_and_valid_constituted_states_pass() {
    assert_eq!(check_full_state(|_| None), Ok(()));
    let mut state = BTreeMap::new();
    for spec in member_composition_specs() {
        state.extend(formed_state(&spec, spec.min_members));
    }
    assert_eq!(check_full_state(|key| state.get(key).cloned()), Ok(()));
}

#[test]
fn member_names_do_not_define_membership_but_must_be_valid() {
    let spec = member_composition_specs()[0];
    let admin_key =
        crate::guard::national_body_composition::storage_key::composition_keys(&spec)[0].clone();
    let mut state = formed_state(&spec, spec.min_members);
    let mut account: DecodedAdminAccount =
        decode_exact(state.get(&admin_key).expect("admin account exists"))
            .expect("admin account decodes");
    account.admins[0].family_name = "张".as_bytes().to_vec().try_into().expect("name fits");
    account.admins[0].given_name = "三".as_bytes().to_vec().try_into().expect("name fits");
    state.insert(admin_key.clone(), account.encode());
    assert_eq!(check_full_state(|key| state.get(key).cloned()), Ok(()));

    let mut account: DecodedAdminAccount =
        decode_exact(state.get(&admin_key).expect("admin account exists"))
            .expect("admin account decodes");
    account.admins[0].given_name = Vec::new().try_into().expect("empty name fits");
    state.insert(admin_key, account.encode());
    assert_eq!(
        check_full_state(|key| state.get(key).cloned()),
        Err(
            crate::guard::national_body_composition::GuardError::InvalidAdminPersonName(
                spec.institution.code
            )
        )
    );
}

#[test]
fn partial_or_out_of_range_composition_is_rejected() {
    let spec = member_composition_specs()[0];
    let mut partial = formed_state(&spec, spec.min_members);
    partial
        .remove(&crate::guard::national_body_composition::storage_key::composition_keys(&spec)[1]);
    assert_eq!(
        composition_state(&spec, &|key| partial.get(key).cloned()),
        Err(
            crate::guard::national_body_composition::GuardError::PartialUnconstitutedState(
                spec.institution.code
            )
        )
    );

    let below = formed_state(&spec, spec.min_members - 1);
    assert_eq!(
        composition_state(&spec, &|key| below.get(key).cloned()),
        Err(
            crate::guard::national_body_composition::GuardError::MemberCountOutOfRange {
                code: spec.institution.code,
                min: spec.min_members,
                max: spec.max_members,
                found: spec.min_members - 1,
            }
        )
    );
}

#[test]
fn constituted_body_cannot_return_to_unconstituted() {
    let spec = member_composition_specs()[0];
    let parent = formed_state(&spec, spec.min_members);
    let delta = crate::guard::national_body_composition::storage_key::composition_keys(&spec)
        .into_iter()
        .map(|key| (key, None))
        .collect();
    assert_eq!(
        check_transition(&delta, |key| parent.get(key).cloned(), |_| None, None),
        Err(
            crate::guard::national_body_composition::GuardError::ConstitutedBodyRemoved(
                spec.institution.code
            )
        )
    );
}

#[test]
fn fixed_governance_threshold_snapshot_is_enforced() {
    let fixed = primitives::governance_skeleton::fixed_institutions()[0];
    let subject_cid_numbers: ProposalSubjectCidNumbers = vec![fixed
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("CID fits")]
    .try_into()
    .expect("single subject fits");
    let proposal: Proposal<u32, [u8; 32]> = Proposal {
        kind: PROPOSAL_KIND_INTERNAL,
        stage: STAGE_INTERNAL,
        status: 0,
        internal_code: Some(fixed.code),
        actor_cid_number: Some(
            fixed
                .cid_number
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("CID fits"),
        ),
        execution_account_id: None,
        subject_cid_numbers,
        start: 1u32,
        end: 2u32,
    };
    let id = 7;
    let expected = primitives::cid::code::fixed_governance_pass_threshold(&fixed.code)
        .expect("fixed threshold");
    let threshold_key = crate::guard::national_body_composition::storage_key::threshold(id);
    let state = BTreeMap::from([
        (
            crate::guard::national_body_composition::storage_key::proposal(id),
            proposal.encode(),
        ),
        (threshold_key.clone(), (expected - 1).encode()),
    ]);
    let delta = BTreeMap::from([(threshold_key, Some((expected - 1).encode()))]);
    assert_eq!(
        check_transition(&delta, |_| None, |key| state.get(key).cloned(), None),
        Err(
            crate::guard::national_body_composition::GuardError::FixedThresholdChanged {
                code: fixed.code,
                expected,
                found: expected - 1,
            }
        )
    );
    assert_eq!(
        check_imported_state(&state),
        Err(
            crate::guard::national_body_composition::GuardError::FixedThresholdChanged {
                code: fixed.code,
                expected,
                found: expected - 1,
            }
        )
    );

    let proposal_only = BTreeMap::from([(
        crate::guard::national_body_composition::storage_key::proposal(id),
        proposal.encode(),
    )]);
    let removed = BTreeMap::from([(
        crate::guard::national_body_composition::storage_key::threshold(id),
        None,
    )]);
    assert_eq!(
        check_transition(
            &removed,
            |_| None,
            |key| proposal_only.get(key).cloned(),
            None,
        ),
        Err(crate::guard::national_body_composition::GuardError::FixedThresholdMissing(fixed.code))
    );
    assert_eq!(check_transition(&removed, |_| None, |_| None, None), Ok(()));
}

#[test]
fn runtime_upgrade_rechecks_existing_fixed_threshold_snapshots() {
    let fixed = primitives::governance_skeleton::fixed_institutions()[0];
    let subject_cid_numbers: ProposalSubjectCidNumbers = vec![fixed
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("CID fits")]
    .try_into()
    .expect("single subject fits");
    let proposal: Proposal<u32, [u8; 32]> = Proposal {
        kind: PROPOSAL_KIND_INTERNAL,
        stage: STAGE_INTERNAL,
        status: 0,
        internal_code: Some(fixed.code),
        actor_cid_number: Some(
            fixed
                .cid_number
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("CID fits"),
        ),
        execution_account_id: None,
        subject_cid_numbers,
        start: 1u32,
        end: 2u32,
    };
    let id = 8;
    let expected = primitives::cid::code::fixed_governance_pass_threshold(&fixed.code)
        .expect("fixed threshold");
    let threshold_key = crate::guard::national_body_composition::storage_key::threshold(id);
    let state = BTreeMap::from([
        (
            crate::guard::national_body_composition::storage_key::proposal(id),
            proposal.encode(),
        ),
        (threshold_key.clone(), (expected - 1).encode()),
    ]);
    let delta = BTreeMap::from([(sp_storage::well_known_keys::CODE.to_vec(), Some(vec![1]))]);
    assert_eq!(
        check_transition(
            &delta,
            |key| state.get(key).cloned(),
            |key| state.get(key).cloned(),
            Some(std::slice::from_ref(&threshold_key)),
        ),
        Err(
            crate::guard::national_body_composition::GuardError::FixedThresholdChanged {
                code: fixed.code,
                expected,
                found: expected - 1,
            }
        )
    );
    assert_eq!(
        check_transition(
            &delta,
            |key| state.get(key).cloned(),
            |key| state.get(key).cloned(),
            None,
        ),
        Err(crate::guard::national_body_composition::GuardError::RuntimeUpgradeVoteKeysMissing)
    );
}

#[test]
fn non_member_singletons_do_not_enter_composition_guard() {
    for code in [
        primitives::cid::code::NLG,
        primitives::cid::code::NSP,
        primitives::cid::code::PRS,
    ] {
        let singleton = primitives::institution_constraints::singleton_institutions()
            .into_iter()
            .find(|item| item.code == code)
            .expect("singleton exists");
        assert!(
            !crate::guard::national_body_composition::storage_key::is_relevant(
                &crate::guard::governance_skeleton::storage_key::account_id(
                    singleton.cid_number.as_bytes(),
                )
            )
        );
        assert!(
            !crate::guard::national_body_composition::storage_key::is_relevant(
                &crate::guard::governance_skeleton::storage_key::institution_role(
                    singleton.cid_number.as_bytes(),
                    b"RUNTIME_ROLE",
                )
            )
        );
    }
}

#[test]
fn six_singleton_threshold_snapshots_are_not_frozen_by_node_guard() {
    for (index, singleton) in primitives::institution_constraints::singleton_institutions()
        .into_iter()
        .enumerate()
    {
        let subject_cid_numbers: ProposalSubjectCidNumbers = vec![singleton
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("CID fits")]
        .try_into()
        .expect("single subject fits");
        let proposal: Proposal<u32, [u8; 32]> = Proposal {
            kind: PROPOSAL_KIND_INTERNAL,
            stage: STAGE_INTERNAL,
            status: 0,
            internal_code: Some(singleton.code),
            actor_cid_number: Some(
                singleton
                    .cid_number
                    .as_bytes()
                    .to_vec()
                    .try_into()
                    .expect("CID fits"),
            ),
            execution_account_id: None,
            subject_cid_numbers,
            start: 1u32,
            end: 2u32,
        };
        let id = index as u64 + 20;
        let threshold_key = crate::guard::national_body_composition::storage_key::threshold(id);
        let state = BTreeMap::from([
            (
                crate::guard::national_body_composition::storage_key::proposal(id),
                proposal.encode(),
            ),
            (threshold_key.clone(), 1u32.encode()),
        ]);
        assert_eq!(
            check_vote_state_keys([threshold_key], |key| state.get(key).cloned()),
            Ok(()),
            "六个国家级单例不属于固定阈值策略",
        );
    }
}

#[test]
fn fixed_proposal_without_canonical_threshold_is_rejected() {
    let fixed = primitives::governance_skeleton::fixed_institutions()[0];
    let subject_cid_numbers: ProposalSubjectCidNumbers = vec![fixed
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("CID fits")]
    .try_into()
    .expect("single subject fits");
    let id = 40;
    let proposal_key = crate::guard::national_body_composition::storage_key::proposal(id);
    let proposal: Proposal<u32, [u8; 32]> = Proposal {
        kind: PROPOSAL_KIND_INTERNAL,
        stage: STAGE_INTERNAL,
        status: 0,
        internal_code: Some(fixed.code),
        actor_cid_number: Some(
            fixed
                .cid_number
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("CID fits"),
        ),
        execution_account_id: None,
        subject_cid_numbers,
        start: 1u32,
        end: 2u32,
    };
    let state = BTreeMap::from([(proposal_key.clone(), proposal.encode())]);
    let delta = BTreeMap::from([(proposal_key, state.values().next().cloned())]);
    assert_eq!(
        check_transition(&delta, |_| None, |key| state.get(key).cloned(), None),
        Err(crate::guard::national_body_composition::GuardError::FixedThresholdMissing(fixed.code))
    );
}
