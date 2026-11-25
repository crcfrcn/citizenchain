//! 以黄金布局验证机构记录的SCALE字段顺序。

use crate::*;
use codec::Encode;

/// 机构记录的声明序就是 SCALE 存储契约，NodeGuard 按同一顺序完整解码。
#[test]
fn institution_info_field_order_matches_node_guard() {
    let value = InstitutionInfo {
        cid_full_name: b"full".to_vec(),
        cid_short_name: b"short".to_vec(),
        town_code: b"001".to_vec(),
        legal_representative: Some(LegalRepresentative {
            family_name: b"family".to_vec(),
            given_name: b"given".to_vec(),
            person_cid: b"citizen-cid".to_vec(),
        }),
        institution_code: *b"NRCG",
        created_at: 7u32,
    };
    assert_eq!(
        value.encode(),
        (
            b"full".to_vec(),
            b"short".to_vec(),
            b"001".to_vec(),
            Some((
                b"family".to_vec(),
                b"given".to_vec(),
                b"citizen-cid".to_vec(),
            )),
            *b"NRCG",
            7u32,
        )
            .encode()
    );
}

#[test]
fn governance_and_policy_scale_coordinates_have_literal_goldens() {
    use codec::DecodeAll;
    let rename = InstitutionGovernanceAction {
        role_mutations: vec![InstitutionRoleMutation::Rename {
            role_code: b"R".to_vec(),
            role_name: b"N".to_vec(),
        }],
        assignment_changes: vec![],
        legal_representative_change: None,
    };
    assert_eq!(rename.encode(), vec![4, 1, 4, 82, 4, 78, 0, 0]);
    let create = InstitutionRoleMutation::Create {
        role_name: b"N".to_vec(),
        term_required: false,
        seats: 2,
        permissions: vec![RolePermissionSpec {
            business_action_id: BusinessActionId {
                module_tag: b"m".to_vec(),
                action_code: 3,
            },
            operation: RolePermissionOperation::Propose,
        }],
        assignments: vec![],
    };
    assert_eq!(
        create.encode(),
        vec![0, 4, 78, 0, 2, 0, 0, 0, 4, 4, 109, 3, 0, 0, 0, 0, 0]
    );
    let configure = InstitutionRoleMutation::Configure {
        role_code: b"R".to_vec(),
        term_required: true,
        seats: 3,
        role_status: InstitutionRoleStatus::Inactive,
    };
    assert_eq!(configure.encode(), vec![3, 4, 82, 1, 3, 0, 0, 0, 1]);
    let policy = InstitutionPolicyAction {
        permission_changes: vec![RolePermissionChange {
            role_code: b"R".to_vec(),
            permissions: vec![RolePermissionSpec {
                business_action_id: BusinessActionId {
                    module_tag: b"m".to_vec(),
                    action_code: 3,
                },
                operation: RolePermissionOperation::Vote,
            }],
        }],
        threshold: Some(2),
    };
    let golden = vec![4, 4, 82, 4, 4, 109, 3, 0, 0, 0, 1, 1, 2, 0, 0, 0];
    assert_eq!(policy.encode(), golden);
    assert_eq!(
        InstitutionPolicyAction::decode_all(&mut golden.as_slice()).unwrap(),
        policy
    );
    let mut trailing = golden;
    trailing.push(0);
    assert!(InstitutionPolicyAction::decode_all(&mut trailing.as_slice()).is_err());
    assert!(InstitutionRoleMutation::decode_all(&mut [4u8].as_slice()).is_err());
}
