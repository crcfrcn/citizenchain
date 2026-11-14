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

#[test]
fn operation_role_permission_matches_exact_subject_action_and_propose() {
    let mut permission: BusinessPermission =
        decode_exact(&fixture_case("permission_resolution_issuance_propose"));
    let action = permission.business_action_id.clone();
    let cid = permission.role_subject.cid_number.clone();
    let role = permission.role_subject.role_code.clone();
    assert!(permission_allows_proposal(
        &[permission.clone()],
        &cid,
        &role,
        &action
    ));
    assert!(!permission_allows_proposal(
        &[permission.clone()],
        b"other",
        &role,
        &action
    ));
    assert!(!permission_allows_proposal(
        &[permission.clone()],
        &cid,
        b"other",
        &action
    ));
    let mut other = action.clone();
    other.action_code += 1;
    assert!(!permission_allows_proposal(
        &[permission.clone()],
        &cid,
        &role,
        &other
    ));
    permission.operation = entitybase::RolePermissionOperation::Vote;
    assert!(!permission_allows_proposal(
        &[permission],
        &cid,
        &role,
        &action
    ));
}

#[test]
fn expired_ended_or_inactive_assignments_never_become_role_candidates() {
    let mut role = RawInstitutionRole {
        cid_number: b"cid".to_vec(),
        role_code: b"ROLE".to_vec(),
        role_name: "登记员".as_bytes().to_vec(),
        term_required: true,
        role_status: ROLE_STATUS_ACTIVE,
    };
    let mut assignment = RawInstitutionAssignment {
        cid_number: b"cid".to_vec(),
        account_id: [1; 32],
        role_code: b"ROLE".to_vec(),
        term_start: 5,
        term_end: 10,
        assignment_source: 0,
        assignment_source_ref: vec![],
        assignment_status: ASSIGNMENT_STATUS_ACTIVE,
    };
    let admins = HashMap::from([(
        [1; 32],
        ActiveAdminRoleProjection {
            account_id: format!("0x{}", hex::encode([2; 32])),
            family_name: "测".into(),
            given_name: "试".into(),
        },
    )]);
    assert!(!assignment_is_effective(&role, &assignment, 11));
    assert!(!assignment_is_effective(&role, &assignment, 4));
    assert!(assignment_is_effective(&role, &assignment, 10));
    assignment.assignment_status = 1;
    assert!(
        merge_active_assignments(vec![role], vec![assignment], &admins, 10)
            .unwrap()
            .is_empty()
    );
    role = RawInstitutionRole {
        cid_number: b"cid".to_vec(),
        role_code: b"ROLE".to_vec(),
        role_name: vec![],
        term_required: false,
        role_status: 1,
    };
    assert!(merge_active_assignments(vec![role], vec![], &admins, 10)
        .unwrap()
        .is_empty());
}

#[test]
fn inactive_or_deleted_role_history_is_not_a_candidate_and_does_not_block_valid_roles() {
    let roles = vec![
        RawInstitutionRole {
            cid_number: b"cid".to_vec(),
            role_code: b"INACTIVE".to_vec(),
            role_name: "已停用岗位".as_bytes().to_vec(),
            term_required: false,
            role_status: 1,
        },
        RawInstitutionRole {
            cid_number: b"cid".to_vec(),
            role_code: b"ACTIVE".to_vec(),
            role_name: "有效岗位".as_bytes().to_vec(),
            term_required: false,
            role_status: ROLE_STATUS_ACTIVE,
        },
    ];
    let assignments = [
        b"INACTIVE".to_vec(),
        b"DELETED".to_vec(),
        b"ACTIVE".to_vec(),
    ]
    .into_iter()
    .map(|role_code| RawInstitutionAssignment {
        cid_number: b"cid".to_vec(),
        account_id: [1; 32],
        role_code,
        term_start: 0,
        term_end: 0,
        assignment_source: 0,
        assignment_source_ref: vec![],
        assignment_status: ASSIGNMENT_STATUS_ACTIVE,
    })
    .collect();
    let admins = HashMap::from([(
        [1; 32],
        ActiveAdminRoleProjection {
            account_id: format!("0x{}", hex::encode([2; 32])),
            family_name: "测".into(),
            given_name: "试".into(),
        },
    )]);
    let candidates = merge_authorization_assignments(roles, assignments, &admins, 20).unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].role_code, "ACTIVE");
}

#[test]
fn real_runtime_metadata_encodes_institution_prefix_and_exact_permission_keys() {
    use codec::Encode;
    use subxt::ext::subxt_core::storage::get_address_bytes;
    let metadata =
        subxt::Metadata::decode(&mut runtime::Runtime::metadata().encode().as_slice()).unwrap();
    let cid = primitives::governance_skeleton::federal_registry_institution()
        .cid_number
        .as_bytes();
    let role = primitives::governance_skeleton::province_commissioner_role_code(*b"GZ");
    for entry in [
        "InstitutionRoles",
        "InstitutionRoleAssignments",
        "InstitutionRolePermissions",
    ] {
        let prefix = dynamic::storage("PublicManage", entry, vec![dynamic::Value::from_bytes(cid)]);
        let exact = dynamic::storage(
            "PublicManage",
            entry,
            vec![
                dynamic::Value::from_bytes(cid),
                dynamic::Value::from_bytes(&role),
            ],
        );
        let prefix_bytes = get_address_bytes(&prefix, &metadata).unwrap();
        let exact_bytes = get_address_bytes(&exact, &metadata).unwrap();
        assert!(prefix_bytes.len() > 32);
        assert!(exact_bytes.len() > prefix_bytes.len());
        assert!(exact_bytes.starts_with(&prefix_bytes));
    }
}

#[test]
fn governance_definitions_include_unassigned_roles_and_keep_assignment_anchors() {
    let cid = "GZ018-SFGYR-201206100-2026";
    let role = |code: &[u8]| RawInstitutionRole {
        cid_number: cid.as_bytes().to_vec(),
        role_code: code.to_vec(),
        role_name: code.to_vec(),
        term_required: false,
        role_status: 0,
    };
    let assignment = RawInstitutionAssignment {
        cid_number: cid.as_bytes().to_vec(),
        role_code: b"ROLE".to_vec(),
        account_id: [0x11; 32],
        term_start: 0,
        term_end: 0,
        assignment_source: 5,
        assignment_source_ref: vec![1],
        assignment_status: 0,
    };
    let rows = governance_roles(
        *b"SFGY",
        cid,
        vec![role(b"ROLE"), role(b"UNASSIGNED"), role(b"LR")],
        vec![assignment],
        HashMap::new(),
        10,
    )
    .unwrap();
    assert_eq!(rows.len(), 3);
    let empty = rows.iter().find(|r| r.role_code == "UNASSIGNED").unwrap();
    assert!(empty.assignments.is_empty());
    let assigned = rows.iter().find(|r| r.role_code == "ROLE").unwrap();
    assert_eq!(
        assigned.assignments[0].account_id,
        format!("0x{}", hex::encode([0x11; 32]))
    );
    let legal = rows.iter().find(|r| r.role_code == "LR").unwrap();
    assert!(legal.fixed && legal.assignment_locked);
    let json = serde_json::to_value(legal).unwrap();
    assert!(json.get("roleCode").is_some());
    assert!(json.get("role_code").is_none());
}

#[test]
fn governance_permissions_cannot_cross_cid_or_role_subject() {
    let cid = "GZ018-SFGYR-201206100-2026";
    let role = || RawInstitutionRole {
        cid_number: cid.as_bytes().to_vec(),
        role_code: b"ROLE".to_vec(),
        role_name: b"Role".to_vec(),
        term_required: false,
        role_status: 0,
    };
    let permission = BusinessPermission {
        role_subject: RoleSubject {
            cid_number: b"another".to_vec(),
            role_code: b"ROLE".to_vec(),
        },
        business_action_id: BusinessActionId {
            module_tag: b"pri-mgmt".to_vec(),
            action_code: 3,
        },
        operation: entitybase::RolePermissionOperation::Propose,
    };
    assert!(governance_roles(
        *b"SFGY",
        cid,
        vec![role()],
        vec![],
        HashMap::from([(b"ROLE".to_vec(), vec![permission])]),
        10
    )
    .is_err());
    assert!(governance_roles(
        *b"SFGY",
        "another",
        vec![role()],
        vec![],
        HashMap::new(),
        10
    )
    .is_err());
}

#[test]
#[ignore = "由控制台交付隔离PostgreSQL后执行真实路由门禁"]
fn governance_read_keeps_login_and_first_passkey_gate() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    let sandbox = crate::database::IsolatedTestDb::new();
    let (state, _, _) = crate::testing::session_fixture(&sandbox.db, true);
    let app = crate::build_routes(state);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let path = "/api/institutions/ZS001-FRG07-249474503-2026/governance";
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = app
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header("authorization", "Bearer fixture-session")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    });
}

#[test]
fn province_commissioner_assignment_is_not_an_independent_governance_edit() {
    let cid = "ZS001-FRG07-249474503-2026";
    let code = primitives::governance_skeleton::province_commissioner_role_code(*b"GZ");
    let row = RawInstitutionRole {
        cid_number: cid.as_bytes().to_vec(),
        role_code: code,
        role_name: "省专员".as_bytes().to_vec(),
        term_required: false,
        role_status: 0,
    };
    let rows = governance_roles(
        primitives::cid::code::FRG,
        cid,
        vec![row],
        vec![],
        HashMap::new(),
        10,
    )
    .unwrap();
    assert!(rows[0].fixed && rows[0].assignment_locked);
    assert_eq!(rows[0].min_assignments, 5);
    assert_eq!(rows[0].max_assignments, Some(5));
}

#[test]
fn public_role_candidate_capability_matches_shared_registry_and_fixed_directories() {
    use entitybase::business_action::*;
    let creg = b"NX013-CREGN-937676656-2026".as_slice();
    let frg = primitives::governance_skeleton::federal_registry_institution()
        .cid_number
        .as_bytes();
    for cid in [creg, frg] {
        let code = primitives::cid::number::parse_cid_number_parts_bytes(cid)
            .unwrap()
            .institution;
        for module in [
            MODULE_CITIZEN_IDENTITY,
            MODULE_ADDRESS_REGISTRY,
            MODULE_INSTITUTION_REGISTRATION,
            MODULE_MULTISIG,
        ] {
            for action_code in 0..=9 {
                let action = BusinessActionId {
                    module_tag: module.to_vec(),
                    action_code,
                };
                let expected = entitybase::fixed_institution_capability_allows(
                    code,
                    cid,
                    module,
                    action_code,
                    entitybase::RolePermissionOperation::Propose,
                ) || registry_institution_capability_allows(
                    code,
                    cid,
                    module,
                    action_code,
                    entitybase::RolePermissionOperation::Propose,
                );
                assert_eq!(
                    public_role_capability_allows(cid, &action).unwrap(),
                    expected
                );
            }
        }
    }
    let action = |module: &[u8], action_code| BusinessActionId {
        module_tag: module.to_vec(),
        action_code,
    };
    assert!(public_role_capability_allows(
        creg,
        &action(MODULE_CITIZEN_IDENTITY, ACTION_REGISTER_VOTING_IDENTITY)
    )
    .unwrap());
    assert!(!public_role_capability_allows(
        creg,
        &action(MODULE_ADDRESS_REGISTRY, ACTION_SET_ADDRESS_CATALOG)
    )
    .unwrap());
    assert!(public_role_capability_allows(
        frg,
        &action(MODULE_ADDRESS_REGISTRY, ACTION_SET_ADDRESS_CATALOG)
    )
    .unwrap());
    assert!(
        public_role_capability_allows(
            b"invalid",
            &action(MODULE_CITIZEN_IDENTITY, ACTION_OCCUPY_CID)
        )
        .is_err(),
        "解析失败必须保留错误，不能变成空候选"
    );
}

#[test]
fn creg_inline_role_fixture_requires_effective_assignment_and_exact_proposal_permission() {
    use codec::Encode;
    use entitybase::business_action::*;
    let cid = b"NX013-CREGN-937676656-2026";
    let code = entitybase::generate_dynamic_role_code(b"pub-mgmt", cid, 0, 18);
    let role = || RawInstitutionRole {
        cid_number: cid.to_vec(),
        role_code: code.clone(),
        role_name: "测试动态岗位".as_bytes().to_vec(),
        term_required: true,
        role_status: ROLE_STATUS_ACTIVE,
    };
    let assignment = |term_end, status| RawInstitutionAssignment {
        cid_number: cid.to_vec(),
        account_id: [0x11; 32],
        role_code: code.clone(),
        term_start: 1,
        term_end,
        assignment_source: 5,
        assignment_source_ref: vec![18],
        assignment_status: status,
    };
    let current = format!("0x{}", hex::encode([0x22; 32]));
    let admins = HashMap::from([(
        [0x11; 32],
        ActiveAdminRoleProjection {
            account_id: current.clone(),
            family_name: "测".into(),
            given_name: "试".into(),
        },
    )]);
    let action = BusinessActionId {
        module_tag: MODULE_CITIZEN_IDENTITY.to_vec(),
        action_code: ACTION_REGISTER_VOTING_IDENTITY,
    };
    assert!(public_role_capability_allows(cid, &action).unwrap());
    assert!(merge_authorization_assignments(
        vec![role()],
        vec![assignment(20, ASSIGNMENT_STATUS_ACTIVE)],
        &HashMap::new(),
        10
    )
    .unwrap()
    .is_empty());
    for (end, status) in [(9, ASSIGNMENT_STATUS_ACTIVE), (20, 1)] {
        assert!(merge_authorization_assignments(
            vec![role()],
            vec![assignment(end, status)],
            &admins,
            10
        )
        .unwrap()
        .is_empty());
    }
    let rows = merge_authorization_assignments(
        vec![role()],
        vec![assignment(20, ASSIGNMENT_STATUS_ACTIVE)],
        &admins,
        10,
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].account_id, current,
        "岗位锚点沿用名册账户，返回当前绑定钱包"
    );
    assert!(!permission_allows_proposal(&[], cid, &code, &action));
    let mut permission = BusinessPermission {
        role_subject: RoleSubject {
            cid_number: cid.to_vec(),
            role_code: code.clone(),
        },
        business_action_id: action.clone(),
        operation: entitybase::RolePermissionOperation::Vote,
    };
    assert!(!permission_allows_proposal(
        &[permission.clone()],
        cid,
        &code,
        &action
    ));
    permission.operation = entitybase::RolePermissionOperation::Propose;
    let encoded = permission.encode();
    let decoded: BusinessPermission = decode_exact(&encoded);
    assert_eq!(decoded.encode(), encoded, "新增内联夹具复用原SCALE字段布局");
    assert!(permission_allows_proposal(&[decoded], cid, &code, &action));
    // 原跨端受保护金标逐字节回归，不因开放顶层能力而重写协议字节。
    let golden = fixture_case("permission_resolution_issuance_propose");
    let decoded: BusinessPermission = decode_exact(&golden);
    assert_eq!(decoded.encode(), golden);
}

// 登记快照夹具保留原始二进制来源、名册锚点以及岗位有序集合，不借用展示投影。
fn registration_snapshot_fixture() -> RegistrationSnapshot {
    let cid = b"NX013-CREGN-937676656-2026".to_vec();
    let code = b"R_EXISTING".to_vec();
    RegistrationSnapshot {
        block_hash: [7; 32],
        block_number: 9,
        current_day: 10,
        cid_number: cid.clone(),
        institution: entitybase::InstitutionInfo {
            cid_full_name: "市注册局".as_bytes().to_vec(),
            cid_short_name: vec![],
            town_code: vec![],
            legal_representative: None,
            institution_code: *b"CREG",
            created_at: 1,
        },
        admins: Some(vec![adminbase::Admin {
            account_id: [1; 32],
            cid_number: b"CN220-CTZN2-198805200-2026".to_vec().try_into().unwrap(),
            family_name: vec![b'A'].try_into().unwrap(),
            given_name: vec![b'B'].try_into().unwrap(),
        }]),
        roles: BTreeMap::from([(
            code.clone(),
            RawInstitutionRole {
                cid_number: cid.clone(),
                role_code: code.clone(),
                role_name: b"Existing".to_vec(),
                term_required: true,
                role_status: 0,
            },
        )]),
        permissions: vec![(code.clone(), vec![])],
        assignments: vec![(
            code.clone(),
            vec![RawInstitutionAssignment {
                cid_number: cid,
                role_code: code,
                account_id: [1; 32],
                term_start: 1,
                term_end: 20,
                assignment_source: 5,
                assignment_source_ref: vec![0xff, 0, 0x80, 0x7f],
                assignment_status: 0,
            }],
        )],
        nonce: 4,
        threshold: Some(2),
    }
}

#[test]
fn registration_snapshot_hash_matches_runtime_bounded_scale_layout_and_revision() {
    use codec::Encode;
    type Name = BoundedVec<u8, ConstU32<128>>;
    let mut snapshot = registration_snapshot_fixture();
    let institution: entitybase::InstitutionInfo<u32, Name, FixtureCid, [u8; 32]> =
        decode_exact(&snapshot.institution.encode());
    let raw_role = snapshot.roles.values().next().unwrap();
    let role: entitybase::InstitutionRole<FixtureCid, FixtureRoleCode, Name> =
        decode_exact(&raw_role.encode());
    let code: FixtureRoleCode = role.role_code.clone();
    let roles = BTreeMap::from([(code.clone(), role)]);
    let assignment: entitybase::InstitutionAdminAssignment<
        FixtureCid,
        [u8; 32],
        FixtureRoleCode,
        Name,
    > = decode_exact(&snapshot.assignments[0].1[0].encode());
    let permissions: Vec<(FixtureRoleCode, Vec<BusinessPermission>)> = vec![(code.clone(), vec![])];
    let assignments = vec![(code, vec![assignment])];
    let expected = entitybase::institution_governance::registration_state_hash(
        b"pub-mgmt",
        &(
            FixtureCid::try_from(snapshot.cid_number.clone()).unwrap(),
            institution,
            Some(vec![[1u8; 32]]),
            roles,
            permissions,
            assignments,
            4u64,
            Some(2u32),
        ),
    );
    assert_eq!(snapshot.state_hash(), expected);
    // 区块和当前钱包展示不参与授权状态；nonce推进、来源、权限和阈值改变必须失效。
    snapshot.block_number += 1;
    snapshot.current_day += 1;
    assert_eq!(snapshot.state_hash(), expected);
    snapshot.nonce += 1;
    assert_ne!(snapshot.state_hash(), expected);
    snapshot.nonce -= 1;
    snapshot.assignments[0].1[0].assignment_source_ref[0] ^= 1;
    assert_ne!(snapshot.state_hash(), expected);
    snapshot.assignments[0].1[0].assignment_source_ref[0] ^= 1;
    snapshot.threshold = None;
    assert_ne!(snapshot.state_hash(), expected);
    snapshot.threshold = Some(2);
    snapshot.admins = None;
    assert_ne!(snapshot.state_hash(), expected);
}

#[test]
fn registration_parser_preserves_old_provenance_and_stamps_only_new_assignments() {
    use crate::institutions::admins::{
        parse_registration_action, InstitutionAssignmentChangeInput, InstitutionRoleMutationInput,
    };
    let snapshot = registration_snapshot_fixture();
    let admins = crate::institutions_call::InstitutionAdminsPayload::Public(
        snapshot.admins.clone().unwrap(),
    );
    let changes: Vec<InstitutionAssignmentChangeInput> = serde_json::from_value(serde_json::json!([
        {"roleCode":"R_EXISTING","assignments":[{"accountId":format!("0x{}",hex::encode([1;32])),"termStart":1,"termEnd":20}]}
    ])).unwrap();
    let action = parse_registration_action(admins.clone(), &[], &changes, &snapshot, None).unwrap();
    let (_, _, assignments) = action.into_registration_parts().unwrap();
    assert_eq!(
        assignments[0].assignments[0].assignment_source,
        entitybase::InstitutionAssignmentSource::InstitutionGovernance
    );
    assert_eq!(
        assignments[0].assignments[0].assignment_source_ref,
        vec![0xff, 0, 0x80, 0x7f]
    );
    let mut changed = changes;
    changed[0].assignments[0].term_end = 21;
    let (_, _, assignments) =
        parse_registration_action(admins.clone(), &[], &changed, &snapshot, None)
            .unwrap()
            .into_registration_parts()
            .unwrap();
    assert_eq!(
        assignments[0].assignments[0].assignment_source,
        entitybase::InstitutionAssignmentSource::Registry
    );
    assert!(assignments[0].assignments[0]
        .assignment_source_ref
        .is_empty());
    let mutation: Vec<InstitutionRoleMutationInput> = serde_json::from_value(serde_json::json!([{
        "mutation":"CREATE","roleName":"业务岗位","permissions":[{"moduleTag":"cit-id","actionCode":0,"operation":"PROPOSE"}],
        "assignments":[{"accountId":format!("0x{}",hex::encode([1;32]))}]
    }])).unwrap();
    assert!(parse_registration_action(admins.clone(), &mutation, &[], &snapshot, Some(2)).is_err());
    let mut first = registration_snapshot_fixture();
    first.threshold = None;
    assert!(parse_registration_action(admins.clone(), &[], &[], &first, None).is_err());
    let (_, mutations, _) = parse_registration_action(admins, &mutation, &[], &first, Some(2))
        .unwrap()
        .into_registration_parts()
        .unwrap();
    let entitybase::InstitutionRoleMutation::Create { assignments, .. } = &mutations[0] else {
        panic!("create")
    };
    assert_eq!(
        assignments[0].assignment_source,
        entitybase::InstitutionAssignmentSource::Registry
    );
    assert!(assignments[0].assignment_source_ref.is_empty());
}

#[test]
fn registration_roles_require_existing_scope_and_choose_only_effective_candidates() {
    let target = b"NX013-CREGN-937676656-2026";
    let role = primitives::governance_skeleton::province_commissioner_role_code(*b"NX");
    assert!(registration_role_in_scope(
        b"ZS001-FRG07-249474503-2026",
        &role,
        target,
        *b"CREG"
    ));
    let wrong = primitives::governance_skeleton::province_commissioner_role_code(*b"GZ");
    assert!(!registration_role_in_scope(
        b"ZS001-FRG07-249474503-2026",
        &wrong,
        target,
        *b"CREG"
    ));
    assert!(!registration_role_in_scope(
        target, b"DYNAMIC", target, *b"CREG"
    ));
    let cfin = |r5: &str| {
        let payload = format!("{r5}CFIN1234567892026");
        format!(
            "{r5}-CFIN{}-123456789-2026",
            primitives::cid::number::checksum_char_m1(&payload, false)
        )
    };
    assert!(registration_role_in_scope(
        target,
        b"DYNAMIC",
        cfin("NX013").as_bytes(),
        *b"CFIN"
    ));
    assert!(!registration_role_in_scope(
        target,
        b"DYNAMIC",
        cfin("NX014").as_bytes(),
        *b"CFIN"
    ));
    let mut roles = vec![RegistrationRoleChoice {
        role_code: "R_ONE".into(),
        role_name: "登记办理岗".into(),
    }];
    assert_eq!(select_registration_role(&roles, None).unwrap(), "R_ONE");
    assert!(select_registration_role(&roles, Some("FORGED")).is_err());
    roles.push(RegistrationRoleChoice {
        role_code: "R_TWO".into(),
        role_name: "登记复核岗".into(),
    });
    assert!(select_registration_role(&roles, None).is_err());
    assert_eq!(
        select_registration_role(&roles, Some("R_TWO")).unwrap(),
        "R_TWO"
    );
    assert!(select_registration_role(&[], None).is_err());
}

#[test]
fn registration_retains_canonical_roster_cid_and_empty_profile_without_local_defaults() {
    use crate::institutions::admins::{retained_registration_admin, InstitutionAdminInput};
    let snapshot = registration_snapshot_fixture();
    let mut stored = snapshot.admins.unwrap()[0].clone();
    stored.family_name = Default::default();
    stored.given_name = Default::default();
    let input: InstitutionAdminInput = serde_json::from_value(
        serde_json::json!({"accountId":format!("0x{}",hex::encode(stored.account_id))}),
    )
    .unwrap();
    let retained = retained_registration_admin(&input, &stored).unwrap();
    assert_eq!(retained.encode(), stored.encode());
    assert!(!retained.cid_number.is_empty());
    assert!(retained.family_name.is_empty() && retained.given_name.is_empty());
    let mut explicit = input;
    explicit.family_name = Some("张".into());
    let updated = retained_registration_admin(&explicit, &stored).unwrap();
    assert_eq!(updated.family_name.as_slice(), "张".as_bytes());
    assert_eq!(updated.account_id, stored.account_id);
    assert_eq!(updated.cid_number, stored.cid_number);
}

#[test]
fn p1_storage_subject_requires_exact_namespace_and_canonical_key() {
    let cid=b"GZ018-SFGYR-201206100-2026";
    let role=b"LR".to_vec();
    let mut key=sp_crypto_hashing::twox_128(b"PublicManage").to_vec();
    key.extend(sp_crypto_hashing::twox_128(b"InstitutionRoles"));
    let c=cid.to_vec().encode();key.extend(sp_crypto_hashing::blake2_128(&c));key.extend(c);
    let r=role.encode();key.extend(sp_crypto_hashing::blake2_128(&r));key.extend(r);
    assert_eq!(role_code_from_storage_key(&key,"PublicManage","InstitutionRoles",cid).unwrap(),role);
    assert!(role_code_from_storage_key(&key,"PrivateManage","InstitutionRoles",cid).is_err());
    assert!(role_code_from_storage_key(&key,"PublicManage","InstitutionRoleAssignments",cid).is_err());
    let mut bad=key.clone();bad[32]^=1;
    assert!(role_code_from_storage_key(&bad,"PublicManage","InstitutionRoles",cid).is_err());
    let mut bad=key;bad.push(0);
    assert!(role_code_from_storage_key(&bad,"PublicManage","InstitutionRoles",cid).is_err());
}
