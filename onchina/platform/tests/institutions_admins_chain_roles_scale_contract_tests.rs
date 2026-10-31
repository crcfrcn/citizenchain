//! 原所属模块回归，断言与输入保持。
use super::*;
use entitybase::{AuthorizationSubject, BusinessActionId, RoleBusinessPermission, RoleSubject};
use frame_support::pallet_prelude::{BoundedVec, ConstU32};

type FixtureCid = BoundedVec<u8, ConstU32<32>>;
type FixtureRoleCode = BoundedVec<u8, ConstU32<64>>;
type FixtureModuleTag = BoundedVec<u8, ConstU32<32>>;
type FixtureSubject = AuthorizationSubject<FixtureCid, FixtureRoleCode, [u8; 32]>;
type FixtureVoters = BoundedVec<FixtureSubject, ConstU32<256>>;
type FixtureVotePlan = (
    BusinessActionId<FixtureModuleTag>,
    FixtureModuleTag,
    FixtureSubject,
    FixtureVoters,
    u8,
    [u8; 32],
);

#[test]
fn assignment_term_window_is_inclusive_and_non_term_requires_zeroes() {
    let role = RawInstitutionRole {
        cid_number: b"CID".to_vec(),
        role_code: b"ROLE".to_vec(),
        role_name: b"Role".to_vec(),
        term_required: true,
        role_status: ROLE_STATUS_ACTIVE,
    };
    let mut assignment = RawInstitutionAssignment {
        cid_number: b"CID".to_vec(),
        account_id: [1; 32],
        role_code: b"ROLE".to_vec(),
        term_start: 10,
        term_end: 20,
        assignment_source: 5,
        assignment_source_ref: b"proposal".to_vec(),
        assignment_status: ASSIGNMENT_STATUS_ACTIVE,
    };
    assert!(assignment_is_effective(&role, &assignment, 10));
    assert!(assignment_is_effective(&role, &assignment, 20));
    assert!(!assignment_is_effective(&role, &assignment, 21));
    assignment.term_start = 0;
    assignment.term_end = 0;
    assert!(!assignment_is_effective(&role, &assignment, 10));
}

#[test]
fn role_assignment_anchor_survives_admin_account_rebind() {
    let role = RawInstitutionRole {
        cid_number: b"CID".to_vec(),
        role_code: b"LR".to_vec(),
        role_name: "法定代表人".as_bytes().to_vec(),
        term_required: false,
        role_status: ROLE_STATUS_ACTIVE,
    };
    let assignment = RawInstitutionAssignment {
        cid_number: b"CID".to_vec(),
        account_id: [0x11; 32],
        role_code: b"LR".to_vec(),
        term_start: 0,
        term_end: 0,
        assignment_source: 5,
        assignment_source_ref: b"proposal".to_vec(),
        assignment_status: ASSIGNMENT_STATUS_ACTIVE,
    };
    let current_account_id = format!("0x{}", hex::encode([0x22; 32]));
    let active_admins = HashMap::from([(
        [0x11; 32],
        ActiveAdminRoleProjection {
            account_id: current_account_id.clone(),
            family_name: "程".to_string(),
            given_name: "伟".to_string(),
        },
    )]);
    let views = merge_active_assignments(vec![role], vec![assignment], &active_admins, 10)
        .expect("role assignment must join through the roster anchor");
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].account_id, current_account_id);
    assert_ne!(
        views[0].account_id,
        format!("0x{}", hex::encode([0x11; 32]))
    );
    assert_eq!(views[0].role_code, "LR");
}

fn fixture_case(name: &str) -> Vec<u8> {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtime/primitives/tests/fixtures/role_permission.json"
    )))
    .expect("岗位权限 fixture 必须是合法 JSON");
    let encoded = fixture["cases"]
        .as_array()
        .and_then(|cases| {
            cases
                .iter()
                .find(|case| case["name"].as_str() == Some(name))
        })
        .and_then(|case| case["encoded_hex"].as_str())
        .expect("岗位权限 fixture 用例必须存在");
    hex::decode(encoded).expect("岗位权限 fixture 必须是合法 hex")
}

fn decode_exact<T: Decode>(bytes: &[u8]) -> T {
    let mut input = bytes;
    let decoded = T::decode(&mut input).expect("fixture SCALE 必须可解码");
    assert!(input.is_empty(), "fixture SCALE 不得存在尾随字节");
    decoded
}

#[test]
fn institution_role_permission_fixture_decodes_exactly() {
    let role: RoleSubject<Vec<u8>, Vec<u8>> =
        decode_exact(&fixture_case("role_subject_nrc_committee"));
    assert_eq!(role.cid_number, b"LN001-NRC0G-944805165-2026");
    assert_eq!(role.role_code, b"COMMITTEE_MEMBER");

    let _: RoleBusinessPermission<Vec<u8>, Vec<u8>, Vec<u8>> =
        decode_exact(&fixture_case("permission_resolution_issuance_propose"));
    let _: AuthorizationSubject<Vec<u8>, Vec<u8>, [u8; 32]> =
        decode_exact(&fixture_case("authorization_personal_multisig"));

    let plan: FixtureVotePlan = decode_exact(&fixture_case("vote_plan_resolution_issuance_joint"));
    assert_eq!(plan.1.as_slice(), b"res-iss");
    assert_eq!(plan.3.len(), 3);
    assert_eq!(plan.4, 1);
    assert_eq!(plan.5, [0xabu8; 32]);
}
