use crate::storage::assignment_is_effective;
use crate::storage::display_source_ref;
use crate::storage::permission_operation_label;
use entitybase::InstitutionAdminAssignment;
use entitybase::InstitutionAssignmentSource;
use entitybase::InstitutionAssignmentStatus;
use entitybase::InstitutionRole;
use entitybase::InstitutionRoleStatus;
use entitybase::RolePermissionOperation;

#[test]
fn source_ref_keeps_text_and_hexes_binary_ids() {
    assert_eq!(display_source_ref(b"registry-1".to_vec()), "registry-1");
    assert_eq!(display_source_ref(vec![1, 0, 0, 0]), "0x01000000");
}

#[test]
fn permission_operation_uses_stable_chinese_labels() {
    assert_eq!(
        permission_operation_label(RolePermissionOperation::Propose),
        "发起提案"
    );
    assert_eq!(
        permission_operation_label(RolePermissionOperation::Vote),
        "参与投票"
    );
}

#[test]
fn assignment_term_window_is_inclusive() {
    let role = InstitutionRole {
        cid_number: b"CID".to_vec(),
        role_code: b"ROLE".to_vec(),
        role_name: b"Role".to_vec(),
        term_required: true,
        role_status: InstitutionRoleStatus::Active,
    };
    let assignment = InstitutionAdminAssignment {
        cid_number: b"CID".to_vec(),
        account_id: [1; 32],
        role_code: b"ROLE".to_vec(),
        term_start: 10,
        term_end: 20,
        assignment_source: InstitutionAssignmentSource::InstitutionGovernance,
        assignment_source_ref: b"proposal".to_vec(),
        assignment_status: InstitutionAssignmentStatus::Active,
    };
    assert!(assignment_is_effective(&role, &assignment, 10));
    assert!(assignment_is_effective(&role, &assignment, 20));
    assert!(!assignment_is_effective(&role, &assignment, 21));
}
