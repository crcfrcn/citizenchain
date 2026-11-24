//! 个人多签引擎的冻结钱包、全员生命周期、1..N普通阈值及互斥回归。
use super::*;

#[test]
fn pending_personal_proposal_uses_supplied_admin_snapshot_and_all_admin_threshold() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_pending_personal_proposal_via_engine(
            pending_personal_admin(0),
            pending_personal_account(),
        );

        assert_eq!(InternalThresholdSnapshot::<Test>::get(proposal_id), Some(2));
        assert!(VotingEngine::is_admin_in_snapshot(
            proposal_id,
            ProposalSubject::PersonalAccount(pending_personal_account()),
            &pending_personal_admin(0)
        ));

        assert_ok!(cast_internal_vote_via_extrinsic(
            pending_personal_admin(1),
            proposal_id,
            true
        ));
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal should exist")
                .status,
            STATUS_PASSED
        );
    });
}

#[test]
fn pending_personal_zero_threshold_is_rejected() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            <InternalVote as InternalVoteEngine<AccountId32>>::create_personal_account_create_proposal_with_data(
                pending_personal_admin(0),
                pending_personal_account(),
                sp_std::vec![pending_personal_admin(0), pending_personal_admin(1)],
                0,
                b"test",
                b"payload".to_vec(),
            ),
            Error::<Test>::InvalidDynamicThreshold
        );
    });
}

#[test]
fn pending_personal_snapshot_rejects_duplicate_admins() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            <InternalVote as InternalVoteEngine<AccountId32>>::create_personal_account_create_proposal_with_data(
                pending_personal_admin(0),
                pending_personal_account(),
                sp_std::vec![pending_personal_admin(0), pending_personal_admin(0)],
                2,
                b"test",
                b"payload".to_vec(),
            ),
            engingcore::Error::<Test>::InvalidInstitution
        );
    });
}

#[test]
fn personal_close_proposal_requires_all_snapshot_admins() {
    new_test_ext().execute_with(|| {
        let proposal_id = <InternalVote as InternalVoteEngine<AccountId32>>::create_personal_lifecycle_proposal_with_data(
            personal_admin(0),
            personal_account_id(),
            b"close",
            b"payload".to_vec(),
        )
        .expect("personal close proposal should be created");
        assert_eq!(InternalThresholdSnapshot::<Test>::get(proposal_id), Some(3));
    });
}

#[test]
fn personal_threshold_must_not_exceed_snapshot_size() {
    new_test_ext().execute_with(|| {
        set_personal_threshold(4);
        assert_noop!(
            <InternalVote as InternalVoteEngine<AccountId32>>::create_personal_proposal_with_data(
                personal_admin(0),
                personal_account_id(),
                b"test",
                b"payload".to_vec(),
            ),
            Error::<Test>::InvalidDynamicThreshold
        );
    });
}

#[test]
fn personal_admin_set_mutation_uses_valid_current_snapshot_threshold() {
    new_test_ext().execute_with(|| {
        set_personal_threshold(4);
        assert_noop!(
            <InternalVote as InternalVoteEngine<AccountId32>>::create_personal_admin_change_proposal_with_data(
                personal_admin(0),
                personal_account_id(),
                3,
                2,
                b"test",
                b"payload".to_vec(),
            ),
            Error::<Test>::InvalidDynamicThreshold
        );
    });
}

#[test]
fn personal_proposal_snapshots_dynamic_threshold() {
    new_test_ext().execute_with(|| {
        set_personal_threshold(3);
        let proposal_id =
            create_personal_proposal_via_engine(personal_admin(0), personal_account_id());

        assert_eq!(InternalThresholdSnapshot::<Test>::get(proposal_id), Some(3));
        set_personal_threshold(2);
        assert_ok!(cast_internal_vote_via_extrinsic(
            personal_admin(1),
            proposal_id,
            true
        ));
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal exists")
                .status,
            STATUS_VOTING
        );
        assert_ok!(cast_internal_vote_via_extrinsic(
            personal_admin(2),
            proposal_id,
            true
        ));
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal exists")
                .status,
            STATUS_PASSED
        );
    });
}

#[test]
fn admin_set_mutation_mutex_blocks_same_subject_regular_proposal() {
    new_test_ext().execute_with(|| {
        let proposal_id =
            create_admin_set_mutation_proposal_via_engine(personal_admin(0), personal_account_id());
        let state = personal_mutex_for(personal_account_id()).expect("mutex should exist");
        assert_eq!(state.admin_set_mutation_proposal, Some(proposal_id));

        assert_noop!(
            <InternalVote as InternalVoteEngine<AccountId32>>::create_personal_proposal_with_data(
                personal_admin(1),
                personal_account_id(),
                b"test",
                b"payload".to_vec(),
            ),
            engingcore::Error::<Test>::AdminSetMutationProposalActive
        );
    });
}

#[test]
fn regular_mutex_blocks_same_subject_admin_set_mutation() {
    new_test_ext().execute_with(|| {
        let proposal_id =
            create_personal_proposal_via_engine(personal_admin(0), personal_account_id());
        let state = personal_mutex_for(personal_account_id()).expect("mutex should exist");
        assert_eq!(state.regular_active_count, 1);
        assert_eq!(state.admin_set_mutation_proposal, None);

        assert_noop!(
            <InternalVote as InternalVoteEngine<AccountId32>>::create_personal_admin_change_proposal_with_data(
                personal_admin(1),
                personal_account_id(),
                3,
                2,
                b"test",
                b"payload".to_vec(),
            ),
            engingcore::Error::<Test>::RegularInternalProposalActive
        );

        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal should exist")
                .status,
            STATUS_VOTING
        );
    });
}

#[test]
fn admin_set_mutation_passed_status_keeps_mutex_until_terminal_status() {
    new_test_ext().execute_with(|| {
        let proposal_id =
            create_admin_set_mutation_proposal_via_engine(personal_admin(0), personal_account_id());

        assert_ok!(VotingEngine::set_status_and_emit(
            proposal_id,
            STATUS_PASSED
        ));
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal should exist")
                .status,
            STATUS_PASSED
        );
        assert!(personal_mutex_for(personal_account_id()).is_some());
        assert_noop!(
            <InternalVote as InternalVoteEngine<AccountId32>>::create_personal_proposal_with_data(
                personal_admin(1),
                personal_account_id(),
                b"test",
                b"payload".to_vec(),
            ),
            engingcore::Error::<Test>::AdminSetMutationProposalActive
        );

        assert_ok!(VotingEngine::set_status_and_emit(
            proposal_id,
            STATUS_EXECUTION_FAILED
        ));
        assert!(personal_mutex_for(personal_account_id()).is_none());
    });
}

#[test]
fn personal_range_is_independent_from_institution_majority() {
    new_test_ext().execute_with(|| {
        for count in [2u32, 3, 4, 64] {
            for threshold in [1, count / 2, count] {
                assert_ok!(
                    <InternalVote as InternalVoteEngine<AccountId32>>::validate_personal_threshold(
                        count, threshold
                    )
                );
            }
            for threshold in [0, count + 1] {
                assert!(
                    <InternalVote as InternalVoteEngine<AccountId32>>::validate_personal_threshold(
                        count, threshold
                    )
                    .is_err()
                );
            }
        }
        // 机构仍要求严格多数，不能借用个人1/N规则。
        assert!(InternalVote::ensure_dynamic_threshold(4, 2).is_err());
        assert_ok!(InternalVote::ensure_dynamic_threshold(4, 3));
        assert!(
            <InternalVote as InternalVoteEngine<AccountId32>>::validate_personal_threshold(65, 1)
                .is_err()
        );
    });
}

#[test]
fn ordinary_one_of_three_binds_data_before_auto_approval() {
    new_test_ext().execute_with(|| {
        set_personal_threshold(1);
        let id =
            <InternalVote as InternalVoteEngine<AccountId32>>::create_personal_proposal_with_data(
                personal_admin(0),
                personal_account_id(),
                b"test",
                b"one-of-three".to_vec(),
            )
            .unwrap();
        assert_eq!(VotingEngine::proposals(id).unwrap().status, STATUS_PASSED);
        assert_eq!(InternalThresholdSnapshot::<Test>::get(id), Some(1));
        assert_eq!(InternalTallies::<Test>::get(id).yes, 1);
        assert!(VotingEngine::is_proposal_owner(id, b"test"));
        assert_eq!(
            VotingEngine::get_proposal_data(id).unwrap(),
            b"one-of-three".to_vec()
        );
        assert!(engingcore::PendingProposalExecutions::<Test>::contains_key(
            id
        ));
    });
}

#[test]
fn create_and_close_keep_all_members_even_when_ordinary_threshold_is_one() {
    for count in [2u8, 3, 4, 64] {
        new_test_ext().execute_with(|| {
            let members=(0..count).map(|i| AccountId32::new([i + 1;32])).collect::<Vec<_>>();
            let id=<InternalVote as InternalVoteEngine<AccountId32>>::create_personal_account_create_proposal_with_data(
                members[0].clone(), pending_personal_account(), members.clone(), 1, b"test", b"create".to_vec()).unwrap();
            assert_eq!(InternalThresholdSnapshot::<Test>::get(id), Some(u32::from(count)));
            assert_eq!(VotingEngine::proposals(id).unwrap().status, STATUS_VOTING);
            for (index,member) in members.iter().enumerate().skip(1) {
                assert_ok!(InternalVote::cast(RuntimeOrigin::signed(member.clone()),id,InternalVoteTicketClaim::Personal,true));
                assert_eq!(VotingEngine::proposals(id).unwrap().status, if index + 1 == usize::from(count) {STATUS_PASSED}else{STATUS_VOTING});
            }
        });
    }
    new_test_ext().execute_with(|| {
        set_personal_threshold(1);
        let id=<InternalVote as InternalVoteEngine<AccountId32>>::create_personal_lifecycle_proposal_with_data(
            personal_admin(0),personal_account_id(),b"close",b"close".to_vec()).unwrap();
        assert_eq!(InternalThresholdSnapshot::<Test>::get(id),Some(3));
        assert_eq!(VotingEngine::proposals(id).unwrap().status,STATUS_VOTING);
        assert_ok!(InternalVote::cast(RuntimeOrigin::signed(personal_admin(1)),id,InternalVoteTicketClaim::Personal,true));
        assert_eq!(VotingEngine::proposals(id).unwrap().status,STATUS_VOTING);
        assert_ok!(InternalVote::cast(RuntimeOrigin::signed(personal_admin(2)),id,InternalVoteTicketClaim::Personal,true));
        assert_eq!(VotingEngine::proposals(id).unwrap().status,STATUS_PASSED);
    });
}

#[test]
fn personal_vote_rejects_institution_claim_duplicate_and_wrong_subject() {
    new_test_ext().execute_with(|| {
        let id = create_personal_proposal_via_engine(personal_admin(0), personal_account_id());
        assert_noop!(
            InternalVote::cast(
                RuntimeOrigin::signed(personal_admin(1)),
                id,
                InternalVoteTicketClaim::InstitutionRole(b"MEMBER".to_vec().try_into().unwrap()),
                true
            ),
            engingcore::Error::<Test>::NoPermission
        );
        assert_noop!(
            InternalVote::cast(
                RuntimeOrigin::signed(personal_admin(0)),
                id,
                InternalVoteTicketClaim::Personal,
                false
            ),
            engingcore::Error::<Test>::AlreadyVoted
        );
        assert_eq!(InternalTallies::<Test>::get(id).yes, 1);
        Proposals::<Test>::mutate(id, |p| p.as_mut().unwrap().internal_code = Some(NRC));
        assert_noop!(
            InternalVote::cast(
                RuntimeOrigin::signed(personal_admin(1)),
                id,
                InternalVoteTicketClaim::Personal,
                true
            ),
            engingcore::Error::<Test>::NoPermission
        );
    });
}
