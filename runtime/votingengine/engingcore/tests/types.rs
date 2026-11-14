use codec::Encode;
use frame_support::{traits::ConstU32, BoundedVec};
// 投票计划有效性及固定编码测试。

use crate::types::*;
use entitybase::RoleSubject;

type MaxModuleTagLen = ConstU32<32>;
type TestVotePlan = VotePlan<[u8; 32], MaxModuleTagLen>;

fn bounded(bytes: &[u8]) -> BoundedVec<u8, MaxModuleTagLen> {
    bytes.to_vec().try_into().expect("测试标签长度合法")
}

fn institution(
    cid_number: &[u8],
    role_code: &[u8],
) -> AuthorizationSubject<CidNumber, RoleCode, [u8; 32]> {
    AuthorizationSubject::Institution(RoleSubject {
        cid_number: cid_number.to_vec().try_into().expect("测试 CID 长度合法"),
        role_code: role_code.to_vec().try_into().expect("测试岗位码长度合法"),
    })
}

fn valid_plan() -> TestVotePlan {
    let proposer = institution(b"LN001-NRC0G-944805165-2026", b"COMMITTEE_MEMBER");
    let voters = vec![
        proposer.clone(),
        institution(b"ZS001-PRC0E-016974075-2026", b"COMMITTEE_MEMBER"),
        institution(b"ZS001-PRB08-233384677-2026", b"DIRECTOR"),
    ];
    TestVotePlan::try_new(
        BusinessActionId {
            module_tag: bounded(b"res-iss"),
            action_code: 0,
        },
        bounded(b"res-iss"),
        proposer,
        voters,
        VotingEngineKind::Joint,
        [0xabu8; 32],
    )
    .expect("测试投票计划必须合法")
}

#[test]
fn voting_engine_discriminants_are_stable() {
    assert_eq!(VotingEngineKind::Internal.encode(), vec![0]);
    assert_eq!(VotingEngineKind::Joint.encode(), vec![1]);
    assert_eq!(VotingEngineKind::Election.encode(), vec![2]);
    assert_eq!(VotingEngineKind::Legislation.encode(), vec![3]);
}

#[test]
fn vote_plan_field_order_is_stable() {
    let plan = valid_plan();
    assert_eq!(
        plan.encode(),
        (
            plan.business_action_id.clone(),
            plan.proposal_owner.clone(),
            plan.proposer_subject.clone(),
            plan.voter_subjects.clone(),
            plan.voting_engine,
            plan.business_object_hash,
        )
            .encode()
    );
}

#[test]
fn vote_plan_rejects_invalid_subject_combinations() {
    let proposer = institution(b"LN001-NRC0G-944805165-2026", b"COMMITTEE_MEMBER");
    let action = BusinessActionId {
        module_tag: bounded(b"res-iss"),
        action_code: 0,
    };
    let create = |owner: &[u8], proposer_subject, voter_subjects| {
        TestVotePlan::try_new(
            action.clone(),
            bounded(owner),
            proposer_subject,
            voter_subjects,
            VotingEngineKind::Joint,
            [0xabu8; 32],
        )
    };

    assert!(matches!(
        create(b"wrong", proposer.clone(), vec![proposer.clone()]),
        Err(VotePlanValidationError::ProposalOwnerMismatch)
    ));
    assert!(matches!(
        create(b"res-iss", proposer.clone(), Vec::new()),
        Err(VotePlanValidationError::VoterSubjectsEmpty)
    ));
    assert!(matches!(
        create(
            b"res-iss",
            proposer.clone(),
            vec![proposer.clone(), proposer.clone()],
        ),
        Err(VotePlanValidationError::DuplicateVoterSubject)
    ));
    assert!(matches!(
        create(
            b"res-iss",
            proposer,
            vec![AuthorizationSubject::PersonalMultisig([7u8; 32])],
        ),
        Err(VotePlanValidationError::AuthorizationSubjectMismatch)
    ));
}

#[test]
fn personal_multisig_plan_requires_the_same_single_account() {
    let personal = AuthorizationSubject::PersonalMultisig([7u8; 32]);
    let result = TestVotePlan::try_new(
        BusinessActionId {
            module_tag: bounded(b"multisig"),
            action_code: 0,
        },
        bounded(b"multisig"),
        personal.clone(),
        vec![personal],
        VotingEngineKind::Internal,
        [0xcdu8; 32],
    );
    assert!(result.is_ok());

    let mismatched = TestVotePlan::try_new(
        BusinessActionId {
            module_tag: bounded(b"multisig"),
            action_code: 0,
        },
        bounded(b"multisig"),
        AuthorizationSubject::PersonalMultisig([7u8; 32]),
        vec![AuthorizationSubject::PersonalMultisig([8u8; 32])],
        VotingEngineKind::Internal,
        [0xcdu8; 32],
    );
    assert!(matches!(
        mismatched,
        Err(VotePlanValidationError::AuthorizationSubjectMismatch)
    ));
}
