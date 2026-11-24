//! 锁定实体共享SCALE、人员岗位选择、本机授权及具体业务权限的拒绝边界。
use super::*;
use codec::{DecodeAll, Encode};
fn person(kind: &str, seed: &str) -> String {
    primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: seed,
            p1: "1",
            province_code: "GD",
            province_name: "广东省",
            city_code: "001",
            city_name: "荔湾市",
            year: 2026,
            institution: kind,
        },
    )
    .unwrap()
}
fn fixture() -> InstitutionRolesState {
    let a = InstitutionAssignmentInfo {
        person_cid: person("CTZN", "one"),
        account_id: Some(format!("0x{}", "11".repeat(32))),
        binding_revision: Some("1".into()),
        family_name: String::new(),
        given_name: String::new(),
        term_start: 0,
        term_end: 0,
        assignment_source: 1,
        assignment_source_label: "注册局".into(),
        assignment_source_ref: "0xff00".into(),
        active: true,
        effective: true,
    };
    let role = InstitutionRoleInfo {
        role_code: "R1".into(),
        role_name: "岗位一".into(),
        term_required: false,
        seats: 2,
        active: true,
        permissions: vec![InstitutionRolePermissionInfo {
            module_tag: "multisig".into(),
            action_code: 0,
            operation: 0,
            operation_label: "发起提案".into(),
        }],
        assignments: vec![a],
    };
    InstitutionRolesState {
        cid_number: "institution".into(),
        institution_code: *b"CREG",
        kind: 0,
        block_hash: "block".into(),
        roles: vec![role],
    }
}
#[test]
fn scale_uses_person_and_seats_and_rejects_trailing_wallet_layout() {
    let role = entitybase::InstitutionRole {
        cid_number: b"cid".to_vec(),
        role_code: b"LR".to_vec(),
        role_name: b"legal".to_vec(),
        term_required: false,
        role_status: entitybase::InstitutionRoleStatus::Active,
        seats: 1,
    };
    let bytes = role.encode();
    assert_eq!(
        entitybase::InstitutionRole::<Vec<u8>, Vec<u8>, Vec<u8>>::decode_all(&mut bytes.as_slice())
            .unwrap()
            .seats,
        1
    );
    let a: entitybase::InstitutionAssignment<Vec<u8>, Vec<u8>, Vec<u8>> =
        entitybase::InstitutionAssignment {
            cid_number: b"cid".to_vec(),
            person_cid: person("CTZN", "one").into_bytes(),
            role_code: b"LR".to_vec(),
            term_start: 0,
            term_end: 0,
            assignment_source: entitybase::InstitutionAssignmentSource::Registry,
            assignment_source_ref: vec![255, 0],
            assignment_status: entitybase::InstitutionAssignmentStatus::Active,
        };
    let mut bytes = a.encode();
    assert!(
        entitybase::InstitutionAssignment::<Vec<u8>, Vec<u8>, Vec<u8>>::decode_all(
            &mut bytes.as_slice()
        )
        .is_ok()
    );
    bytes.push(0);
    assert!(
        entitybase::InstitutionAssignment::<Vec<u8>, Vec<u8>, Vec<u8>>::decode_all(
            &mut bytes.as_slice()
        )
        .is_err()
    );
}
#[test]
fn namespace_and_full_cid_determine_eligible_person_types() {
    use crate::storage::{person_allowed, EntityPallet};
    let citizen = person("CTZN", "one");
    let resident = person("NATP", "two");
    assert!(person_allowed(citizen.as_bytes(), EntityPallet::Public));
    assert!(!person_allowed(resident.as_bytes(), EntityPallet::Public));
    assert!(person_allowed(resident.as_bytes(), EntityPallet::Private));
    for bad in ["CTZN", "invalid", &format!("{citizen}X")] {
        assert!(!person_allowed(bad.as_bytes(), EntityPallet::Private));
    }
}
#[test]
fn access_matches_exact_person_role_wallet_and_true_binding() {
    let mut s = fixture();
    let a = &s.roles[0].assignments[0];
    let access = RoleAccess {
        cid_number: s.cid_number.clone(),
        role_code: "R1".into(),
        person_cid: a.person_cid.clone(),
        account_id: a.account_id.clone().unwrap(),
        binding_revision: "1".into(),
        activated_at_ms: 1000,
    };
    assert!(s.matches_access(&access));
    for field in ["person", "role", "institution", "wallet", "revision"] {
        let mut changed = access.clone();
        match field {
            "person" => changed.person_cid = person("CTZN", "two"),
            "role" => changed.role_code = "R2".into(),
            "institution" => changed.cid_number = "other".into(),
            "wallet" => changed.account_id = format!("0x{}", "22".repeat(32)),
            _ => changed.binding_revision = "2".into(),
        };
        assert!(!s.matches_access(&changed), "{field}");
    }
    s.roles[0].assignments[0].effective = false;
    assert!(!s.matches_access(&access));
    s.roles[0].assignments[0].effective = true;
    s.roles[0].assignments[0].account_id = None;
    assert!(!s.matches_access(&access));
}
#[test]
fn permission_checks_selected_role_action_and_operation() {
    let s = fixture();
    let person = &s.roles[0].assignments[0].person_cid;
    assert!(s.permits(person, "R1", "multisig", 0, 0));
    for (role, module, action, op) in [
        ("R2", "multisig", 0, 0),
        ("R1", "other", 0, 0),
        ("R1", "multisig", 1, 0),
        ("R1", "multisig", 0, 1),
    ] {
        assert!(!s.permits(person, role, module, action, op));
    }
}
#[test]
fn unassigned_roles_and_unbound_people_remain_visible_without_roster() {
    let mut s = fixture();
    s.roles[0].assignments[0].account_id = None;
    let mut empty = s.roles[0].clone();
    empty.role_code = "LR".into();
    empty.seats = 1;
    empty.assignments.clear();
    s.roles.push(empty);
    let json = serde_json::to_value(s).unwrap();
    assert!(json.get("admins").is_none());
    assert_eq!(json["roles"][1]["seats"], 1);
    assert_eq!(json["roles"][1]["assignments"], serde_json::json!([]));
    assert!(json["roles"][0]["assignments"][0]["accountId"].is_null());
}
#[test]
fn local_access_requires_identity_fields_without_admin_version() {
    let a = RoleAccess {
        cid_number: "cid".into(),
        role_code: "R1".into(),
        person_cid: person("CTZN", "one"),
        account_id: format!("0x{}", "11".repeat(32)),
        binding_revision: "1".into(),
        activated_at_ms: 1000,
    };
    let raw = serde_json::to_value(a).unwrap();
    for key in ["personCid", "roleCode", "bindingRevision"] {
        let mut bad = raw.clone();
        bad.as_object_mut().unwrap().remove(key);
        assert!(serde_json::from_value::<RoleAccess>(bad).is_err());
    }
    let mut bad = raw;
    bad["walletRevision"] = "0".into();
    assert!(serde_json::from_value::<RoleAccess>(bad).is_err());
}

#[test]
fn same_person_can_select_distinct_roles_without_combining_local_access() {
    let mut s = fixture();
    let mut second = s.roles[0].clone();
    second.role_code = "R2".into();
    s.roles.push(second);
    let person = s.roles[0].assignments[0].person_cid.clone();
    let account = s.roles[0].assignments[0].account_id.clone().unwrap();
    let first = crate::activation::selected_access(&s, &person, "R1", &account).unwrap();
    let second = crate::activation::selected_access(&s, &person, "R2", &account).unwrap();
    assert_ne!(first.role_code, second.role_code);
    assert!(s.matches_access(&first));
    assert!(s.matches_access(&second));
    assert!(crate::activation::selected_access(&s, &person, "OTHER", &account).is_err());
}
