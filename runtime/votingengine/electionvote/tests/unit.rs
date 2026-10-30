//! 原有行为回归完整迁入，复用所属包测试夹具。
use super::*;

#[test]
fn popular_election_uses_population_snapshot_and_generates_result() {
    new_test_ext().execute_with(|| {
        let candidates = vec![candidate(11), candidate(12)];
        let voters = [account(21), account(22), account(23)];
        let proposal_id = create_popular(candidates.clone());

        // 创建后人口增长不能把新账户塞进既有普选；Popular 不保存全量选民表。
        POPULATION_COUNT.with(|count| *count.borrow_mut() = 4);
        assert_noop!(
            ElectionVote::cast_popular_vote(
                RuntimeOrigin::signed(account(24)),
                proposal_id,
                candidates[0].clone()
            ),
            Error::<Test>::VoterNotEligible
        );

        assert_ok!(ElectionVote::cast_popular_vote(
            RuntimeOrigin::signed(voters[0].clone()),
            proposal_id,
            candidates[0].clone()
        ));
        assert_ok!(ElectionVote::cast_popular_vote(
            RuntimeOrigin::signed(voters[1].clone()),
            proposal_id,
            candidates[0].clone()
        ));
        assert_ok!(ElectionVote::cast_popular_vote(
            RuntimeOrigin::signed(voters[2].clone()),
            proposal_id,
            candidates[1].clone()
        ));

        let proposal = votebase::pallet::Proposals::<Test>::get(proposal_id).unwrap();
        assert_eq!(proposal.status, STATUS_PASSED);
        assert_eq!(
            VotingEngine::population_eligible_total_of(proposal_id),
            Some(3)
        );
        let winners = ElectionResults::<Test>::get(proposal_id).unwrap();
        assert_eq!(winners[0].candidate_subject, candidates[0]);
        assert_eq!(winners[0].votes, 2);
        assert_eq!(
            ElectionVote::on_election_vote_finalized(proposal_id, true),
            Ok(ProposalExecutionOutcome::Executed)
        );
    });
}

#[test]
fn mutual_election_uses_role_assignment_snapshot() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_mutual();
        let admins = target_admins();
        for voter in admins.iter() {
            assert_ok!(ElectionVote::cast_mutual_vote(
                RuntimeOrigin::signed(voter.clone()),
                proposal_id,
                TARGET_ROLE.to_vec().try_into().expect("role fits"),
                candidate(11)
            ));
        }

        let meta = ElectionMetaStore::<Test>::get(proposal_id).unwrap();
        assert_eq!(meta.mode, ElectionMode::Mutual);
        assert_eq!(meta.actor_cid_number, institution_cid_number());
        assert_eq!(meta.role_code, elected_role_code());
        assert_eq!(
            votebase::pallet::Proposals::<Test>::get(proposal_id)
                .unwrap()
                .status,
            STATUS_PASSED
        );
    });
}

#[test]
fn creation_rejects_vote_plan_without_mutual_voter_role() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            ElectionVote::do_create_mutual_election(
                organizer_admin(),
                vote_plan(false),
                institution_cid_number(),
                elected_role_code(),
                1,
                10,
                20,
                vec![candidate(11)],
            ),
            Error::<Test>::InvalidVotePlan
        );
    });
}

#[test]
fn popular_creation_rejects_invalid_candidate_subject_and_bad_shape() {
    new_test_ext().execute_with(|| {
        let invalid_subject = votebase::CitizenSubject {
            cid_number: candidate(12).cid_number,
            // 249 有普通公民/投票身份，但没有竞选身份，不能进入候选人快照。
            account_id: account(249),
        };
        assert_noop!(
            ElectionVote::do_create_popular_election(
                organizer_admin(),
                vote_plan(false),
                institution_cid_number(),
                elected_role_code(),
                1,
                10,
                20,
                PopulationScope::Country,
                vec![candidate(11), invalid_subject]
            ),
            Error::<Test>::CandidateSubjectInvalid
        );
        assert_noop!(
            ElectionVote::do_create_popular_election(
                account(9),
                vote_plan(false),
                institution_cid_number(),
                elected_role_code(),
                1,
                10,
                20,
                PopulationScope::Country,
                vec![candidate(11), candidate(12)]
            ),
            Error::<Test>::NotOrganizerAdmin
        );
    });
}

#[test]
fn popular_creation_rejects_population_data_that_is_not_ready() {
    new_test_ext().execute_with(|| {
        POPULATION_READY.with(|ready| *ready.borrow_mut() = false);
        assert_noop!(
            ElectionVote::do_create_popular_election(
                organizer_admin(),
                vote_plan(false),
                institution_cid_number(),
                elected_role_code(),
                1,
                10,
                20,
                PopulationScope::Country,
                vec![candidate(11), candidate(12)]
            ),
            votebase::Error::<Test>::PopulationDataNotReady
        );
        assert_eq!(votebase::Proposals::<Test>::iter().count(), 0);
        assert_eq!(
            votebase::ProposalPopulationSnapshots::<Test>::iter().count(),
            0
        );
    });
}

#[test]
fn cast_rejects_wrong_voter_candidate_stage_and_duplicate_vote() {
    new_test_ext().execute_with(|| {
        let candidates = vec![candidate(11), candidate(12)];
        let voters = [account(21), account(22), account(23)];
        let proposal_id = create_popular(candidates.clone());
        assert_noop!(
            ElectionVote::cast_popular_vote(
                RuntimeOrigin::signed(account(99)),
                proposal_id,
                candidates[0].clone()
            ),
            Error::<Test>::VoterNotEligible
        );
        assert_noop!(
            ElectionVote::cast_popular_vote(
                RuntimeOrigin::signed(voters[0].clone()),
                proposal_id,
                candidate(99)
            ),
            Error::<Test>::CandidateNotInSnapshot
        );
        assert_noop!(
            ElectionVote::cast_mutual_vote(
                RuntimeOrigin::signed(voters[0].clone()),
                proposal_id,
                TARGET_ROLE.to_vec().try_into().expect("role fits"),
                candidates[0].clone()
            ),
            votebase::Error::<Test>::InvalidProposalStage
        );
        assert_ok!(ElectionVote::cast_popular_vote(
            RuntimeOrigin::signed(voters[0].clone()),
            proposal_id,
            candidates[0].clone()
        ));
        assert_noop!(
            ElectionVote::cast_popular_vote(
                RuntimeOrigin::signed(voters[0].clone()),
                proposal_id,
                candidates[0].clone()
            ),
            votebase::Error::<Test>::AlreadyVoted
        );
    });
}

#[test]
fn same_citizen_cid_cannot_vote_twice_after_account_id_replacement() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_popular(vec![candidate(11), candidate(12)]);
        let original_account_id = account(21);
        let replacement_account_id = account(29);
        let voter_subject = test_citizen_subject(&original_account_id);

        assert_ok!(ElectionVote::cast_popular_vote(
            RuntimeOrigin::signed(original_account_id),
            proposal_id,
            candidate(11),
        ));
        let ticket = PopularElectionVotesByCid::<Test>::get(proposal_id, &voter_subject.cid_number)
            .expect("complete popular election ticket is stored");
        assert_eq!(ticket.voter_subject, voter_subject);
        assert_eq!(ticket.candidate_subject, candidate(11));

        assert_noop!(
            ElectionVote::cast_popular_vote(
                RuntimeOrigin::signed(replacement_account_id),
                proposal_id,
                candidate(11),
            ),
            votebase::Error::<Test>::AlreadyVoted
        );
    });
}

#[test]
fn candidate_snapshot_rejects_duplicate_permanent_cid() {
    new_test_ext().execute_with(|| {
        let duplicate_cid_subject = votebase::CitizenSubject {
            cid_number: candidate(11).cid_number,
            account_id: account(12),
        };
        assert_noop!(
            ElectionVote::do_create_popular_election(
                organizer_admin(),
                vote_plan(false),
                institution_cid_number(),
                elected_role_code(),
                1,
                10,
                20,
                PopulationScope::Country,
                vec![candidate(11), duplicate_cid_subject],
            ),
            Error::<Test>::DuplicateCandidateCid
        );
    });
}

#[test]
fn same_admin_can_vote_once_for_each_frozen_institution_role() {
    new_test_ext().execute_with(|| {
        let proposal_id = ElectionVote::do_create_mutual_election(
            organizer_admin(),
            multi_role_vote_plan(),
            institution_cid_number(),
            elected_role_code(),
            1,
            10,
            20,
            vec![candidate(11), candidate(12)],
        )
        .expect("multi-role mutual election should be created");
        let voter = target_admins()[0].clone();

        assert_ok!(ElectionVote::cast_mutual_vote(
            RuntimeOrigin::signed(voter.clone()),
            proposal_id,
            TARGET_ROLE.to_vec().try_into().expect("role fits"),
            candidate(11),
        ));
        assert_ok!(ElectionVote::cast_mutual_vote(
            RuntimeOrigin::signed(voter),
            proposal_id,
            SECOND_VOTER_ROLE.to_vec().try_into().expect("role fits"),
            candidate(11),
        ));

        assert_eq!(
            MutualElectionVotesByTicket::<Test>::iter_prefix(proposal_id).count(),
            2
        );
        assert_eq!(
            ElectionCandidateTallies::<Test>::get(proposal_id, candidate(11).cid_number),
            2
        );
    });
}

#[test]
fn timeout_is_rejected_before_expiry_then_finalizes_no_vote_election() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_popular(vec![candidate(11), candidate(12)]);
        let proposal = votebase::pallet::Proposals::<Test>::get(proposal_id).unwrap();
        assert_noop!(
            ElectionVote::finalize_election_popular_timeout(&proposal, proposal_id),
            votebase::Error::<Test>::VoteNotExpired
        );
        System::set_block_number(proposal.end + 1);
        assert_ok!(VotingEngine::finalize_proposal(
            RuntimeOrigin::signed(account(99)),
            proposal_id
        ));
        assert_eq!(
            votebase::pallet::Proposals::<Test>::get(proposal_id)
                .unwrap()
                .status,
            STATUS_REJECTED
        );
    });
}

#[test]
fn election_cleanup_removes_all_track_storage() {
    new_test_ext().execute_with(|| {
        let candidates = vec![candidate(11), candidate(12)];
        let voters = [account(21), account(22), account(23)];
        let proposal_id = create_popular(candidates.clone());
        assert_ok!(ElectionVote::cast_popular_vote(
            RuntimeOrigin::signed(voters[0].clone()),
            proposal_id,
            candidates[0].clone()
        ));

        assert!(
            <ElectionVote as ProposalTrackHandler<u64, AccountId32>>::cleanup_chunk(
                votebase::PROPOSAL_KIND_ELECTION,
                proposal_id,
                1,
            )
            .is_some()
        );
        let _ = <ElectionVote as ProposalTrackHandler<u64, AccountId32>>::cleanup_chunk(
            votebase::PROPOSAL_KIND_ELECTION,
            proposal_id,
            8,
        );
        assert_eq!(
            <ElectionVote as ProposalTrackHandler<u64, AccountId32>>::cleanup_terminal(
                votebase::PROPOSAL_KIND_ELECTION,
                proposal_id,
            ),
            Some(())
        );

        assert!(!ElectionMetaStore::<Test>::contains_key(proposal_id));
        assert!(!ElectionCandidates::<Test>::contains_key(proposal_id));
        assert!(!ElectionResults::<Test>::contains_key(proposal_id));
        assert!(PopularElectionVotesByCid::<Test>::iter_prefix(proposal_id)
            .next()
            .is_none());
        assert!(
            MutualElectionVotesByTicket::<Test>::iter_prefix(proposal_id)
                .next()
                .is_none()
        );
        assert!(ElectionCandidateTallies::<Test>::iter_prefix(proposal_id)
            .next()
            .is_none());
        assert!(!ElectionTallyStore::<Test>::contains_key(proposal_id));
    });
}

#[test]
fn result_callback_fails_closed_for_incomplete_or_foreign_result() {
    new_test_ext().execute_with(|| {
        assert_eq!(
            ElectionVote::on_election_vote_finalized(999, true),
            Ok(ProposalExecutionOutcome::Ignored)
        );
        let proposal_id = create_popular(vec![candidate(11), candidate(12)]);
        assert_eq!(
            ElectionVote::on_election_vote_finalized(proposal_id, true),
            Ok(ProposalExecutionOutcome::FatalFailed)
        );
        assert_eq!(
            ElectionVote::on_election_vote_finalized(proposal_id, false),
            Ok(ProposalExecutionOutcome::Executed)
        );
    });
}
