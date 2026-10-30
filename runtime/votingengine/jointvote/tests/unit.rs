//! 原有行为回归完整迁入，复用所属包测试夹具。
use super::*;

#[test]
fn joint_proposal_binds_role_plan_and_never_writes_admin_snapshot() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal();
        let plan = votebase::ProposalVotePlans::<Test>::get(proposal_id)
            .expect("joint proposal must bind vote plan");
        assert_eq!(plan.voter_subjects.len(), CHINA_CB.len() + CHINA_CH.len());
        assert_eq!(
            votebase::VoterSnapshot::<Test>::iter_prefix(proposal_id).count(),
            CHINA_CB.len() + CHINA_CH.len()
        );
        assert_eq!(
            votebase::InstitutionTicketCountSnapshot::<Test>::iter_prefix(proposal_id).count(),
            CHINA_CB.len() + CHINA_CH.len()
        );
        assert_eq!(
            votebase::AdminSnapshot::<Test>::iter_prefix(proposal_id).count(),
            0,
            "migrated joint proposals must not write administrator snapshots"
        );
    });
}

#[test]
fn institution_ticket_count_preserves_same_account_id_in_multiple_roles() {
    new_test_ext().execute_with(|| {
        let proposal_id = 77;
        let account = AccountId32::new([77; 32]);
        let second = AccountId32::new([78; 32]);
        let cid_a: votebase::types::CidNumber =
            nrc_cid_number().try_into().expect("bounded NRC CID");
        let cid_b: votebase::types::CidNumber = CHINA_CB[1]
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("bounded PRC CID");
        let role = |cid: votebase::types::CidNumber, code: &[u8]| {
            votebase::types::AuthorizationSubject::Institution(votebase::types::RoleSubject {
                cid_number: cid,
                role_code: code.to_vec().try_into().expect("bounded test role"),
            })
        };

        let role_a = role(cid_a.clone(), b"ROLE_A");
        let role_b = role(cid_a.clone(), b"ROLE_B");
        let role_c = role(cid_b.clone(), b"ROLE_C");
        assert_ok!(VotingEngine::snapshot_role_voters(
            proposal_id,
            role_a.clone(),
            vec![account.clone(), second]
        ));
        assert_ok!(VotingEngine::snapshot_role_voters(
            proposal_id,
            role_b.clone(),
            vec![account.clone()]
        ));
        assert_ok!(VotingEngine::snapshot_role_voters(
            proposal_id,
            role_c.clone(),
            vec![account.clone()]
        ));

        assert_eq!(
            VotingEngine::institution_ticket_count(proposal_id, cid_a),
            Some(3),
            "同一账户在同一机构的两个岗位各保留一张票据"
        );
        assert_eq!(
            VotingEngine::institution_ticket_count(proposal_id, cid_b),
            Some(1)
        );
        assert!(VotingEngine::is_subject_voter_in_snapshot(
            proposal_id,
            role_a,
            &account
        ));
        assert!(VotingEngine::is_subject_voter_in_snapshot(
            proposal_id,
            role_b,
            &account
        ));
        assert!(VotingEngine::is_subject_voter_in_snapshot(
            proposal_id,
            role_c,
            &account
        ));
    });
}

#[test]
fn joint_internal_requires_all_105_weight() {
    assert!(!is_joint_unanimous(JOINT_VOTE_PASS_THRESHOLD - 1));
    assert!(is_joint_unanimous(JOINT_VOTE_PASS_THRESHOLD));
}

#[test]
fn joint_referendum_uses_strict_majority() {
    assert!(!is_jointreferendum_vote_passed(50, 100));
    assert!(is_jointreferendum_vote_passed(51, 100));
    assert!(is_jointreferendum_vote_rejected(50, 100));
    assert!(!is_jointreferendum_vote_rejected(49, 100));
}

#[test]
fn joint_referendum_fails_closed_without_population() {
    assert!(!is_jointreferendum_vote_passed(1, 0));
    assert!(!is_jointreferendum_vote_rejected(1, 0));
}

#[test]
fn all_105_weight_via_cast_admin_passes_and_executes() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal();
        for (code, institution) in all_joint_institutions() {
            finalize_institution(proposal_id, code, institution, true);
        }

        assert_eq!(
            JointTallies::<Test>::get(proposal_id).yes,
            JOINT_VOTE_PASS_THRESHOLD
        );
        assert_eq!(
            VotingEngine::proposals(proposal_id).unwrap().status,
            STATUS_PASSED
        );

        <VotingEngine as Hooks<u64>>::on_initialize(System::block_number());
        assert_eq!(
            VotingEngine::proposals(proposal_id).unwrap().status,
            STATUS_EXECUTED
        );
    });
}

#[test]
fn one_institution_rejection_via_cast_admin_enters_referendum() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal();
        let cid_number = CHINA_CB[1].cid_number.as_bytes().to_vec();
        finalize_institution(proposal_id, votebase::PRC, cid_number, false);

        let proposal = VotingEngine::proposals(proposal_id).expect("proposal should exist");
        assert_eq!(proposal.stage, STAGE_REFERENDUM);
        assert_eq!(proposal.status, STATUS_VOTING);
    });
}

#[test]
fn joint_internal_timeout_via_public_finalizer_enters_referendum() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal();
        let end = VotingEngine::proposals(proposal_id).unwrap().end;
        System::set_block_number(end + 1);

        assert_ok!(VotingEngine::finalize_proposal(
            RuntimeOrigin::signed(AccountId32::new([250; 32])),
            proposal_id,
        ));

        let proposal = VotingEngine::proposals(proposal_id).expect("proposal should exist");
        assert_eq!(proposal.stage, STAGE_REFERENDUM);
        assert_eq!(proposal.status, STATUS_VOTING);
    });
}

#[test]
fn cast_referendum_extrinsic_uses_frozen_snapshot_and_strict_majority() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal();
        let cid_number = CHINA_CB[1].cid_number.as_bytes().to_vec();
        finalize_institution(proposal_id, votebase::PRC, cid_number, false);

        let voters = referendum_voters();
        assert_ok!(JointVote::cast_referendum(
            RuntimeOrigin::signed(voters[0].clone()),
            proposal_id,
            true,
        ));
        assert_eq!(
            VotingEngine::proposals(proposal_id).unwrap().status,
            STATUS_VOTING
        );
        assert_ok!(JointVote::cast_referendum(
            RuntimeOrigin::signed(voters[1].clone()),
            proposal_id,
            true,
        ));

        assert_eq!(ReferendumTallies::<Test>::get(proposal_id).yes, 2);
        assert_eq!(
            VotingEngine::proposals(proposal_id).unwrap().status,
            STATUS_PASSED
        );
    });
}

#[test]
fn newly_added_voter_cannot_enter_existing_snapshot() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal();
        let cid_number = CHINA_CB[1].cid_number.as_bytes().to_vec();
        finalize_institution(proposal_id, votebase::PRC, cid_number, false);

        let post_snapshot_account = AccountId32::new([204; 32]);
        assert!(JointVote::cast_referendum(
            RuntimeOrigin::signed(post_snapshot_account),
            proposal_id,
            true,
        )
        .is_err());
        let post_snapshot_cid: votebase::types::CidNumber =
            vec![204; 32].try_into().expect("account fits CID");
        assert!(!ReferendumVotesByCid::<Test>::contains_key(
            proposal_id,
            post_snapshot_cid,
        ));
    });
}

#[test]
fn same_citizen_cid_cannot_vote_twice_after_account_id_replacement() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal();
        let cid_number = CHINA_CB[1].cid_number.as_bytes().to_vec();
        finalize_institution(proposal_id, votebase::PRC, cid_number, false);

        let first_account_id = referendum_voters()[0].clone();
        assert_ok!(JointVote::cast_referendum(
            RuntimeOrigin::signed(first_account_id.clone()),
            proposal_id,
            true,
        ));
        assert_noop!(
            JointVote::cast_referendum(
                RuntimeOrigin::signed(replacement_account_id()),
                proposal_id,
                false,
            ),
            votebase::Error::<Test>::AlreadyVoted
        );

        let subject = referendum_subject(&first_account_id).expect("first voter is eligible");
        let ticket = ReferendumVotesByCid::<Test>::get(proposal_id, &subject.cid_number)
            .expect("complete citizen referendum ticket should exist");
        assert_eq!(ticket.voter_subject, subject);
        assert!(ticket.approve);
        assert_eq!(ReferendumTallies::<Test>::get(proposal_id).yes, 1);
        assert_eq!(ReferendumTallies::<Test>::get(proposal_id).no, 0);
    });
}
