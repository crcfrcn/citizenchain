//! 与Runtime共享类型核对岗位、人员任职、状态哈希及纯岗位治理边界。
use super::*;
use codec::{DecodeAll, Encode};
fn fixture() -> RegistrationSnapshot {
    let cid = primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: "role-fixture",
            p1: "1",
            province_code: "GD",
            province_name: "广东省",
            city_code: "001",
            city_name: "荔湾市",
            year: 2026,
            institution: "CREG",
        },
    )
    .unwrap()
    .into_bytes();
    let person = crate::testing::person_cid().into_bytes();
    let role = Role {
        cid_number: cid.clone(),
        role_code: b"R_EDIT".to_vec(),
        role_name: "编辑岗位".as_bytes().to_vec(),
        term_required: false,
        role_status: entitybase::InstitutionRoleStatus::Active,
        seats: 2,
    };
    let a = Assignment {
        cid_number: cid.clone(),
        person_cid: person,
        role_code: role.role_code.clone(),
        term_start: 0,
        term_end: 0,
        assignment_source: entitybase::InstitutionAssignmentSource::Registry,
        assignment_source_ref: vec![0xff, 0x00],
        assignment_status: entitybase::InstitutionAssignmentStatus::Active,
    };
    let key = role.role_code.clone();
    let mut roles = BTreeMap::from([(key.clone(), role)]);
    roles.insert(
        b"LR".to_vec(),
        Role {
            cid_number: cid.clone(),
            role_code: b"LR".to_vec(),
            role_name: "法定代表人".as_bytes().to_vec(),
            term_required: false,
            role_status: entitybase::InstitutionRoleStatus::Active,
            seats: 1,
        },
    );
    RegistrationSnapshot {
        owner: crate::chain_runtime::EntityPallet::Public,
        block_hash: [1; 32],
        block_number: 12,
        current_day: 20000,
        cid_number: cid,
        institution: Institution {
            cid_full_name: "机构".as_bytes().to_vec(),
            cid_short_name: "机构".as_bytes().to_vec(),
            town_code: Vec::new(),
            legal_representative: None,
            institution_code: *b"CREG",
            created_at: 0,
        },
        roles,
        permissions: vec![(b"LR".to_vec(), vec![]), (key.clone(), vec![])],
        assignments: vec![(b"LR".to_vec(), vec![]), (key, vec![a])],
        nonce: 17,
        threshold: Some(2),
        max_seats: 3,
    }
}
#[test]
fn person_types_use_complete_canonical_cid_and_entity_kind() {
    let citizen = crate::testing::person_cid();
    let resident = crate::testing::person_for("NATP", "resident");
    assert!(person_allowed(citizen.as_bytes(), EntityPallet::Public));
    assert!(person_allowed(resident.as_bytes(), EntityPallet::Private));
    assert!(!person_allowed(resident.as_bytes(), EntityPallet::Public));
    for bad in [
        "CTZN",
        "invalid",
        "",
        " SMTP",
        citizen.trim_end_matches('6'),
    ] {
        assert!(!person_allowed(bad.as_bytes(), EntityPallet::Public));
    }
    assert!(!person_allowed(
        format!(" {citizen}").as_bytes(),
        EntityPallet::Public
    ));
    assert!(!person_allowed(&fixture().cid_number, EntityPallet::Public));
}
#[test]
fn role_and_assignment_scale_are_exactly_shared_runtime_types() {
    let s = fixture();
    let role = s.roles.get(b"R_EDIT".as_slice()).unwrap();
    let a = &s.assignments[1].1[0];
    let decoded = Role::decode_all(&mut role.encode().as_slice()).unwrap();
    assert_eq!(decoded.seats, 2);
    let decoded = Assignment::decode_all(&mut a.encode().as_slice()).unwrap();
    assert_eq!(decoded.person_cid, a.person_cid);
    let mut broken = a.encode();
    broken.push(0);
    assert!(Assignment::decode_all(&mut broken.as_slice()).is_err());
}
#[test]
fn effective_terms_status_and_role_state_are_all_required() {
    let s = fixture();
    let mut role = s.roles[b"R_EDIT".as_slice()].clone();
    let mut a = s.assignments[1].1[0].clone();
    assert!(effective(&role, &a, 20000));
    a.term_start = 1;
    assert!(!effective(&role, &a, 20000));
    role.term_required = true;
    a.term_start = 100;
    a.term_end = 200;
    assert!(effective(&role, &a, 100));
    assert!(effective(&role, &a, 200));
    assert!(!effective(&role, &a, 99));
    assert!(!effective(&role, &a, 201));
    a.assignment_status = entitybase::InstitutionAssignmentStatus::Ended;
    assert!(!effective(&role, &a, 150));
    a.assignment_status = entitybase::InstitutionAssignmentStatus::Active;
    role.role_status = entitybase::InstitutionRoleStatus::Inactive;
    assert!(!effective(&role, &a, 150));
}
#[test]
fn state_hash_matches_role_registration_contract_and_has_no_wallet_dimension() {
    let mut s = fixture();
    let expected = entitybase::institution_governance::registration_state_hash(
        s.module_tag(),
        &(
            &s.cid_number,
            &s.institution,
            &s.roles,
            &s.permissions,
            &s.assignments,
            s.nonce,
            s.threshold,
        ),
    );
    assert_eq!(s.state_hash(), expected);
    let first = s.state_hash();
    s.roles.get_mut(b"R_EDIT".as_slice()).unwrap().seats = 3;
    assert_ne!(s.state_hash(), first);
    let second = s.state_hash();
    s.assignments[1].1[0].person_cid = crate::testing::person_for("CTZN", "other").into_bytes();
    assert_ne!(s.state_hash(), second);
}
#[test]
fn unassigned_roles_and_unique_legal_seat_are_visible_without_member_list() {
    let s = fixture();
    let state = s.governance_state(&[]).unwrap();
    assert_eq!(state.roles.len(), 2);
    assert_eq!(state.roles[0].role_code, "LR");
    assert_eq!(state.roles[0].seats, 1);
    assert!(state.roles[0].assignments.is_empty());
    let json = serde_json::to_value(state).unwrap();
    assert!(json.get("members").is_none());
    assert!(json.get("walletRevision").is_none());
    assert!(json.get("registrationStateHash").is_some());
}
#[test]
fn retained_assignments_preserve_real_provenance_and_do_not_infer_a_wallet() {
    let s = fixture();
    let a = &s.assignments[1].1[0];
    let retained = s
        .retained_assignment(b"R_EDIT", &a.person_cid, 0, 0)
        .unwrap();
    assert_eq!(retained.assignment_source_ref, vec![0xff, 0x00]);
    assert_eq!(
        retained.assignment_source,
        entitybase::InstitutionAssignmentSource::Registry
    );
    assert!(s
        .retained_assignment(b"R_EDIT", &a.person_cid, 1, 2)
        .is_none());
}
fn target(person: String) -> crate::institutions::InstitutionAssignmentTargetInput {
    crate::institutions::InstitutionAssignmentTargetInput {
        person_cid: person,
        term_start: 0,
        term_end: 0,
    }
}
#[test]
fn duplicate_people_cannot_occupy_two_seats_in_one_role() {
    let s = fixture();
    let person = crate::testing::person_cid();
    let c = crate::institutions::InstitutionAssignmentChangeInput {
        role_code: "R_EDIT".into(),
        assignments: vec![target(person.clone()), target(person)],
    };
    assert!(crate::institutions::parse_role_action(&s, &[], &[c], true).is_err());
}
#[test]
fn same_person_may_hold_different_roles_while_legal_seat_cannot_overfill() {
    let s = fixture();
    let person = crate::testing::person_cid();
    let changes = vec![
        crate::institutions::InstitutionAssignmentChangeInput {
            role_code: "R_EDIT".into(),
            assignments: vec![target(person.clone())],
        },
        crate::institutions::InstitutionAssignmentChangeInput {
            role_code: "LR".into(),
            assignments: vec![target(person.clone())],
        },
    ];
    assert!(crate::institutions::parse_role_action(&s, &[], &changes, false).is_ok());
    let mut changes = changes;
    changes[1]
        .assignments
        .push(target(crate::testing::person_for("CTZN", "second")));
    assert!(crate::institutions::parse_role_action(&s, &[], &changes, false).is_err());
}
#[test]
fn public_rejects_residents_private_accepts_them_without_preregistering_members() {
    let mut s = fixture();
    let change = crate::institutions::InstitutionAssignmentChangeInput {
        role_code: "R_EDIT".into(),
        assignments: vec![target(crate::testing::person_for("NATP", "resident"))],
    };
    assert!(crate::institutions::parse_role_action(&s, &[], &[change.clone()], true).is_err());
    s.owner = EntityPallet::Private;
    assert!(crate::institutions::parse_role_action(&s, &[], &[change], true).is_ok());
}
#[test]
fn new_roles_require_positive_bounded_seats_and_exact_permission_operations() {
    let s = fixture();
    let mut create = crate::institutions::InstitutionRoleMutationInput {
        mutation: "CREATE".into(),
        role_code: None,
        role_name: Some("新岗位".into()),
        term_required: false,
        seats: Some(2),
        permissions: vec![crate::institutions::InstitutionRolePermissionInput {
            module_tag: "pub-mgmt".into(),
            action_code: 3,
            operation: "PROPOSE".into(),
        }],
        assignments: vec![target(crate::testing::person_cid())],
    };
    assert!(crate::institutions::parse_role_action(&s, &[create.clone()], &[], true).is_ok());
    for seats in [None, Some(0), Some(4)] {
        create.seats = seats;
        assert!(crate::institutions::parse_role_action(&s, &[create.clone()], &[], true).is_err());
    }
    create.seats = Some(1);
    create.role_code = Some("CLIENT_CODE".into());
    assert!(crate::institutions::parse_role_action(&s, &[create], &[], true).is_err());
}
#[test]
fn rename_cannot_change_capacity_permissions_or_fixed_legal_role() {
    let s = fixture();
    let mut mutation = crate::institutions::InstitutionRoleMutationInput {
        mutation: "RENAME".into(),
        role_code: Some("R_EDIT".into()),
        role_name: Some("新名称".into()),
        term_required: false,
        seats: None,
        permissions: vec![],
        assignments: vec![],
    };
    assert!(crate::institutions::parse_role_action(&s, &[mutation.clone()], &[], false).is_ok());
    mutation.seats = Some(3);
    assert!(crate::institutions::parse_role_action(&s, &[mutation.clone()], &[], false).is_err());
    mutation.seats = None;
    mutation.role_code = Some("LR".into());
    assert!(crate::institutions::parse_role_action(&s, &[mutation], &[], false).is_err());
}
#[test]
fn registration_selection_never_silently_chooses_another_role() {
    let roles = [
        RegistrationRoleChoice {
            role_code: "R1".into(),
            role_name: "岗位一".into(),
        },
        RegistrationRoleChoice {
            role_code: "R2".into(),
            role_name: "岗位二".into(),
        },
    ];
    assert!(select_registration_role(&roles, None).is_err());
    assert_eq!(select_registration_role(&roles, Some("R2")).unwrap(), "R2");
    assert!(select_registration_role(&roles, Some("R3")).is_err());
}

#[test]
fn legal_representative_and_unique_assignment_cannot_diverge() {
    let s = fixture();
    let person = crate::testing::person_cid();
    let change = entitybase::InstitutionLegalRepresentativeChange::Set {
        family_name: b"family".to_vec(),
        given_name: b"given".to_vec(),
        person_cid: person.as_bytes().to_vec(),
    };
    assert!(
        crate::institutions::governance::validate_legal_change(&s, &[], Some(&change)).is_err()
    );
    let rows = crate::institutions::parse_role_action(
        &s,
        &[],
        &[crate::institutions::InstitutionAssignmentChangeInput {
            role_code: "LR".into(),
            assignments: vec![target(person)],
        }],
        false,
    )
    .unwrap()
    .1;
    assert!(
        crate::institutions::governance::validate_legal_change(&s, &rows, Some(&change)).is_ok()
    );
    assert!(crate::institutions::governance::validate_legal_change(&s, &rows, None).is_err());
    assert!(crate::institutions::governance::validate_legal_change(
        &s,
        &rows,
        Some(&entitybase::InstitutionLegalRepresentativeChange::Clear)
    )
    .is_err());
    assert!(crate::institutions::governance::validate_legal_change(
        &s,
        &[],
        Some(&entitybase::InstitutionLegalRepresentativeChange::Clear)
    )
    .is_ok());
}
#[test]
fn registration_cannot_replace_protected_legal_assignments() {
    let s = fixture();
    let changes = [crate::institutions::InstitutionAssignmentChangeInput {
        role_code: "LR".into(),
        assignments: vec![target(crate::testing::person_cid())],
    }];
    assert!(crate::institutions::parse_role_action(&s, &[], &changes, true).is_err());
    assert!(crate::institutions::parse_role_action(&s, &[], &changes, false).is_ok());
}
