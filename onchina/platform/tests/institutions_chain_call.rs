//! 冷签生产端编码须能由实际Runtime完整解码，禁止残留管理员钱包调用。
use super::*;
use codec::{DecodeAll, Encode};
fn action() -> entitybase::InstitutionGovernanceAction {
    entitybase::InstitutionGovernanceAction {
        role_mutations: vec![],
        assignment_changes: vec![entitybase::InstitutionRoleAssignmentChange {
            role_code: b"R1".to_vec(),
            assignments: vec![entitybase::InstitutionAssignmentTarget {
                person_cid: crate::testing::person_cid().into_bytes(),
                term_start: 0,
                term_end: 0,
                assignment_source: entitybase::InstitutionAssignmentSource::InstitutionGovernance,
                assignment_source_ref: vec![],
                assignment_status: entitybase::InstitutionAssignmentStatus::Active,
            }],
        }],
        legal_representative_change: None,
    }
}
#[test]
fn both_entity_governance_calls_decode_in_actual_runtime_without_an_admin_variant() {
    for is_private in [false, true] {
        let args = ProposeInstitutionGovernanceArgs {
            is_private,
            cid_number: b"fixture".to_vec(),
            governance_action: action().encode(),
            institution_code: *b"CREG",
            actor_cid_number: b"fixture".to_vec(),
            proposer_role_code: b"R1".to_vec(),
        };
        let call = encode_propose_institution_governance(&args);
        assert_eq!(call.call_data[..2], [if is_private { 31 } else { 30 }, 8]);
        let decoded = runtime::RuntimeCall::decode_all(&mut call.call_data.as_slice()).unwrap();
        assert_eq!(decoded.encode(), call.call_data);
    }
}
#[test]
fn registration_action_is_a_distinct_two_vector_structure_in_runtime() {
    for is_private in [false, true] {
        let args = RegisterInstitutionRolesArgs {
            is_private,
            cid_number: b"target".to_vec(),
            action: entitybase::InstitutionRoleRegistration {
                role_mutations: vec![],
                assignment_changes: action().assignment_changes,
            },
            institution_code: *b"CREG",
            actor_cid_number: b"actor".to_vec(),
            actor_role_code: b"R1".to_vec(),
            expected_state_hash: [7; 32],
            initial_threshold: Some(2),
        };
        let call = encode_register_institution_roles(&args);
        let decoded = runtime::RuntimeCall::decode_all(&mut call.call_data.as_slice()).unwrap();
        assert_eq!(decoded.encode(), call.call_data);
        assert_eq!(
            call.action,
            chain_action_code(if is_private { 31 } else { 30 }, 9)
        );
        let mut broken = call.call_data;
        broken.push(0);
        assert!(runtime::RuntimeCall::decode_all(&mut broken.as_slice()).is_err());
    }
}
#[test]
fn create_role_carries_capacity_before_permission_and_person_vectors() {
    let mutation = entitybase::InstitutionRoleMutation::Create {
        role_name: "岗位".as_bytes().to_vec(),
        term_required: false,
        seats: 19,
        permissions: vec![],
        assignments: vec![],
    };
    let bytes = mutation.encode();
    let decoded = entitybase::InstitutionRoleMutation::decode_all(&mut bytes.as_slice()).unwrap();
    assert_eq!(decoded, mutation);
}
#[test]
fn account_lifecycle_calls_preserve_original_indices_and_values() {
    let add = encode_propose_add_institution_account(&ProposeAddInstitutionAccountArgs {
        is_private: false,
        cid_number: b"fixture".to_vec(),
        account_names: vec!["账户".as_bytes().to_vec()],
        proposer_role_code: b"R1".to_vec(),
    });
    assert_eq!(add.call_data[..2], [30, 7]);
    let decoded = runtime::RuntimeCall::decode_all(&mut add.call_data.as_slice()).unwrap();
    assert_eq!(decoded.encode(), add.call_data);
    let close = encode_propose_close_institution(&ProposeCloseInstitutionArgs {
        is_private: true,
        actor_cid_number: b"fixture".to_vec(),
        proposer_role_code: b"R1".to_vec(),
        institution_account_id: [1; 32],
        beneficiary_account_id: [2; 32],
    });
    assert_eq!(close.call_data[..2], [31, 1]);
    assert!(runtime::RuntimeCall::decode_all(&mut close.call_data.as_slice()).is_ok());
}
