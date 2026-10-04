use super::*;

#[test]
fn nrc_destroy_executes_when_yes_votes_reach_threshold() {
    new_test_ext().execute_with(|| {
        let institution = nrc_pallet_id();
        let account = institution_account_id(&institution);

        assert_ok!(ResolutionDestroy::propose_destroy(
            RuntimeOrigin::signed(nrc_admin(0)),
            nrc_cid(),
            bounded_test_role(NRC),
            institution.clone(),
            100
        ));
        let pid = last_proposal_id();

        for i in 1..13 {
            assert_ok!(cast_vote(nrc_admin(i), pid, true));
        }

        assert_eq!(Balances::free_balance(&account), 900);
        assert_eq!(
            Balances::free_balance(AccountId32::new(CHINA_CB[0].fee_account)),
            990
        );
    });
}

#[test]
fn destroy_fee_shortage_never_falls_back_to_admin() {
    new_test_ext().execute_with(|| {
        let institution = nrc_pallet_id();
        let account = institution_account_id(&institution);
        let fee_account = AccountId32::new(CHINA_CB[0].fee_account);
        let admin = nrc_admin(0);
        assert_ok!(Balances::force_set_balance(
            RuntimeOrigin::root(),
            fee_account.clone(),
            1,
        ));
        assert_ok!(Balances::force_set_balance(
            RuntimeOrigin::root(),
            admin.clone(),
            10_000,
        ));

        assert_ok!(ResolutionDestroy::propose_destroy(
            RuntimeOrigin::signed(admin.clone()),
            nrc_cid(),
            bounded_test_role(NRC),
            institution,
            100,
        ));
        let pid = last_proposal_id();
        for i in 1..13 {
            assert_ok!(cast_vote(nrc_admin(i), pid, true));
        }

        assert_eq!(Balances::free_balance(&account), 1_000);
        assert_eq!(Balances::free_balance(&fee_account), 1);
        assert_eq!(Balances::free_balance(&admin), 10_000);
        assert_eq!(
            votingengine::Pallet::<Test>::proposals(pid)
                .expect("proposal should exist")
                .status,
            STATUS_PASSED
        );
    });
}

#[test]
fn prc_destroy_executes_when_yes_votes_reach_threshold() {
    new_test_ext().execute_with(|| {
        let institution = prc_pallet_id();
        let account = institution_account_id(&institution);

        assert_ok!(ResolutionDestroy::propose_destroy(
            RuntimeOrigin::signed(prc_admin(0)),
            prc_cid(),
            bounded_test_role(PRC),
            institution.clone(),
            200
        ));
        let pid = last_proposal_id();

        for i in 1..6 {
            assert_ok!(cast_vote(prc_admin(i), pid, true));
        }

        assert_eq!(Balances::free_balance(&account), 800);
    });
}

#[test]
fn prb_destroy_executes_when_yes_votes_reach_threshold() {
    new_test_ext().execute_with(|| {
        let institution = prb_pallet_id();
        let account = institution_account_id(&institution);

        assert_ok!(ResolutionDestroy::propose_destroy(
            RuntimeOrigin::signed(prb_admin(0)),
            prb_cid(),
            bounded_test_role(PRB),
            institution.clone(),
            300
        ));
        let pid = last_proposal_id();

        for i in 1..6 {
            assert_ok!(cast_vote(prb_admin(i), pid, true));
        }

        assert_eq!(Balances::free_balance(&account), 700);
    });
}

#[test]
fn destroy_business_rejects_other_internal_vote_institutions() {
    new_test_ext().execute_with(|| {
        let njd_code = votingengine::types::NJD;
        let njd = primitives::cid::china::china_sf::CHINA_SF
            .iter()
            .find(|node| {
                votingengine::types::institution_code_from_cid_number(node.cid_number)
                    == Some(njd_code)
            })
            .map(|node| {
                (
                    CidNumber::try_from(node.cid_number.as_bytes().to_vec())
                        .expect("NJD CID fits runtime bound"),
                    AccountId32::new(node.main_account),
                )
            })
            .expect("NJD must exist in CHINA_SF");

        assert_noop!(
            ResolutionDestroy::propose_destroy(
                RuntimeOrigin::signed(nrc_admin(0)),
                njd.0,
                bounded_test_role(njd_code),
                njd.1,
                100,
            ),
            Error::<Test>::InvalidInstitution
        );
    });
}

#[test]
fn non_admin_cannot_propose_or_vote() {
    new_test_ext().execute_with(|| {
        let institution = nrc_pallet_id();

        assert_noop!(
            ResolutionDestroy::propose_destroy(
                RuntimeOrigin::signed(prc_admin(0)),
                nrc_cid(),
                bounded_test_role(NRC),
                institution.clone(),
                100
            ),
            Error::<Test>::UnauthorizedAdmin
        );

        assert_ok!(ResolutionDestroy::propose_destroy(
            RuntimeOrigin::signed(nrc_admin(0)),
            nrc_cid(),
            bounded_test_role(NRC),
            institution.clone(),
            100
        ));
        let pid = last_proposal_id();

        assert_noop!(
            cast_vote(prc_admin(0), pid, true),
            votingengine::pallet::Error::<Test>::NoPermission
        );
    });
}

#[test]
fn zero_amount_and_insufficient_balance_are_rejected() {
    new_test_ext().execute_with(|| {
        let institution = nrc_pallet_id();

        assert_noop!(
            ResolutionDestroy::propose_destroy(
                RuntimeOrigin::signed(nrc_admin(0)),
                nrc_cid(),
                bounded_test_role(NRC),
                institution.clone(),
                0
            ),
            Error::<Test>::ZeroAmount
        );

        assert_ok!(ResolutionDestroy::propose_destroy(
            RuntimeOrigin::signed(nrc_admin(0)),
            nrc_cid(),
            bounded_test_role(NRC),
            institution.clone(),
            2_000
        ));
        let pid = last_proposal_id();

        for i in 1..12 {
            assert_ok!(cast_vote(nrc_admin(i), pid, true));
        }

        // 发起人已自动赞成，最后一张补票触发自动执行失败路径。
        assert_ok!(cast_vote(nrc_admin(12), pid, true));
        assert_eq!(
            votingengine::Pallet::<Test>::proposals(pid)
                .expect("proposal should exist")
                .status,
            STATUS_PASSED
        );
        assert_eq!(
            Balances::free_balance(institution_account_id(&institution)),
            1_000
        );
        assert!(votingengine::Pallet::<Test>::get_proposal_data(pid).is_some());
        assert_ok!(VotingEngine::retry_passed_proposal(
            RuntimeOrigin::signed(nrc_admin(0)),
            pid
        ));
        assert_eq!(
            votingengine::Pallet::<Test>::proposal_execution_retry_state(pid)
                .expect("retry state should exist")
                .manual_attempts,
            1
        );
        assert_eq!(
            votingengine::Pallet::<Test>::proposals(pid)
                .expect("proposal should exist")
                .status,
            STATUS_PASSED
        );
    });
}

#[test]
fn existential_deposit_is_preserved() {
    new_test_ext().execute_with(|| {
        let institution = nrc_pallet_id();
        let account = institution_account_id(&institution);

        assert_ok!(ResolutionDestroy::propose_destroy(
            RuntimeOrigin::signed(nrc_admin(0)),
            nrc_cid(),
            bounded_test_role(NRC),
            institution.clone(),
            1_000
        ));
        let pid = last_proposal_id();

        for i in 1..13 {
            assert_ok!(cast_vote(nrc_admin(i), pid, true));
        }

        // 如果不校验 ED，这里会被销毁到 0 并触发账户 reap。
        assert_eq!(Balances::free_balance(&account), 1_000);
        assert_ok!(VotingEngine::retry_passed_proposal(
            RuntimeOrigin::signed(nrc_admin(0)),
            pid
        ));
        assert_eq!(Balances::free_balance(&account), 1_000);
        assert_eq!(
            votingengine::Pallet::<Test>::proposal_execution_retry_state(pid)
                .expect("retry state should exist")
                .manual_attempts,
            1
        );
    });
}

#[test]
fn rejected_proposal_does_not_block_new_proposal() {
    new_test_ext().execute_with(|| {
        let institution = nrc_pallet_id();
        assert_ok!(ResolutionDestroy::propose_destroy(
            RuntimeOrigin::signed(nrc_admin(0)),
            nrc_cid(),
            bounded_test_role(NRC),
            institution.clone(),
            100
        ));
        let pid1 = last_proposal_id();

        let end = votingengine::Pallet::<Test>::proposals(pid1)
            .expect("proposal should exist")
            .end;
        System::set_block_number(end + 1);
        assert_ok!(votingengine::Pallet::<Test>::finalize_proposal(
            RuntimeOrigin::signed(nrc_admin(0)),
            pid1
        ));
        assert_eq!(
            votingengine::Pallet::<Test>::proposals(pid1)
                .expect("proposal should exist")
                .status,
            STATUS_REJECTED
        );

        assert_ok!(ResolutionDestroy::propose_destroy(
            RuntimeOrigin::signed(nrc_admin(0)),
            nrc_cid(),
            bounded_test_role(NRC),
            institution.clone(),
            50
        ));
        let pid2 = last_proposal_id();
        // 提案 2 应该已创建
        assert!(votingengine::Pallet::<Test>::get_proposal_data(pid2).is_some());
    });
}

#[test]
fn execute_destroy_succeeds_after_failed_auto_execution() {
    new_test_ext().execute_with(|| {
        let institution = nrc_pallet_id();
        let account = institution_account_id(&institution);

        assert_ok!(ResolutionDestroy::propose_destroy(
            RuntimeOrigin::signed(nrc_admin(0)),
            nrc_cid(),
            bounded_test_role(NRC),
            institution.clone(),
            1_100
        ));
        let pid = last_proposal_id();

        for i in 1..13 {
            assert_ok!(cast_vote(nrc_admin(i), pid, true));
        }

        // 自动执行失败后状态保留为 PASSED，补充余额后可手动重试。
        assert_eq!(
            votingengine::Pallet::<Test>::proposals(pid)
                .expect("proposal should exist")
                .status,
            STATUS_PASSED
        );
        assert_eq!(Balances::free_balance(&account), 1_000);
        assert!(votingengine::Pallet::<Test>::get_proposal_data(pid).is_some());

        // 补充余额后手动重试执行
        let _ = Balances::deposit_creating(&account, 200);
        assert_ok!(VotingEngine::retry_passed_proposal(
            RuntimeOrigin::signed(nrc_admin(0)),
            pid
        ));
        assert_eq!(Balances::free_balance(&account), 100);
    });
}

#[test]
fn executed_proposal_does_not_block_new_proposal() {
    new_test_ext().execute_with(|| {
        let institution = nrc_pallet_id();
        assert_ok!(ResolutionDestroy::propose_destroy(
            RuntimeOrigin::signed(nrc_admin(0)),
            nrc_cid(),
            bounded_test_role(NRC),
            institution.clone(),
            100
        ));
        let pid1 = last_proposal_id();

        for i in 1..13 {
            assert_ok!(cast_vote(nrc_admin(i), pid1, true));
        }

        assert_ok!(ResolutionDestroy::propose_destroy(
            RuntimeOrigin::signed(nrc_admin(0)),
            nrc_cid(),
            bounded_test_role(NRC),
            institution.clone(),
            50
        ));
        let pid2 = last_proposal_id();
        assert!(votingengine::Pallet::<Test>::get_proposal_data(pid2).is_some());
    });
}

#[test]
fn duplicate_vote_is_rejected_by_votingengine() {
    new_test_ext().execute_with(|| {
        let institution = nrc_pallet_id();
        assert_ok!(ResolutionDestroy::propose_destroy(
            RuntimeOrigin::signed(nrc_admin(0)),
            nrc_cid(),
            bounded_test_role(NRC),
            institution.clone(),
            100
        ));
        let pid = last_proposal_id();
        assert_ok!(cast_vote(nrc_admin(1), pid, true));
        assert_noop!(
            cast_vote(nrc_admin(1), pid, true),
            votingengine::pallet::Error::<Test>::AlreadyVoted
        );
    });
}

#[test]
fn execute_destroy_requires_role_voter_snapshot() {
    new_test_ext().execute_with(|| {
        let institution = nrc_pallet_id();
        let account = institution_account_id(&institution);
        let outsider = AccountId32::new([99u8; 32]);

        assert_ok!(ResolutionDestroy::propose_destroy(
            RuntimeOrigin::signed(nrc_admin(0)),
            nrc_cid(),
            bounded_test_role(NRC),
            institution.clone(),
            1_100
        ));
        let pid = last_proposal_id();
        for i in 1..13 {
            assert_ok!(cast_vote(nrc_admin(i), pid, true));
        }
        let _ = Balances::deposit_creating(&account, 200);
        assert_noop!(
            VotingEngine::retry_passed_proposal(RuntimeOrigin::signed(outsider), pid),
            votingengine::pallet::Error::<Test>::NoPermission
        );
        assert_ok!(VotingEngine::retry_passed_proposal(
            RuntimeOrigin::signed(nrc_admin(0)),
            pid
        ));
        assert_eq!(Balances::free_balance(&account), 100);
    });
}

#[test]
fn institution_query_returns_none_for_invalid_account() {
    new_test_ext().execute_with(|| {
        assert_eq!(
            <TestInstitutionQuery as entity_primitives::InstitutionMultisigQuery<AccountId32>>::lookup_org(
                &AccountId32::new([0u8; 32])
            ),
            None
        );
    });
}
