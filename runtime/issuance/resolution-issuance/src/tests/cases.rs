#![cfg(test)]

use super::*;

#[test]
fn only_authorized_admin_can_propose() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            ResolutionIssuance::propose_issuance(
                RuntimeOrigin::signed(AccountId32::new([2u8; 32])),
                actor_cid_number(),
                committee_role_code(),
                reason_ok(),
                4300,
                allocations_ok(4300)
            ),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn administrator_without_committee_role_cannot_propose() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            ResolutionIssuance::propose_issuance(
                RuntimeOrigin::signed(AccountId32::new([4u8; 32])),
                actor_cid_number(),
                committee_role_code(),
                reason_ok(),
                4300,
                allocations_ok(4300)
            ),
            pallet::Error::<Test>::UnauthorizedActorRole
        );
    });
}

#[test]
fn authorized_admin_cannot_replace_committee_role_with_another_role() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            ResolutionIssuance::propose_issuance(
                RuntimeOrigin::signed(AccountId32::new([1u8; 32])),
                actor_cid_number(),
                director_role_code(),
                reason_ok(),
                4300,
                allocations_ok(4300)
            ),
            pallet::Error::<Test>::UnauthorizedActorRole
        );
    });
}

#[test]
fn authorized_admin_cannot_supply_invalid_actor_cid() {
    new_test_ext().execute_with(|| {
        let invalid_actor_cid: votingengine::types::CidNumber = b"invalid"
            .to_vec()
            .try_into()
            .expect("invalid CID fixture should fit bound");
        assert_noop!(
            ResolutionIssuance::propose_issuance(
                RuntimeOrigin::signed(AccountId32::new([1u8; 32])),
                invalid_actor_cid,
                committee_role_code(),
                reason_ok(),
                4300,
                allocations_ok(4300)
            ),
            pallet::Error::<Test>::InvalidActorCid
        );
    });
}

#[test]
fn reject_invalid_allocation_count() {
    new_test_ext().execute_with(|| {
        let one = vec![crate::proposal::RecipientAmount {
            recipient_account_id: reserve_council_accounts()[0].clone(),
            amount: 1000,
        }];
        let alloc: pallet::AllocationOf<Test> = one.try_into().expect("should fit");
        assert_noop!(
            ResolutionIssuance::propose_issuance(
                RuntimeOrigin::signed(AccountId32::new([1u8; 32])),
                actor_cid_number(),
                committee_role_code(),
                reason_ok(),
                1000,
                alloc
            ),
            pallet::Error::<Test>::InvalidAllocationCount
        );
    });
}

#[test]
fn approved_callback_executes_issuance() {
    new_test_ext().execute_with(|| {
        assert_ok!(ResolutionIssuance::propose_issuance(
            RuntimeOrigin::signed(AccountId32::new([1u8; 32])),
            actor_cid_number(),
            committee_role_code(),
            reason_ok(),
            4300,
            allocations_ok(4300)
        ));
        let plan = votingengine::ProposalVotePlans::<Test>::get(100)
            .expect("resolution issuance must bind vote plan");
        assert_eq!(
            plan.business_action_id.action_code,
            entity_primitives::business_action::ACTION_RESOLUTION_ISSUANCE
        );
        assert!(matches!(
            &plan.proposer_subject,
            entity_primitives::AuthorizationSubject::Institution(role)
                if role.cid_number == actor_cid_number()
                    && role.role_code == committee_role_code()
        ));
        let proposal_data = ResolutionIssuance::load_proposal_data(100)
            .expect("resolution issuance proposal data must decode");
        assert_eq!(proposal_data.actor_cid_number, actor_cid_number());
        assert_eq!(proposal_data.proposer_role_code, committee_role_code());
        assert_eq!(plan.voter_subjects.len(), 87);
        assert_eq!(
            plan.voter_subjects
                .iter()
                .filter(|subject| matches!(
                    subject,
                    entity_primitives::AuthorizationSubject::Institution(role)
                        if role.role_code.as_slice()
                            == primitives::governance_skeleton::ROLE_CODE_DIRECTOR
                ))
                .count(),
            43
        );

        insert_engine_proposal(100);
        assert_ok!(call_joint_callback(100, true));
        assert_eq!(
            votingengine::pallet::Proposals::<Test>::get(100)
                .expect("engine proposal should exist")
                .status,
            votingengine::STATUS_EXECUTED
        );
        assert_eq!(pallet::VotingProposalCount::<Test>::get(), 0);
        assert!(pallet::Executed::<Test>::get(100).is_some());
        assert!(pallet::EverExecuted::<Test>::contains_key(100));
        assert_eq!(pallet::TotalIssued::<Test>::get(), 4300);
    });
}

#[test]
fn approved_referendum_callback_executes_issuance() {
    new_test_ext().execute_with(|| {
        assert_ok!(ResolutionIssuance::propose_issuance(
            RuntimeOrigin::signed(AccountId32::new([1u8; 32])),
            actor_cid_number(),
            committee_role_code(),
            reason_ok(),
            4300,
            allocations_ok(4300)
        ));

        // 联合投票未直接通过、进入公投后，终态 stage 会保留为 REFERENDUM。
        insert_engine_proposal_with_stage_and_status(
            100,
            votingengine::STAGE_REFERENDUM,
            votingengine::STATUS_PASSED,
        );
        assert_ok!(call_joint_callback(100, true));
        assert_eq!(pallet::TotalIssued::<Test>::get(), 4300);
        assert!(pallet::EverExecuted::<Test>::contains_key(100));
    });
}

#[test]
fn callback_rejects_non_finalizable_engine_status() {
    new_test_ext().execute_with(|| {
        assert_ok!(ResolutionIssuance::propose_issuance(
            RuntimeOrigin::signed(AccountId32::new([1u8; 32])),
            actor_cid_number(),
            committee_role_code(),
            reason_ok(),
            4300,
            allocations_ok(4300)
        ));

        insert_engine_proposal_with_status(100, votingengine::STATUS_VOTING);
        assert_noop!(
            call_joint_callback(100, true),
            pallet::Error::<Test>::ProposalNotFinalizable
        );
        assert_eq!(pallet::VotingProposalCount::<Test>::get(), 1);
        assert!(!pallet::Executed::<Test>::contains_key(100));
        assert_eq!(pallet::TotalIssued::<Test>::get(), 0);
    });
}

#[test]
fn callback_requires_votingengine_scope() {
    new_test_ext().execute_with(|| {
        assert_ok!(ResolutionIssuance::propose_issuance(
            RuntimeOrigin::signed(AccountId32::new([1u8; 32])),
            actor_cid_number(),
            committee_role_code(),
            reason_ok(),
            4300,
            allocations_ok(4300)
        ));

        insert_engine_proposal(100);
        assert_noop!(
            ResolutionIssuance::on_joint_vote_finalized(100, true),
            pallet::Error::<Test>::ProposalNotFinalizable
        );
        assert_eq!(pallet::VotingProposalCount::<Test>::get(), 1);
        assert!(!pallet::Executed::<Test>::contains_key(100));
    });
}

#[test]
fn second_callback_after_executed_is_rejected() {
    new_test_ext().execute_with(|| {
        assert_ok!(ResolutionIssuance::propose_issuance(
            RuntimeOrigin::signed(AccountId32::new([1u8; 32])),
            actor_cid_number(),
            committee_role_code(),
            reason_ok(),
            4300,
            allocations_ok(4300)
        ));

        insert_engine_proposal(100);
        assert_ok!(call_joint_callback(100, true));
        assert_noop!(
            call_joint_callback(100, true),
            pallet::Error::<Test>::ProposalNotFinalizable
        );
        assert_eq!(
            votingengine::pallet::Proposals::<Test>::get(100)
                .expect("engine proposal should exist")
                .status,
            votingengine::STATUS_EXECUTED
        );
        assert_eq!(pallet::VotingProposalCount::<Test>::get(), 0);
        assert_eq!(pallet::TotalIssued::<Test>::get(), 4300);
    });
}

#[test]
fn rejected_callback_does_not_issue() {
    new_test_ext().execute_with(|| {
        assert_ok!(ResolutionIssuance::propose_issuance(
            RuntimeOrigin::signed(AccountId32::new([1u8; 32])),
            actor_cid_number(),
            committee_role_code(),
            reason_ok(),
            4300,
            allocations_ok(4300)
        ));

        insert_engine_proposal_with_status(100, votingengine::STATUS_REJECTED);
        assert_ok!(call_joint_callback(100, false));
        assert_eq!(pallet::VotingProposalCount::<Test>::get(), 0);
        assert!(!pallet::Executed::<Test>::contains_key(100));
        assert_eq!(pallet::TotalIssued::<Test>::get(), 0);
    });
}

#[test]
fn callback_rejects_corrupted_reason_with_reason_too_long() {
    new_test_ext().execute_with(|| {
        assert_ok!(ResolutionIssuance::propose_issuance(
            RuntimeOrigin::signed(AccountId32::new([1u8; 32])),
            actor_cid_number(),
            committee_role_code(),
            reason_ok(),
            4300,
            allocations_ok(4300)
        ));

        overwrite_proposal_data(
            100,
            crate::proposal::IssuanceProposalData {
                actor_cid_number: actor_cid_number(),
                proposer_role_code: committee_role_code(),
                proposer_account_id: AccountId32::new([1u8; 32]),
                reason: vec![b'x'; 129],
                total_amount: 4300,
                allocations: allocations_ok(4300).to_vec(),
            },
        );
        insert_engine_proposal(100);
        assert_noop!(
            call_joint_callback(100, true),
            pallet::Error::<Test>::ReasonTooLong
        );
        assert_eq!(pallet::VotingProposalCount::<Test>::get(), 1);
        assert!(!pallet::Executed::<Test>::contains_key(100));
        assert_eq!(pallet::TotalIssued::<Test>::get(), 0);
    });
}

#[test]
fn issuance_event_comes_from_unified_pallet() {
    new_test_ext().execute_with(|| {
        assert_ok!(ResolutionIssuance::propose_issuance(
            RuntimeOrigin::signed(AccountId32::new([1u8; 32])),
            actor_cid_number(),
            committee_role_code(),
            reason_ok(),
            4300,
            allocations_ok(4300)
        ));
        insert_engine_proposal(100);
        assert_ok!(call_joint_callback(100, true));

        assert!(frame_system::Pallet::<Test>::events().iter().any(|record| {
            matches!(
                &record.event,
                RuntimeEvent::ResolutionIssuance(
                    pallet::Event::<Test>::ResolutionIssuanceExecuted {
                        proposal_id: 100,
                        ..
                    }
                )
            )
        }));
    });
}
