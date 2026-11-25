//! 验证受保护机构组成、法定岗位及规范人员CID及法定席位约束。

use super::*;

use crate::guard::national_body_composition::check_full_state;
use crate::guard::national_body_composition::check_transition;
use crate::guard::national_body_composition::check_vote_state_keys;
use crate::guard::national_body_composition::composition_state;
use crate::guard::national_body_composition::decode_exact;
use crate::guard::national_body_composition::DecodedAssignment;
use crate::guard::national_body_composition::DecodedInstitution;
use crate::guard::national_body_composition::DecodedRole;
use engingcore::types::ProposalSubjectCidNumbers;
use engingcore::types::PROPOSAL_KIND_INTERNAL;
use engingcore::types::STAGE_INTERNAL;
use engingcore::Proposal;
use entitybase::InstitutionAssignmentSource;
use primitives::institution_constraints::member_composition_specs;
use primitives::institution_constraints::MemberCompositionSpec;
use std::collections::BTreeMap;

fn person(index: u32) -> Vec<u8> {
    primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: &format!("guard-person-{index}"),
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
    .into_bytes()
}

fn legacy_state(spec: &MemberCompositionSpec) -> BTreeMap<Vec<u8>, Vec<u8>> {
    let mut state = formed_state(spec, spec.min_members);
    let [_, role_key, rows_key, _] = storage_key::composition_keys(spec);
    let role: DecodedRole = decode_exact(&state[&role_key]).unwrap();
    state.insert(
        role_key,
        (
            &role.cid_number,
            &role.role_code,
            &role.role_name,
            role.term_required,
            role.role_status,
        )
            .encode(),
    );
    let rows: Vec<_> = (0..spec.min_members)
        .map(|index| {
            let mut wallet = [0u8; 32];
            wallet[..4].copy_from_slice(&index.to_le_bytes());
            (
                role.cid_number.clone(),
                wallet,
                role.role_code.clone(),
                0u32,
                0u32,
                InstitutionAssignmentSource::PopularElection,
                b"election".to_vec(),
                InstitutionAssignmentStatus::Active,
            )
        })
        .collect();
    state.insert(rows_key, rows.encode());
    state
}

#[test]
fn migration_checks_legacy_composition_and_exact_cid_conversion_without_wallet_authority() {
    let spec = member_composition_specs()[0];
    let parent = legacy_state(&spec);
    let [_, role_key, rows_key, _] = storage_key::composition_keys(&spec);
    let legacy = |key: &[u8]| key == role_key || key == rows_key;
    assert_eq!(
        check_full_state_with_layout(|k| parent.get(k).cloned(), legacy),
        Ok(())
    );
    let mut post = formed_state(&spec, 0);
    let delta = BTreeMap::from([
        (role_key.clone(), Some(post[&role_key].clone())),
        (rows_key.clone(), Some(post[&rows_key].clone())),
    ]);
    let check = |post: &BTreeMap<Vec<u8>, Vec<u8>>, migrating| {
        check_transition_with_layout(
            &delta,
            |k| parent.get(k).cloned(),
            |k| post.get(k).cloned(),
            legacy,
            |_| false,
            migrating,
            None,
        )
    };
    assert_eq!(check(&post, true), Ok(()), "原钱包均未绑定时只产生真实空缺");
    assert_eq!(
        check(&post, false),
        Err(GuardError::ConstitutedBodyRemoved(spec.institution.code))
    );
    post = formed_state(&spec, spec.min_members);
    assert_eq!(
        check(&post, true),
        Err(GuardError::MigrationTransitionInvalid(
            spec.institution.code
        )),
        "合法格式的虚构人员CID也不能替代真实绑定转换"
    );

    let mut duplicate = parent.clone();
    let mut rows: Vec<(
        Vec<u8>,
        [u8; 32],
        Vec<u8>,
        u32,
        u32,
        InstitutionAssignmentSource,
        Vec<u8>,
        InstitutionAssignmentStatus,
    )> = decode_exact(&parent[&rows_key]).unwrap();
    rows[1].1 = rows[0].1;
    duplicate.insert(rows_key.clone(), rows.encode());
    assert_eq!(
        check_full_state_with_layout(|k| duplicate.get(k).cloned(), legacy),
        Err(GuardError::DuplicateMember(spec.institution.code))
    );
}
fn formed_state(spec: &MemberCompositionSpec, count: u32) -> BTreeMap<Vec<u8>, Vec<u8>> {
    let institution = DecodedInstitution {
        cid_full_name: b"institution".to_vec(),
        cid_short_name: b"inst".to_vec(),
        town_code: vec![],
        legal_representative: None,
        institution_code: spec.institution.code,
        created_at: 0,
    };
    let role = DecodedRole {
        cid_number: spec.institution.cid_number.as_bytes().to_vec(),
        role_code: spec.role_code.to_vec(),
        role_name: spec.role_name.to_vec(),
        term_required: false,
        role_status: InstitutionRoleStatus::Active,
        seats: spec.max_members,
    };
    let assignments = (0..count)
        .map(|i| DecodedAssignment {
            cid_number: role.cid_number.clone(),
            person_cid: person(i),
            role_code: role.role_code.clone(),
            term_start: 0,
            term_end: 0,
            assignment_source: InstitutionAssignmentSource::PopularElection,
            assignment_source_ref: b"election".to_vec(),
            assignment_status: InstitutionAssignmentStatus::Active,
        })
        .collect::<Vec<_>>();
    let [entity, key, rows, permissions] = storage_key::composition_keys(spec);
    BTreeMap::from([
        (entity, institution.encode()),
        (key, role.encode()),
        (rows, assignments.encode()),
        (
            permissions,
            Vec::<entitybase::RoleBusinessPermission<Vec<u8>, Vec<u8>, Vec<u8>>>::new().encode(),
        ),
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
fn registered_institution_can_await_member_role_without_orphan_data() {
    let spec = member_composition_specs()[0];
    let parent = formed_state(&spec, spec.min_members);
    let keys = storage_key::composition_keys(&spec);
    let mut state = BTreeMap::from([(keys[0].clone(), parent[&keys[0]].clone())]);
    assert_eq!(
        composition_state(&spec, &|k| state.get(k).cloned()),
        Ok(CompositionState::Unconstituted)
    );
    for orphan in [2, 3] {
        state.insert(keys[orphan].clone(), parent[&keys[orphan]].clone());
        assert_eq!(
            composition_state(&spec, &|k| state.get(k).cloned()),
            Err(GuardError::PartialUnconstitutedState(spec.institution.code))
        );
        state.remove(&keys[orphan]);
    }
    let delta = keys[1..].iter().map(|k| (k.clone(), None)).collect();
    assert_eq!(
        check_transition(
            &delta,
            |k| parent.get(k).cloned(),
            |k| state.get(k).cloned(),
            None
        ),
        Err(GuardError::ConstitutedBodyRemoved(spec.institution.code))
    );
    state.insert(keys[0].clone(), vec![0]);
    assert_eq!(
        composition_state(&spec, &|k| state.get(k).cloned()),
        Err(GuardError::InstitutionDecodeFailed(spec.institution.code))
    );
}

#[test]
fn person_identity_and_unique_seat_are_required() {
    let spec = member_composition_specs()[0];
    let mut state = formed_state(&spec, spec.min_members);
    let key = storage_key::composition_keys(&spec)[2].clone();
    let original: Vec<DecodedAssignment> = decode_exact(state.get(&key).unwrap()).unwrap();
    let mut rows = original.clone();
    rows[1].person_cid = rows[0].person_cid.clone();
    state.insert(key.clone(), rows.encode());
    assert_eq!(
        check_full_state(|k| state.get(k).cloned()),
        Err(GuardError::DuplicateMember(spec.institution.code))
    );
    let mut rows = original;
    rows[0].person_cid = b"invalid".to_vec();
    state.insert(key, rows.encode());
    assert_eq!(
        check_full_state(|k| state.get(k).cloned()),
        Err(GuardError::PersonCidInvalid(spec.institution.code))
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
        check_vote_state_keys(state.keys(), |key| state.get(key).cloned()),
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
        let id = index as u128 + 20;
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

#[test]
fn exact_hashed_suffix_pairs_proposal_and_threshold_without_id_decoding() {
    let fixed = primitives::governance_skeleton::fixed_institutions()[0];
    let expected = primitives::cid::code::fixed_governance_pass_threshold(&fixed.code).unwrap();
    let proposal = storage_key::proposal(u128::MAX);
    let threshold = storage_key::threshold(u128::MAX);
    assert_eq!(
        storage_key::proposal_for_threshold(&threshold).unwrap(),
        proposal
    );
    assert_eq!(
        storage_key::threshold_for_proposal(&proposal).unwrap(),
        threshold
    );
    let raw = (0u8, 0u8, 0u8, Some(fixed.code)).encode();
    let mut state = BTreeMap::from([
        (proposal.clone(), raw),
        (threshold.clone(), (expected - 1).encode()),
    ]);
    assert!(check_vote_state_keys([&proposal, &threshold], |k| state.get(k).cloned()).is_err());
    state.insert(threshold.clone(), expected.encode());
    check_vote_state_keys([&proposal, &threshold], |k| state.get(k).cloned()).unwrap();
    let mut bad = proposal;
    bad[32] ^= 1;
    assert!(storage_key::threshold_for_proposal(&bad).is_err());
}

#[test]
fn permission_subject_cannot_be_reassigned_to_a_different_body() {
    let spec = member_composition_specs()[0];
    let mut state = formed_state(&spec, spec.min_members);
    let permission = entitybase::RoleBusinessPermission {
        role_subject: entitybase::RoleSubject {
            cid_number: b"other".to_vec(),
            role_code: spec.role_code.to_vec(),
        },
        business_action_id: entitybase::BusinessActionId {
            module_tag: b"leg-yuan".to_vec(),
            action_code: 0,
        },
        operation: entitybase::RolePermissionOperation::Propose,
    };
    state.insert(
        storage_key::composition_keys(&spec)[3].clone(),
        vec![permission].encode(),
    );
    assert_eq!(
        check_full_state(|k| state.get(k).cloned()),
        Err(GuardError::PermissionsChanged(spec.institution.code))
    );
}
