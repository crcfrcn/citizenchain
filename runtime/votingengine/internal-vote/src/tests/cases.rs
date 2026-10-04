use super::*;

#[test]
fn unix_seconds_to_year_uses_utc_gregorian_boundaries() {
    new_test_ext().execute_with(|| {
        assert_eq!(
            VotingEngine::unix_seconds_to_year(1_798_761_600).expect("valid 2027 timestamp"),
            2027
        );
        assert_eq!(
            VotingEngine::unix_seconds_to_year(1_830_297_600).expect("valid 2028 timestamp"),
            2028
        );
        assert_eq!(
            VotingEngine::unix_seconds_to_year(1_861_919_999).expect("valid 2028 timestamp"),
            2028
        );
        assert_eq!(
            VotingEngine::unix_seconds_to_year(1_861_920_000).expect("valid 2029 timestamp"),
            2029
        );
        assert_eq!(
            VotingEngine::unix_seconds_to_year(1_956_528_000).expect("valid 2032 timestamp"),
            2032
        );
    });
}

#[test]
fn leap_year_rules_match_gregorian_calendar() {
    new_test_ext().execute_with(|| {
        assert!(VotingEngine::is_leap_year(2000));
        assert!(!VotingEngine::is_leap_year(2100));
        assert!(VotingEngine::is_leap_year(2400));
        assert_eq!(VotingEngine::days_in_year(2028), 366);
        assert_eq!(VotingEngine::days_in_year(2029), 365);
    });
}

#[test]
fn proposal_id_counter_resets_at_real_utc_year_boundary() {
    // 双层 ID：
    // - 主键 NextProposalId 全局单调累加,跨年不重置
    // - 展示号 ProposalDisplayMeta.seq_in_year 跨年自动重置回 0
    // - CurrentProposalYear 跟着系统时间切换
    new_test_ext().execute_with(|| {
        set_test_now_secs(1_830_297_599);
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());
        // 主键纯单调:首个提案 = 0
        assert_eq!(proposal_id, 0);
        // 展示号:2027 年首个,seq_in_year = 0
        let display = votingengine::pallet::ProposalDisplayId::<Test>::get(proposal_id)
            .expect("display id present");
        assert_eq!(display.year, 2027);
        assert_eq!(display.seq_in_year, 0);
        assert_eq!(CurrentProposalYear::<Test>::get(), 2027);
        assert_eq!(YearProposalCounter::<Test>::get(), 1);

        set_test_now_secs(1_830_297_600);
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(1), NRC, nrc_cid());
        // 主键单调累加:第二个提案 = 1(跨年也只 +1)
        assert_eq!(proposal_id, 1);
        // 展示号:2028 年首个,seq_in_year 跨年重置为 0
        let display = votingengine::pallet::ProposalDisplayId::<Test>::get(proposal_id)
            .expect("display id present");
        assert_eq!(display.year, 2028);
        assert_eq!(display.seq_in_year, 0);
        assert_eq!(CurrentProposalYear::<Test>::get(), 2028);
        assert_eq!(YearProposalCounter::<Test>::get(), 1);
        assert_eq!(NextProposalId::<Test>::get(), 2);
    });
}

#[test]
fn internal_proposal_must_be_created_by_same_institution_admin() {
    new_test_ext().execute_with(|| {
        // `create_internal_proposal` 仅由业务模块通过 `InternalVoteEngine` trait
        // 入口调用,这里直接验证 trait 路径的权限校验。
        let outsider = AccountId32::new([7u8; 32]);

        assert_noop!(
            <InternalVote as InternalVoteEngine<AccountId32>>::create_institution_proposal_with_data(
                outsider,
                NRC,
                nrc_cid().to_vec(),
                None,
                subject_cids_for(&nrc_cid()),
                internal_vote_plan(&nrc_cid(), b"payload"),
                b"payload".to_vec(),
            ),
            votingengine::Error::<Test>::NoPermission
        );

        assert_noop!(
            <InternalVote as InternalVoteEngine<AccountId32>>::create_institution_proposal_with_data(
                prc_admin(0),
                NRC,
                nrc_cid().to_vec(),
                None,
                subject_cids_for(&nrc_cid()),
                internal_vote_plan(&nrc_cid(), b"payload"),
                b"payload".to_vec(),
            ),
            votingengine::Error::<Test>::NoPermission
        );

        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());
        // 双层 ID：主键纯单调，首个提案 = 0；展示号通过 ProposalDisplayId 反查。
        assert_eq!(proposal_id, 0);
        let display = votingengine::pallet::ProposalDisplayId::<Test>::get(proposal_id)
            .expect("display id present");
        assert_eq!(display.year, 2026);
        assert_eq!(display.seq_in_year, 0);
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal exists")
                .stage,
            STAGE_INTERNAL
        );
    });
}

#[test]
fn institution_proposal_rejects_personal_code() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            <InternalVote as InternalVoteEngine<AccountId32>>::create_institution_proposal_with_data(
                pending_personal_admin(0),
                PERSONAL_CODE,
                nrc_cid().to_vec(),
                None,
                subject_cids_for(&nrc_cid()),
                internal_vote_plan(&nrc_cid(), b"payload"),
                b"payload".to_vec(),
            ),
            Error::<Test>::InvalidInternalCode
        );
    });
}

#[test]
fn governance_internal_proposal_snapshots_fixed_threshold_not_provider() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());

        // 测试 Provider 对治理机构故意返回 1，这里必须仍写入固定治理阈值。
        assert_eq!(
            InternalThresholdSnapshot::<Test>::get(proposal_id),
            Some(primitives::count_const::NRC_INTERNAL_THRESHOLD)
        );
    });
}

#[test]
fn permanent_singleton_reads_entity_threshold_and_freezes_snapshot() {
    new_test_ext().execute_with(|| {
        let actor_cid_number = permanent_singleton_cid();
        set_institution_threshold(actor_cid_number.clone(), 2);

        let proposal_id = create_internal_proposal_via_engine(
            permanent_singleton_admin(0),
            PERMANENT_SINGLETON_CODE,
            actor_cid_number.clone(),
        );
        assert_eq!(InternalThresholdSnapshot::<Test>::get(proposal_id), Some(2));
        assert_eq!(
            <InternalVote as InternalVoteEngine<AccountId32>>::active_institution_threshold(
                PERMANENT_SINGLETON_CODE,
                actor_cid_number.as_slice(),
            ),
            Some(2)
        );
    });
}

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
fn institution_proposal_keeps_cid_identity_and_execution_account_separate() {
    new_test_ext().execute_with(|| {
        for (institution_code, actor_cid_number) in
            [(PUBLIC_CODE, public_cid()), (PRIVATE_CODE, private_cid())]
        {
            let execution_account_id = test_institution_execution_account();
            let proposal_id = <InternalVote as InternalVoteEngine<AccountId32>>::create_institution_proposal_with_data(
                test_institution_admin(0),
                institution_code,
                actor_cid_number.to_vec(),
                Some(execution_account_id.clone()),
                subject_cids_for(&actor_cid_number),
                internal_vote_plan(&actor_cid_number, b"payload"),
                b"payload".to_vec(),
            )
            .expect("institution proposal should be created");

            assert_eq!(InternalThresholdSnapshot::<Test>::get(proposal_id), Some(3));
            let role_subject = AuthorizationSubject::Institution(RoleSubject {
                cid_number: actor_cid_number.clone(),
                role_code: test_institution_role(institution_code)
                    .to_vec()
                    .try_into()
                    .expect("测试岗位码合法"),
            });
            assert!(VotingEngine::is_subject_voter_in_snapshot(
                proposal_id,
                role_subject,
                &test_institution_admin(1)
            ));
            let proposal = VotingEngine::proposals(proposal_id).expect("proposal should exist");
            assert_eq!(proposal.actor_cid_number, Some(actor_cid_number));
            assert_eq!(proposal.execution_account_id, Some(execution_account_id));
        }
    });
}

#[test]
fn pending_personal_dynamic_threshold_must_be_strict_majority() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            <InternalVote as InternalVoteEngine<AccountId32>>::create_personal_account_create_proposal_with_data(
                pending_personal_admin(0),
                pending_personal_account(),
                sp_std::vec![pending_personal_admin(0), pending_personal_admin(1)],
                1,
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
            votingengine::Error::<Test>::InvalidInstitution
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
fn institution_orgs_snapshot_dynamic_active_threshold_by_cid() {
    new_test_ext().execute_with(|| {
        for (institution_code, actor_cid_number) in
            [(PUBLIC_CODE, public_cid()), (PRIVATE_CODE, private_cid())]
        {
            set_institution_threshold(actor_cid_number.clone(), 3);
            let proposal_id = create_internal_proposal_via_engine(
                test_institution_admin(0),
                institution_code,
                actor_cid_number.clone(),
            );

            assert_eq!(InternalThresholdSnapshot::<Test>::get(proposal_id), Some(3));
            let role_subject = AuthorizationSubject::Institution(RoleSubject {
                cid_number: actor_cid_number.clone(),
                role_code: test_institution_role(institution_code)
                    .to_vec()
                    .try_into()
                    .expect("测试岗位码合法"),
            });
            assert!(VotingEngine::is_subject_voter_in_snapshot(
                proposal_id,
                role_subject,
                &test_institution_admin(2)
            ));
        }
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
            votingengine::Error::<Test>::AdminSetMutationProposalActive
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
            votingengine::Error::<Test>::RegularInternalProposalActive
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
fn regular_internal_proposals_can_coexist_under_same_subject() {
    new_test_ext().execute_with(|| {
        let first = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());
        let second = create_internal_proposal_via_engine(nrc_admin(1), NRC, nrc_cid());

        assert_ne!(first, second);
        let state = internal_mutex_for(nrc_cid()).expect("mutex should exist");
        assert_eq!(state.regular_active_count, 2);
        assert_eq!(state.admin_set_mutation_proposal, None);
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
            votingengine::Error::<Test>::AdminSetMutationProposalActive
        );

        assert_ok!(VotingEngine::set_status_and_emit(
            proposal_id,
            STATUS_EXECUTION_FAILED
        ));
        assert!(personal_mutex_for(personal_account_id()).is_none());
    });
}

#[test]
fn proposal_status_transition_state_machine_is_strict() {
    new_test_ext().execute_with(|| {
        let voting_to_passed = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());
        assert_noop!(
            VotingEngine::set_status_and_emit(voting_to_passed, STATUS_EXECUTED),
            votingengine::Error::<Test>::InvalidProposalStatus
        );
        assert_noop!(
            VotingEngine::set_status_and_emit(voting_to_passed, STATUS_EXECUTION_FAILED),
            votingengine::Error::<Test>::InvalidProposalStatus
        );
        assert_noop!(
            VotingEngine::set_status_and_emit(voting_to_passed, STATUS_VOTING),
            votingengine::Error::<Test>::InvalidProposalStatus
        );
        assert_ok!(VotingEngine::set_status_and_emit(
            voting_to_passed,
            STATUS_PASSED
        ));
        assert_noop!(
            VotingEngine::set_status_and_emit(voting_to_passed, STATUS_REJECTED),
            votingengine::Error::<Test>::InvalidProposalStatus
        );
        assert_noop!(
            VotingEngine::set_status_and_emit(voting_to_passed, STATUS_VOTING),
            votingengine::Error::<Test>::InvalidProposalStatus
        );
        assert_ok!(VotingEngine::set_status_and_emit(
            voting_to_passed,
            STATUS_EXECUTED
        ));
        assert_noop!(
            VotingEngine::set_status_and_emit(voting_to_passed, STATUS_PASSED),
            votingengine::Error::<Test>::InvalidProposalStatus
        );

        let passed_to_failed = create_internal_proposal_via_engine(nrc_admin(1), NRC, nrc_cid());
        assert_ok!(VotingEngine::set_status_and_emit(
            passed_to_failed,
            STATUS_PASSED
        ));
        assert_ok!(VotingEngine::set_status_and_emit(
            passed_to_failed,
            STATUS_EXECUTION_FAILED
        ));
        assert_noop!(
            VotingEngine::set_status_and_emit(passed_to_failed, STATUS_EXECUTED),
            votingengine::Error::<Test>::InvalidProposalStatus
        );

        let rejected = create_internal_proposal_via_engine(nrc_admin(2), NRC, nrc_cid());
        assert_ok!(VotingEngine::set_status_and_emit(rejected, STATUS_REJECTED));
        assert_noop!(
            VotingEngine::set_status_and_emit(rejected, STATUS_PASSED),
            votingengine::Error::<Test>::InvalidProposalStatus
        );
    });
}

#[test]
fn internal_vote_must_be_by_same_institution_admin() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_internal_proposal_via_engine(prb_admin(0), PRB, prb_cid());

        assert_noop!(
            cast_internal_vote_via_extrinsic(nrc_admin(0), proposal_id, true),
            votingengine::Error::<Test>::NoPermission
        );

        assert_ok!(cast_internal_vote_via_extrinsic(
            prb_admin(1),
            proposal_id,
            true
        ));
    });
}

#[test]
fn nrc_internal_vote_passes_at_13_yes_votes() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());

        for i in 1..12 {
            assert_ok!(cast_internal_vote_via_extrinsic(
                nrc_admin(i),
                proposal_id,
                true
            ));
        }
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal exists")
                .status,
            STATUS_VOTING
        );

        assert_ok!(cast_internal_vote_via_extrinsic(
            nrc_admin(12),
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
fn internal_vote_is_rejected_after_timeout() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_internal_proposal_via_engine(prc_admin(0), PRC, prc_cid());

        let proposal = VotingEngine::proposals(proposal_id).expect("proposal exists");
        System::set_block_number(proposal.end + 1);

        assert_ok!(VotingEngine::finalize_proposal(
            RuntimeOrigin::signed(prc_admin(0)),
            proposal_id,
        ));
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal exists")
                .status,
            STATUS_REJECTED
        );
    });
}

#[test]
fn internal_vote_timeout_is_auto_rejected_on_initialize() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_internal_proposal_via_engine(prc_admin(0), PRC, prc_cid());

        let proposal = VotingEngine::proposals(proposal_id).expect("proposal exists");
        System::set_block_number(proposal.end);
        <VotingEngine as Hooks<u64>>::on_initialize(proposal.end);
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal should exist")
                .status,
            STATUS_VOTING
        );

        let next = proposal.end + 1;
        System::set_block_number(next);
        <VotingEngine as Hooks<u64>>::on_initialize(next);
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal should exist")
                .status,
            STATUS_REJECTED
        );
    });
}

#[test]
fn joint_proposal_must_be_created_by_nrc_or_prc_admin() {
    new_test_ext().execute_with(|| {
        // `create_joint_proposal` 仅由业务模块通过 `JointVoteEngine` trait
        // 入口调用,这里直接验证 trait 路径的权限校验。

        // 外部人员不能创建联合提案
        let outsider = AccountId32::new([9u8; 32]);
        assert_noop!(
            try_create_joint_proposal_for(outsider, nrc_cid(), 10,),
            votingengine::Error::<Test>::NoPermission
        );

        // 省储委会管理员可以创建联合提案
        set_population_data(10);
        assert_ok!(try_create_joint_proposal_for(prc_admin(0), prc_cid(), 10,));

        // 国家储委会管理员可以创建联合提案
        // 使用独立外部状态验证另一类合法发起人，避免两个联合提案争用同一组治理锁。
    });
    new_test_ext().execute_with(|| {
        set_population_data(10);
        assert_ok!(try_create_joint_proposal_for(nrc_admin(0), nrc_cid(), 10,));
    });
}

#[test]
fn joint_proposal_creates_and_binds_population_snapshot_inline() {
    new_test_ext().execute_with(|| {
        set_population_data(10);
        let proposal_id = try_create_joint_proposal_for(nrc_admin(0), nrc_cid(), 10)
            .expect("joint proposal should create its own snapshot");
        assert_eq!(
            VotingEngine::population_eligible_total_of(proposal_id),
            Some(10)
        );
    });
}

#[test]
fn joint_proposal_with_empty_population_rolls_back() {
    new_test_ext().execute_with(|| {
        set_population_data(0);
        assert_noop!(
            try_create_joint_proposal_for(nrc_admin(0), nrc_cid(), 0,),
            joint_vote::Error::<Test>::CitizenEligibleTotalNotSet
        );
        assert_eq!(votingengine::pallet::NextProposalId::<Test>::get(), 0);
        assert!(!votingengine::pallet::Proposals::<Test>::contains_key(0));
    });
}

#[test]
fn joint_vote_requires_current_institution_admin() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal_for(nrc_admin(0), nrc_cid(), 10);

        assert_ok!(submit_joint_vote(
            nrc_admin(0),
            proposal_id,
            nrc_cid(),
            true
        ));

        assert_ok!(submit_joint_vote(
            prc_admin(0),
            proposal_id,
            prc_cid(),
            true
        ));

        assert_noop!(
            submit_joint_vote(prc_admin(0), proposal_id, nrc_cid(), true),
            votingengine::Error::<Test>::NoPermission
        );
    });
}

#[test]
fn joint_vote_rejects_duplicate_admin_vote() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal_for(nrc_admin(0), nrc_cid(), 10);

        assert_ok!(submit_joint_vote(
            nrc_admin(0),
            proposal_id,
            nrc_cid(),
            true
        ));

        assert_noop!(
            submit_joint_vote(nrc_admin(0), proposal_id, nrc_cid(), true),
            votingengine::Error::<Test>::AlreadyVoted
        );
    });
}

#[test]
fn joint_vote_uses_fixed_governance_threshold_not_provider() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal_for(nrc_admin(0), nrc_cid(), 10);

        // 测试 Provider 对治理机构故意返回 1；联合投票必须等固定阈值票数才形成机构结果。
        assert_ok!(submit_joint_vote(
            nrc_admin(0),
            proposal_id,
            nrc_cid(),
            true
        ));
        assert_eq!(
            joint_vote::JointVotesByInstitution::<Test>::get(proposal_id, nrc_cid()),
            None
        );

        for i in 1..primitives::count_const::NRC_INTERNAL_THRESHOLD as usize {
            assert_ok!(submit_joint_vote(
                nrc_admin(i),
                proposal_id,
                nrc_cid(),
                true
            ));
        }
        assert_eq!(
            joint_vote::JointVotesByInstitution::<Test>::get(proposal_id, nrc_cid()),
            Some(true)
        );
    });
}

#[test]
fn national_judicial_yuan_uses_fixed_internal_threshold() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_internal_proposal_via_engine(njd_admin(0), NJD, njd_cid());
        assert_eq!(
            InternalThresholdSnapshot::<Test>::get(proposal_id),
            Some(primitives::count_const::NJD_INTERNAL_THRESHOLD)
        );
    });
}

#[test]
fn joint_vote_auto_rejects_institution_when_yes_is_no_longer_reachable() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal_for(nrc_admin(0), nrc_cid(), 10);

        cast_joint_votes_until_finalized(proposal_id, nrc_cid(), false);

        assert_eq!(
            joint_vote::JointVotesByInstitution::<Test>::get(proposal_id, nrc_cid()),
            Some(false)
        );
        let proposal = Proposals::<Test>::get(proposal_id).expect("proposal should exist");
        assert_eq!(proposal.stage, STAGE_REFERENDUM);
        assert_eq!(
            joint_vote::JointTallies::<Test>::get(proposal_id).no,
            primitives::count_const::NRC_JOINT_VOTE_WEIGHT
        );
    });
}

#[test]
fn joint_stage_mutex_is_keyed_by_cid_and_released_at_referendum_stage() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal_for(nrc_admin(0), nrc_cid(), 10);

        assert!(internal_mutex_for(nrc_cid()).is_some());

        cast_joint_votes_until_finalized(proposal_id, nrc_cid(), false);
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal should exist")
                .stage,
            STAGE_REFERENDUM
        );
        assert!(internal_mutex_for(nrc_cid()).is_none());
    });
}

#[test]
fn joint_referendum_allows_eligible_account() {
    new_test_ext().execute_with(|| {
        insert_joint_referendum_proposal(0, 10, 100);

        assert_ok!(<joint_vote::Pallet<Test>>::do_jointreferendum_vote(
            nrc_admin(0),
            0,
            true
        ));
        assert_eq!(joint_vote::ReferendumTallies::<Test>::get(0).yes, 1);
        assert!(joint_vote::ReferendumVotesByCid::<Test>::contains_key(
            0,
            test_citizen_subject(&nrc_admin(0)).cid_number
        ));
    });
}

#[test]
fn joint_referendum_same_account_can_only_vote_once_per_proposal() {
    new_test_ext().execute_with(|| {
        insert_joint_referendum_proposal(0, 10, 100);

        assert_ok!(<joint_vote::Pallet<Test>>::do_jointreferendum_vote(
            nrc_admin(0),
            0,
            true
        ));

        assert_noop!(
            <joint_vote::Pallet<Test>>::do_jointreferendum_vote(nrc_admin(0), 0, false),
            votingengine::Error::<Test>::AlreadyVoted
        );
    });
}

#[test]
fn joint_referendum_same_account_can_vote_on_different_proposals() {
    new_test_ext().execute_with(|| {
        insert_joint_referendum_proposal(0, 10, 100);
        insert_joint_referendum_proposal(1, 10, 100);

        assert_ok!(<joint_vote::Pallet<Test>>::do_jointreferendum_vote(
            nrc_admin(0),
            0,
            true
        ));
        assert_ok!(<joint_vote::Pallet<Test>>::do_jointreferendum_vote(
            nrc_admin(0),
            1,
            true
        ));
    });
}

#[test]
fn joint_referendum_rejects_when_eligible_total_not_set_in_proposal() {
    new_test_ext().execute_with(|| {
        insert_joint_referendum_proposal(0, 0, 100);

        assert_noop!(
            <joint_vote::Pallet<Test>>::do_jointreferendum_vote(nrc_admin(0), 0, true),
            joint_vote::Error::<Test>::CitizenEligibleTotalNotSet
        );
    });
}

#[test]
fn joint_referendum_rejects_votes_beyond_population_snapshot_denominator() {
    new_test_ext().execute_with(|| {
        insert_joint_referendum_proposal(0, 10, 100);
        joint_vote::ReferendumTallies::<Test>::insert(0, VoteCountU64 { yes: 5, no: 5 });

        assert_noop!(
            <joint_vote::Pallet<Test>>::do_jointreferendum_vote(nrc_admin(0), 0, true),
            joint_vote::Error::<Test>::ReferendumSnapshotExhausted
        );
        assert!(!joint_vote::ReferendumVotesByCid::<Test>::contains_key(
            0,
            test_citizen_subject(&nrc_admin(0)).cid_number
        ));
    });
}

#[test]
fn joint_referendum_timeout_with_half_or_less_is_rejected() {
    new_test_ext().execute_with(|| {
        insert_joint_referendum_proposal(0, 10, 5);
        joint_vote::ReferendumTallies::<Test>::insert(0, VoteCountU64 { yes: 5, no: 0 });
        System::set_block_number(6);

        assert_ok!(VotingEngine::finalize_proposal(
            RuntimeOrigin::signed(nrc_admin(0)),
            0
        ));
        assert_eq!(
            Proposals::<Test>::get(0)
                .expect("proposal should exist")
                .status,
            STATUS_REJECTED
        );
    });
}

#[test]
fn joint_referendum_timeout_is_auto_rejected_on_initialize() {
    new_test_ext().execute_with(|| {
        insert_joint_referendum_proposal(0, 10, 5);
        assert_ok!(VotingEngine::schedule_proposal_expiry(0, 5));
        joint_vote::ReferendumTallies::<Test>::insert(0, VoteCountU64 { yes: 5, no: 0 });

        System::set_block_number(6);
        <VotingEngine as Hooks<u64>>::on_initialize(6);
        assert_eq!(
            Proposals::<Test>::get(0)
                .expect("proposal should exist")
                .status,
            STATUS_REJECTED
        );
    });
}

#[test]
fn joint_referendum_timeout_auto_registers_cleanup_and_clears_referendum_votes() {
    new_test_ext().execute_with(|| {
        insert_joint_referendum_proposal(0, 10, 5);
        assert_ok!(VotingEngine::schedule_proposal_expiry(0, 5));

        assert_ok!(<joint_vote::Pallet<Test>>::do_jointreferendum_vote(
            nrc_admin(0),
            0,
            true
        ));
        assert!(joint_vote::ReferendumVotesByCid::<Test>::contains_key(
            0,
            test_citizen_subject(&nrc_admin(0)).cid_number
        ));

        System::set_block_number(6);
        <VotingEngine as Hooks<u64>>::on_initialize(6);

        assert_eq!(
            Proposals::<Test>::get(0)
                .expect("proposal should exist")
                .status,
            STATUS_REJECTED
        );
        assert!(joint_vote::ReferendumVotesByCid::<Test>::contains_key(
            0,
            test_citizen_subject(&nrc_admin(0)).cid_number
        ));

        let retention = 90u64 * primitives::pow_const::BLOCKS_PER_DAY;
        let cleanup_block = 6 + retention;
        for i in 0..20u64 {
            System::set_block_number(cleanup_block + i);
            <VotingEngine as Hooks<u64>>::on_initialize(cleanup_block + i);
        }
        assert!(!joint_vote::ReferendumVotesByCid::<Test>::contains_key(
            0,
            test_citizen_subject(&nrc_admin(0)).cid_number
        ));
    });
}

#[test]
fn joint_referendum_rejects_ineligible_account() {
    new_test_ext().execute_with(|| {
        insert_joint_referendum_proposal(0, 10, 100);
        let outsider = AccountId32::new([7u8; 32]);

        assert_noop!(
            <joint_vote::Pallet<Test>>::do_jointreferendum_vote(outsider, 0, true),
            joint_vote::Error::<Test>::CitizenNotEligible
        );
    });
}

#[test]
fn joint_referendum_rejects_when_not_in_referendum_stage() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal_for(nrc_admin(0), nrc_cid(), 10);

        assert_noop!(
            <joint_vote::Pallet<Test>>::do_jointreferendum_vote(nrc_admin(0), proposal_id, true),
            votingengine::Error::<Test>::InvalidProposalStage
        );
    });
}

#[test]
fn joint_referendum_passes_immediately_when_yes_exceeds_half() {
    new_test_ext().execute_with(|| {
        insert_joint_referendum_proposal(0, 10, 100);
        joint_vote::ReferendumTallies::<Test>::insert(0, VoteCountU64 { yes: 5, no: 0 });

        assert_ok!(<joint_vote::Pallet<Test>>::do_jointreferendum_vote(
            nrc_admin(0),
            0,
            true
        ));
        process_current_block();

        let proposal = Proposals::<Test>::get(0).expect("proposal should exist");
        assert_eq!(proposal.status, STATUS_EXECUTED);
    });
}

#[test]
fn delayed_cleanup_cleans_referendum_votes_after_retention() {
    new_test_ext().execute_with(|| {
        insert_joint_referendum_proposal(0, 10, 100);
        joint_vote::ReferendumTallies::<Test>::insert(0, VoteCountU64 { yes: 5, no: 0 });

        assert_ok!(<joint_vote::Pallet<Test>>::do_jointreferendum_vote(
            nrc_admin(0),
            0,
            true
        ));
        process_current_block();

        let proposal = Proposals::<Test>::get(0).expect("proposal should exist");
        assert_eq!(proposal.status, STATUS_EXECUTED);
        assert!(joint_vote::ReferendumVotesByCid::<Test>::contains_key(
            0,
            test_citizen_subject(&nrc_admin(0)).cid_number
        ));

        let retention = 90u64 * primitives::pow_const::BLOCKS_PER_DAY;
        let cleanup_block = retention;
        for i in 0..20u64 {
            System::set_block_number(cleanup_block + i);
            <VotingEngine as Hooks<u64>>::on_initialize(cleanup_block + i);
        }
        assert!(!joint_vote::ReferendumVotesByCid::<Test>::contains_key(
            0,
            test_citizen_subject(&nrc_admin(0)).cid_number
        ));
    });
}

#[test]
fn delayed_cleanup_removes_joint_vote_plan_and_role_snapshots() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal_for(nrc_admin(0), nrc_cid(), 100);
        assert!(votingengine::ProposalVotePlans::<Test>::contains_key(
            proposal_id
        ));
        assert!(
            votingengine::VoterSnapshot::<Test>::iter_prefix(proposal_id)
                .next()
                .is_some()
        );
        assert!(
            votingengine::InstitutionTicketCountSnapshot::<Test>::iter_prefix(proposal_id)
                .next()
                .is_some()
        );

        assert_ok!(VotingEngine::set_status_and_emit(
            proposal_id,
            STATUS_REJECTED
        ));
        let retention = 90u64 * primitives::pow_const::BLOCKS_PER_DAY;
        for offset in 0..30u64 {
            System::set_block_number(retention + offset);
            <VotingEngine as Hooks<u64>>::on_initialize(retention + offset);
        }

        assert!(!votingengine::ProposalVotePlans::<Test>::contains_key(
            proposal_id
        ));
        assert_eq!(
            votingengine::VoterSnapshot::<Test>::iter_prefix(proposal_id).count(),
            0
        );
        assert_eq!(
            votingengine::InstitutionTicketCountSnapshot::<Test>::iter_prefix(proposal_id).count(),
            0
        );
    });
}

#[test]
fn joint_referendum_finalize_before_timeout_is_rejected() {
    new_test_ext().execute_with(|| {
        insert_joint_referendum_proposal(0, 10, 100);
        System::set_block_number(100);

        assert_noop!(
            VotingEngine::finalize_proposal(RuntimeOrigin::signed(nrc_admin(0)), 0),
            votingengine::Error::<Test>::VoteNotExpired
        );
    });
}

#[test]
fn joint_referendum_pass_threshold_function_boundaries_are_correct() {
    assert!(!joint_vote::is_jointreferendum_vote_passed(0, 0));
    assert!(!joint_vote::is_jointreferendum_vote_passed(5, 10));
    assert!(joint_vote::is_jointreferendum_vote_passed(6, 10));

    // 极端大数不能因为 u64 乘法饱和把刚过半误判为未通过。
    let eligible = u64::MAX;
    assert!(!joint_vote::is_jointreferendum_vote_passed(
        eligible / 2,
        eligible
    ));
    assert!(joint_vote::is_jointreferendum_vote_passed(
        eligible / 2 + 1,
        eligible
    ));
}

#[test]
fn joint_referendum_reject_threshold_function_boundaries_are_correct() {
    // eligible_total=0 → 不否决（无意义）
    assert!(!joint_vote::is_jointreferendum_vote_rejected(0, 0));
    // 反对 4/10 = 40% < 50% → 不否决（赞成仍有可能 > 50%）
    assert!(!joint_vote::is_jointreferendum_vote_rejected(4, 10));
    // 反对 5/10 = 50% → 否决（赞成最多 50%，无法严格 > 50%）
    assert!(joint_vote::is_jointreferendum_vote_rejected(5, 10));
    // 反对 6/10 = 60% → 否决
    assert!(joint_vote::is_jointreferendum_vote_rejected(6, 10));

    // 极端大数不能因为 u64 乘法饱和把刚过半误判为未否决。
    let eligible = u64::MAX;
    assert!(!joint_vote::is_jointreferendum_vote_rejected(
        eligible / 2,
        eligible
    ));
    assert!(joint_vote::is_jointreferendum_vote_rejected(
        eligible / 2 + 1,
        eligible
    ));
}

#[test]
fn joint_vote_all_yes_passes_immediately() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal_for(nrc_admin(0), nrc_cid(), 100);

        cast_joint_votes_until_finalized(proposal_id, nrc_cid(), true);

        for (institution, _) in all_prc_institutions() {
            cast_joint_votes_until_finalized(proposal_id, institution, true);
        }
        for (institution, _) in all_prb_institutions() {
            cast_joint_votes_until_finalized(proposal_id, institution, true);
        }

        let proposal = Proposals::<Test>::get(proposal_id).expect("proposal should exist");
        assert_eq!(proposal.status, STATUS_EXECUTED);
        assert_eq!(proposal.stage, STAGE_JOINT);
        assert_eq!(
            joint_vote::JointTallies::<Test>::get(proposal_id).yes,
            primitives::count_const::JOINT_VOTE_TOTAL
        );
    });
}

#[test]
fn joint_vote_non_unanimous_moves_to_referendum_immediately_after_one_institution_rejects() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal_for(nrc_admin(0), nrc_cid(), 77);
        cast_joint_votes_until_finalized(proposal_id, nrc_cid(), true);
        let first_prc = all_prc_institutions()
            .first()
            .cloned()
            .expect("there should be at least one prc institution");
        cast_joint_votes_until_finalized(proposal_id, first_prc.0, false);

        let proposal = Proposals::<Test>::get(proposal_id).expect("proposal should exist");
        assert_eq!(proposal.stage, STAGE_REFERENDUM);
        assert_eq!(proposal.status, STATUS_VOTING);
        assert_eq!(proposal.start, System::block_number());
        assert_eq!(
            proposal.end,
            proposal.start + primitives::count_const::VOTING_DURATION_BLOCKS as u64
        );
        assert_eq!(
            VotingEngine::population_eligible_total_of(proposal_id),
            Some(77)
        );
        assert_eq!(joint_vote::JointTallies::<Test>::get(proposal_id).no, 1);
    });
}

#[test]
fn joint_vote_timeout_moves_to_referendum_when_not_unanimous() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal_for(nrc_admin(0), nrc_cid(), 88);

        assert_ok!(submit_joint_vote(
            nrc_admin(0),
            proposal_id,
            nrc_cid(),
            true
        ));

        let proposal = Proposals::<Test>::get(proposal_id).expect("proposal should exist");
        System::set_block_number(proposal.end + 1);
        assert_ok!(VotingEngine::finalize_proposal(
            RuntimeOrigin::signed(nrc_admin(0)),
            proposal_id
        ));

        let proposal = Proposals::<Test>::get(proposal_id).expect("proposal should exist");
        assert_eq!(proposal.stage, STAGE_REFERENDUM);
        assert_eq!(proposal.status, STATUS_VOTING);
        assert_eq!(
            proposal.end,
            (proposal.start + primitives::count_const::VOTING_DURATION_BLOCKS as u64)
        );
    });
}

#[test]
fn joint_vote_timeout_auto_moves_to_referendum_on_initialize() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal_for(nrc_admin(0), nrc_cid(), 88);

        assert_ok!(submit_joint_vote(
            nrc_admin(0),
            proposal_id,
            nrc_cid(),
            true
        ));

        let proposal = Proposals::<Test>::get(proposal_id).expect("proposal should exist");
        let expired_at = proposal.end + 1;
        System::set_block_number(expired_at);
        <VotingEngine as Hooks<u64>>::on_initialize(expired_at);

        let proposal = Proposals::<Test>::get(proposal_id).expect("proposal should exist");
        assert_eq!(proposal.stage, STAGE_REFERENDUM);
        assert_eq!(proposal.status, STATUS_VOTING);
        assert_eq!(proposal.start, expired_at);
        assert_eq!(
            proposal.end,
            expired_at + primitives::count_const::VOTING_DURATION_BLOCKS as u64
        );
    });
}

#[test]
fn joint_vote_timeout_with_unanimous_tally_passes() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal_for(nrc_admin(0), nrc_cid(), 66);
        joint_vote::JointTallies::<Test>::insert(
            proposal_id,
            VoteCountU32 {
                yes: primitives::count_const::JOINT_VOTE_TOTAL,
                no: 0,
            },
        );

        let proposal = Proposals::<Test>::get(proposal_id).expect("proposal should exist");
        System::set_block_number(proposal.end + 1);
        assert_ok!(VotingEngine::finalize_proposal(
            RuntimeOrigin::signed(nrc_admin(0)),
            proposal_id
        ));
        process_current_block();

        let proposal = Proposals::<Test>::get(proposal_id).expect("proposal should exist");
        assert_eq!(proposal.status, STATUS_EXECUTED);
        assert_eq!(proposal.stage, STAGE_JOINT);
    });
}

#[test]
fn joint_vote_callback_failure_defers_execution_without_reverting_vote_result() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal_for(nrc_admin(0), nrc_cid(), 100);

        set_joint_callback_should_fail(true);
        assert_ok!(VotingEngine::set_status_and_emit(
            proposal_id,
            STATUS_PASSED
        ));
        process_current_block();

        let proposal = Proposals::<Test>::get(proposal_id).expect("proposal should exist");
        assert_eq!(proposal.status, STATUS_PASSED);
        assert_eq!(proposal.stage, STAGE_JOINT);
        let pending = votingengine::pallet::PendingProposalExecutions::<Test>::get(proposal_id)
            .expect("failed callback should remain queued");
        assert_eq!(pending.attempts, 1);
        assert!(pending.next_attempt_at > System::block_number());
    });
}

#[test]
fn joint_vote_callback_failure_does_not_cleanup_referendum_votes() {
    new_test_ext().execute_with(|| {
        insert_joint_referendum_proposal(0, 10, 100);
        let voter_subject = test_citizen_subject(&nrc_admin(0));
        joint_vote::ReferendumVotesByCid::<Test>::insert(
            0,
            voter_subject.cid_number.clone(),
            votingengine::CitizenReferendumTicket {
                voter_subject,
                approve: true,
            },
        );
        set_joint_callback_should_fail(true);

        assert_ok!(VotingEngine::set_status_and_emit(0, STATUS_PASSED));
        process_current_block();
        assert_eq!(
            Proposals::<Test>::get(0)
                .expect("proposal should exist")
                .status,
            STATUS_PASSED
        );
        assert!(joint_vote::ReferendumVotesByCid::<Test>::contains_key(
            0,
            test_citizen_subject(&nrc_admin(0)).cid_number
        ));
    });
}

#[test]
fn proposal_finalized_event_uses_status_after_joint_callback_override() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal_for(nrc_admin(0), nrc_cid(), 100);

        set_joint_callback_override_status(Some(STATUS_EXECUTION_FAILED));
        assert_ok!(VotingEngine::set_status_and_emit(
            proposal_id,
            STATUS_PASSED
        ));
        process_current_block();

        let proposal = Proposals::<Test>::get(proposal_id).expect("proposal should exist");
        assert_eq!(proposal.status, STATUS_EXECUTION_FAILED);

        let finalized = System::events()
            .into_iter()
            .rev()
            .find_map(|record| match record.event {
                RuntimeEvent::VotingEngine(votingengine::Event::ProposalFinalized {
                    proposal_id: event_id,
                    status,
                }) if event_id == proposal_id => Some(status),
                _ => None,
            })
            .expect("proposal finalized event should exist");
        assert_eq!(finalized, STATUS_EXECUTION_FAILED);
        let finalized_count = System::events()
            .into_iter()
            .filter(|record| {
                matches!(
                    &record.event,
                    RuntimeEvent::VotingEngine(votingengine::Event::ProposalFinalized {
                        proposal_id: event_id,
                        ..
                    }) if *event_id == proposal_id
                )
            })
            .count();
        assert_eq!(finalized_count, 2);
    });
}

#[test]
fn auto_finalize_drops_failed_joint_callback_from_expiry_bucket() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal_for(nrc_admin(0), nrc_cid(), 66);

        joint_vote::JointTallies::<Test>::insert(
            proposal_id,
            VoteCountU32 {
                yes: primitives::count_const::JOINT_VOTE_TOTAL,
                no: 0,
            },
        );

        let proposal = Proposals::<Test>::get(proposal_id).expect("proposal should exist");
        let expired_at = proposal.end + 1;

        set_joint_callback_should_fail(true);
        System::set_block_number(expired_at);
        <VotingEngine as Hooks<u64>>::on_initialize(expired_at);

        assert_eq!(
            Proposals::<Test>::get(proposal_id)
                .expect("proposal should exist")
                .status,
            STATUS_PASSED
        );
        assert!(votingengine::pallet::PendingProposalExecutions::<Test>::contains_key(proposal_id));
        assert!(PendingExpiryBucket::<Test>::get().is_none());
        assert!(ProposalsByExpiry::<Test>::get(expired_at).is_empty());
    });
}

#[test]
fn auto_finalize_errors_back_off_then_enter_dead_letter_without_starving_hooks() {
    new_test_ext().execute_with(|| {
        reset_internal_callback_state();
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());
        let first_attempt_at = VotingEngine::proposals(proposal_id)
            .expect("proposal should exist")
            .end
            + 1;
        INTERNAL_CALLBACK_SHOULD_FAIL.with(|flag| *flag.borrow_mut() = true);

        System::set_block_number(first_attempt_at);
        <VotingEngine as Hooks<u64>>::on_initialize(first_attempt_at);
        let first = AutoFinalizeRetryStates::<Test>::get(proposal_id)
            .expect("first auto-finalize error should defer");
        assert_eq!(first.attempts, 1);
        assert!(ProposalsByExpiry::<Test>::get(first.next_attempt_at).contains(&proposal_id));

        System::set_block_number(first.next_attempt_at);
        <VotingEngine as Hooks<u64>>::on_initialize(first.next_attempt_at);
        let second = AutoFinalizeRetryStates::<Test>::get(proposal_id)
            .expect("second auto-finalize error should defer");
        assert_eq!(second.attempts, 2);

        System::set_block_number(second.next_attempt_at);
        <VotingEngine as Hooks<u64>>::on_initialize(second.next_attempt_at);

        assert!(AutoFinalizeRetryStates::<Test>::get(proposal_id).is_none());
        assert_eq!(AutoFinalizeDeadLetters::<Test>::get(proposal_id), Some(3));
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("dead-letter proposal remains available for manual finalization")
                .status,
            STATUS_VOTING
        );
        assert!(System::events().into_iter().any(|record| matches!(
            record.event,
            RuntimeEvent::VotingEngine(
                votingengine::Event::ProposalAutoFinalizeDeadLettered {
                    proposal_id: event_id,
                    attempts: 3,
                }
            ) if event_id == proposal_id
        )));
    });
}

#[test]
fn auto_finalize_uses_pending_cursor_when_expiry_bucket_exceeds_per_block_limit() {
    new_test_ext().execute_with(|| {
        let end = 5u64;
        let expiry = end + 1;
        let total = 70u64;
        for proposal_id in 0..total {
            insert_joint_referendum_proposal(proposal_id, 10, end);
            assert_ok!(VotingEngine::schedule_proposal_expiry(proposal_id, end));
        }

        System::set_block_number(6);
        <VotingEngine as Hooks<u64>>::on_initialize(6);
        assert_eq!(ProposalsByExpiry::<Test>::get(expiry).len(), 6);
        assert_eq!(PendingExpiryBucket::<Test>::get(), Some(expiry));

        System::set_block_number(7);
        <VotingEngine as Hooks<u64>>::on_initialize(7);
        assert!(ProposalsByExpiry::<Test>::get(expiry).is_empty());
        assert!(PendingExpiryBucket::<Test>::get().is_none());
        for proposal_id in 0..total {
            assert_eq!(
                Proposals::<Test>::get(proposal_id)
                    .expect("proposal should exist")
                    .status,
                STATUS_REJECTED
            );
        }
    });
}

#[test]
fn schedule_proposal_expiry_rejects_bucket_overflow() {
    new_test_ext().execute_with(|| {
        let end = 5u64;
        for proposal_id in 0..128u64 {
            assert_ok!(VotingEngine::schedule_proposal_expiry(proposal_id, end));
        }

        assert_noop!(
            VotingEngine::schedule_proposal_expiry(999, end),
            votingengine::Error::<Test>::TooManyProposalsAtExpiry
        );
    });
}

#[test]
fn scheduled_cleanup_fifo_activates_all_due_items_without_orphaning() {
    new_test_ext().execute_with(|| {
        let cleanup_block = 77u64;
        for proposal_id in 0..50u64 {
            insert_joint_referendum_proposal(proposal_id, 10, 100);
            ScheduledCleanups::<Test>::insert(
                proposal_id,
                votingengine::ScheduledCleanup {
                    cleanup_at: cleanup_block,
                    proposal_id,
                },
            );
        }
        ScheduledCleanupTail::<Test>::put(50);

        System::set_block_number(cleanup_block);
        <VotingEngine as Hooks<u64>>::on_initialize(cleanup_block);

        assert_eq!(ScheduledCleanupHead::<Test>::get(), 50);
        for proposal_id in 0..50u64 {
            assert!(PendingProposalCleanups::<Test>::contains_key(proposal_id));
        }
    });
}

#[test]
fn pending_cleanup_fifo_rotates_large_and_small_proposals_fairly() {
    new_test_ext().execute_with(|| {
        let large = 40u64;
        let small = 41u64;
        insert_joint_referendum_proposal(large, 10, 100);
        insert_joint_referendum_proposal(small, 10, 100);
        for seed in 1..=5u8 {
            AdminSnapshot::<Test>::insert(
                large,
                ProposalSubject::PersonalAccount(AccountId32::new([seed; 32])),
                BoundedVec::<AccountId32, ConstU32<32>>::default(),
            );
        }
        PendingProposalCleanups::<Test>::insert(large, PendingCleanupStage::AdminSnapshots);
        PendingProposalCleanups::<Test>::insert(small, PendingCleanupStage::AdminSnapshots);
        PendingCleanupQueue::<Test>::insert(0, large);
        PendingCleanupQueue::<Test>::insert(1, small);
        PendingCleanupQueueTail::<Test>::put(2);

        <VotingEngine as Hooks<u64>>::on_initialize(System::block_number());

        assert!(PendingProposalCleanups::<Test>::contains_key(large));
        assert_eq!(
            PendingProposalCleanups::<Test>::get(small),
            Some(PendingCleanupStage::VoterSnapshots)
        );
        assert_eq!(PendingCleanupQueueHead::<Test>::get(), 3);
        assert_eq!(PendingCleanupQueueTail::<Test>::get(), 5);
    });
}

#[test]
fn cleanup_dispatches_only_to_the_proposal_track() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());
        joint_vote::JointVotesByInstitution::<Test>::insert(proposal_id, nrc_cid(), true);
        PendingProposalCleanups::<Test>::insert(proposal_id, PendingCleanupStage::TrackData);
        PendingCleanupQueue::<Test>::insert(0, proposal_id);
        PendingCleanupQueueTail::<Test>::put(1);

        <VotingEngine as Hooks<u64>>::on_initialize(System::block_number());

        assert!(Proposals::<Test>::get(proposal_id).is_none());
        assert!(joint_vote::JointVotesByInstitution::<Test>::contains_key(
            proposal_id,
            nrc_cid()
        ));
    });
}

#[test]
fn schedule_cleanup_returns_error_when_fifo_sequence_is_exhausted() {
    new_test_ext().execute_with(|| {
        let now = System::block_number();
        exhaust_cleanup_sequence(now);

        assert_noop!(
            votingengine::cleanup::schedule_cleanup::<Test>(9_999, now),
            votingengine::Error::<Test>::CleanupQueueSequenceExhausted
        );
    });
}

#[test]
fn terminal_status_rolls_back_when_cleanup_cannot_be_scheduled() {
    new_test_ext().execute_with(|| {
        let proposal_id = 9_999u64;
        let now = System::block_number();
        insert_joint_referendum_proposal(proposal_id, 10, now + 100);
        exhaust_cleanup_sequence(now);

        assert_noop!(
            VotingEngine::set_status_and_emit(proposal_id, STATUS_REJECTED),
            votingengine::Error::<Test>::CleanupQueueSequenceExhausted
        );
        assert_eq!(
            Proposals::<Test>::get(proposal_id)
                .expect("proposal should remain")
                .status,
            STATUS_VOTING
        );
        assert!(PendingProposalCleanups::<Test>::get(proposal_id).is_none());
    });
}

#[test]
fn retry_deadline_keeps_retry_state_when_cleanup_scheduling_fails() {
    new_test_ext().execute_with(|| {
        reset_internal_callback_state();
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());

        assert_ok!(VotingEngine::set_status_and_emit(
            proposal_id,
            STATUS_PASSED
        ));
        process_current_block();
        let deadline = ProposalExecutionRetryStates::<Test>::get(proposal_id)
            .expect("retry state should exist")
            .retry_deadline;
        exhaust_cleanup_sequence(deadline);

        System::set_block_number(deadline);
        <VotingEngine as Hooks<u64>>::on_initialize(deadline);

        assert_eq!(
            Proposals::<Test>::get(proposal_id)
                .expect("proposal should remain")
                .status,
            STATUS_PASSED
        );
        assert!(ProposalExecutionRetryStates::<Test>::get(proposal_id).is_some());
        assert!(
            ExecutionRetryDeadlines::<Test>::get(deadline + 1).contains(&proposal_id),
            "cleanup scheduling failure must requeue retry deadline instead of losing it"
        );
    });
}

#[test]
fn retry_deadline_enters_pending_queue_when_reschedule_window_is_full() {
    new_test_ext().execute_with(|| {
        reset_internal_callback_state();
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());

        assert_ok!(VotingEngine::set_status_and_emit(
            proposal_id,
            STATUS_PASSED
        ));
        process_current_block();
        let deadline = ProposalExecutionRetryStates::<Test>::get(proposal_id)
            .expect("retry state should exist")
            .retry_deadline;
        exhaust_cleanup_sequence(deadline);
        fill_retry_deadline_window(deadline + 1);

        System::set_block_number(deadline);
        <VotingEngine as Hooks<u64>>::on_initialize(deadline);

        assert!(ProposalExecutionRetryStates::<Test>::get(proposal_id).is_some());
        assert_eq!(
            PendingExecutionRetryExpirations::<Test>::get(proposal_id),
            Some(deadline)
        );
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal should remain")
                .status,
            STATUS_PASSED
        );

        reset_cleanup_sequence(deadline + 1);
        clear_retry_deadline_window(deadline + 1);
        System::set_block_number(deadline + 1);
        <VotingEngine as Hooks<u64>>::on_initialize(deadline + 1);

        assert!(PendingExecutionRetryExpirations::<Test>::get(proposal_id).is_none());
        assert!(ProposalExecutionRetryStates::<Test>::get(proposal_id).is_none());
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal should exist")
                .status,
            STATUS_EXECUTION_FAILED
        );
    });
}

#[test]
fn delayed_cleanup_chunks_cleanup_across_blocks() {
    new_test_ext().execute_with(|| {
        let proposal_id = 42u64;
        let referendum_accounts = [
            AccountId32::new([201u8; 32]),
            AccountId32::new([202u8; 32]),
            AccountId32::new([203u8; 32]),
        ];

        insert_joint_referendum_proposal(proposal_id, 10, 100);
        joint_vote::JointVotesByInstitution::<Test>::insert(proposal_id, nrc_cid(), true);
        joint_vote::JointVotesByInstitution::<Test>::insert(proposal_id, prc_cid(), true);
        joint_vote::JointVotesByInstitution::<Test>::insert(proposal_id, prb_cid(), true);
        for account in referendum_accounts.iter() {
            let voter_subject = test_citizen_subject(account);
            joint_vote::ReferendumVotesByCid::<Test>::insert(
                proposal_id,
                voter_subject.cid_number.clone(),
                votingengine::CitizenReferendumTicket {
                    voter_subject,
                    approve: true,
                },
            );
        }

        // 投票通过后由 callback 返回 Executed，终态会注册 90 天后清理。
        assert_ok!(VotingEngine::set_status_and_emit(
            proposal_id,
            STATUS_PASSED
        ));
        process_current_block();
        // 此时 PendingProposalCleanups 尚未设置（要等 90 天后延迟 FIFO 激活）
        assert!(PendingProposalCleanups::<Test>::get(proposal_id).is_none());

        // set_status_and_emit 在 block 0 调用，cleanup_at = 0 + retention
        let retention = 90u64 * primitives::pow_const::BLOCKS_PER_DAY;
        let cleanup_block = retention;
        // 运行多轮 on_initialize 直到清理完成
        for i in 0..20u64 {
            System::set_block_number(cleanup_block + i);
            <VotingEngine as Hooks<u64>>::on_initialize(cleanup_block + i);
            if PendingProposalCleanups::<Test>::get(proposal_id).is_none()
                && Proposals::<Test>::get(proposal_id).is_none()
            {
                break;
            }
        }

        assert!(PendingProposalCleanups::<Test>::get(proposal_id).is_none());
        for account in referendum_accounts.iter() {
            assert!(!joint_vote::ReferendumVotesByCid::<Test>::contains_key(
                proposal_id,
                test_citizen_subject(account).cid_number
            ));
        }
    });
}

// ──── 公开 internal_vote extrinsic + InternalVoteResultCallback ────

/// 重置内部投票回调测试桩状态,避免用例间污染。
fn reset_internal_callback_state() {
    INTERNAL_CALLBACK_SHOULD_FAIL.with(|flag| *flag.borrow_mut() = false);
    INTERNAL_CALLBACK_OVERRIDE_STATUS.with(|value| *value.borrow_mut() = None);
    INTERNAL_CALLBACK_LOG.with(|log| log.borrow_mut().clear());
    INTERNAL_TERMINAL_CLEANUP_LOG.with(|log| log.borrow_mut().clear());
    set_internal_terminal_cleanup_should_fail(false);
}

#[test]
fn internal_vote_public_call_casts_vote() {
    new_test_ext().execute_with(|| {
        reset_internal_callback_state();
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());

        assert_ok!(cast_internal_vote_via_extrinsic(
            nrc_admin(1),
            proposal_id,
            true
        ));

        let ticket = InternalVoteTicket::Institution(votingengine::InstitutionVoteTicket {
            role_subject: RoleSubject {
                cid_number: nrc_cid(),
                role_code: test_institution_role(NRC)
                    .to_vec()
                    .try_into()
                    .expect("测试岗位码合法"),
            },
            voter_account_id: nrc_admin(0),
        });
        assert!(InternalVotesByTicket::<Test>::contains_key(
            proposal_id,
            ticket
        ));
        assert_eq!(InternalTallies::<Test>::get(proposal_id).yes, 2);
        assert_eq!(InternalTallies::<Test>::get(proposal_id).no, 0);
    });
}

#[test]
fn internal_vote_counts_same_account_id_once_per_role_ticket() {
    new_test_ext().execute_with(|| {
        reset_internal_callback_state();
        // 两个岗位各 3 席，共 6 张票；动态机构阈值必须为严格多数 4。
        set_institution_threshold(public_cid(), 4);
        let data = b"multi-role-ticket".to_vec();
        let owner: BoundedVec<u8, ConstU32<32>> = b"test".to_vec().try_into().unwrap();
        let primary_role: votingengine::types::RoleCode = b"TEST_ROLE".to_vec().try_into().unwrap();
        let second_role: votingengine::types::RoleCode =
            b"TEST_SECOND_ROLE".to_vec().try_into().unwrap();
        let primary_subject = RoleSubject {
            cid_number: public_cid(),
            role_code: primary_role.clone(),
        };
        let second_subject = RoleSubject {
            cid_number: public_cid(),
            role_code: second_role.clone(),
        };
        let vote_plan = votingengine::types::VotePlanOf::try_new(
            votingengine::types::BusinessActionId {
                module_tag: owner.clone(),
                action_code: 0,
            },
            owner,
            AuthorizationSubject::Institution(primary_subject.clone()),
            vec![
                AuthorizationSubject::Institution(primary_subject.clone()),
                AuthorizationSubject::Institution(second_subject.clone()),
            ],
            votingengine::types::VotingEngineKind::Internal,
            sp_io::hashing::blake2_256(&data),
        )
        .unwrap();
        let proposal_id = <InternalVote as InternalVoteEngine<AccountId32>>::
            create_institution_proposal_with_data(
                test_institution_admin(0),
                PUBLIC_CODE,
                public_cid().to_vec(),
                None,
                subject_cids_for(&public_cid()),
                vote_plan,
                data,
            )
            .unwrap();

        // 建案自动使用主岗位投出第一张票；同一账户再以第二岗位投第二张票。
        assert_eq!(InternalTallies::<Test>::get(proposal_id).yes, 1);
        assert_ok!(InternalVote::cast(
            RuntimeOrigin::signed(test_institution_admin(0)),
            proposal_id,
            InternalVoteTicketClaim::InstitutionRole(second_role.clone()),
            true,
        ));
        assert_eq!(InternalTallies::<Test>::get(proposal_id).yes, 2);
        for subject in [primary_subject, second_subject] {
            assert!(InternalVotesByTicket::<Test>::contains_key(
                proposal_id,
                InternalVoteTicket::Institution(votingengine::InstitutionVoteTicket {
                    role_subject: subject,
                    voter_account_id: test_institution_admin(0),
                })
            ));
        }
        assert_noop!(
            InternalVote::cast(
                RuntimeOrigin::signed(test_institution_admin(0)),
                proposal_id,
                InternalVoteTicketClaim::InstitutionRole(second_role),
                true,
            ),
            votingengine::Error::<Test>::AlreadyVoted
        );
    });
}

#[test]
fn rebound_wallet_votes_as_canonical_and_old_wallet_loses_power() {
    new_test_ext().execute_with(|| {
        // 本用例只验证换绑后的岗位投票授权迁移；CID 换绑本身不创建或消费投票。
        reset_internal_callback_state();
        // 岗位快照 3 席（admin(0..2)），阈值 3：中途不会提前达标终结，便于逐条断言。
        set_institution_threshold(public_cid(), 3);
        let data = b"rebind-vote".to_vec();
        let owner: BoundedVec<u8, ConstU32<32>> = b"test".to_vec().try_into().unwrap();
        let role: votingengine::types::RoleCode = b"TEST_ROLE".to_vec().try_into().unwrap();
        let subject = RoleSubject {
            cid_number: public_cid(),
            role_code: role.clone(),
        };
        let vote_plan = votingengine::types::VotePlanOf::try_new(
            votingengine::types::BusinessActionId {
                module_tag: owner.clone(),
                action_code: 0,
            },
            owner,
            AuthorizationSubject::Institution(subject.clone()),
            vec![AuthorizationSubject::Institution(subject.clone())],
            votingengine::types::VotingEngineKind::Internal,
            sp_io::hashing::blake2_256(&data),
        )
        .unwrap();
        let proposal_id = <InternalVote as InternalVoteEngine<AccountId32>>::
            create_institution_proposal_with_data(
                test_institution_admin(0),
                PUBLIC_CODE,
                public_cid().to_vec(),
                None,
                subject_cids_for(&public_cid()),
                vote_plan,
                data,
            )
            .unwrap();
        // 建案自动由 admin(0) 投出第一票。
        assert_eq!(InternalTallies::<Test>::get(proposal_id).yes, 1);

        // admin(1) 的公民 CID 换绑到新钱包（快照仍冻结着规范账户 admin(1)）。
        let rebound_wallet = AccountId32::new([200u8; 32]);
        set_rebind(test_institution_admin(1), rebound_wallet.clone());

        // 换绑不掉权：新钱包解析到规范账户 admin(1)，命中快照 → 可投。
        assert_ok!(InternalVote::cast(
            RuntimeOrigin::signed(rebound_wallet.clone()),
            proposal_id,
            InternalVoteTicketClaim::InstitutionRole(role.clone()),
            true,
        ));
        assert_eq!(InternalTallies::<Test>::get(proposal_id).yes, 2);
        // 票据按【规范账户】记账而非投票钱包——这正是防双投的关键。
        assert!(InternalVotesByTicket::<Test>::contains_key(
            proposal_id,
            InternalVoteTicket::Institution(votingengine::InstitutionVoteTicket {
                role_subject: subject.clone(),
                voter_account_id: test_institution_admin(1),
            })
        ));
        assert!(!InternalVotesByTicket::<Test>::contains_key(
            proposal_id,
            InternalVoteTicket::Institution(votingengine::InstitutionVoteTicket {
                role_subject: subject.clone(),
                voter_account_id: rebound_wallet.clone(),
            })
        ));

        // 旧钱包掉权：CID 已不再绑定它，解析为 None。
        assert_noop!(
            InternalVote::cast(
                RuntimeOrigin::signed(test_institution_admin(1)),
                proposal_id,
                InternalVoteTicketClaim::InstitutionRole(role.clone()),
                true,
            ),
            votingengine::Error::<Test>::NoPermission
        );

        // 新钱包再投：票据已存在 → 双投窗口已闭合。
        assert_noop!(
            InternalVote::cast(
                RuntimeOrigin::signed(rebound_wallet),
                proposal_id,
                InternalVoteTicketClaim::InstitutionRole(role),
                true,
            ),
            votingengine::Error::<Test>::AlreadyVoted
        );
    });
}

#[test]
fn internal_vote_rejects_non_admin() {
    new_test_ext().execute_with(|| {
        reset_internal_callback_state();
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());

        // 非 NRC 管理员(比如 PRB 的管理员)不能投 NRC 的内部提案。
        assert_noop!(
            cast_internal_vote_via_extrinsic(prb_admin(0), proposal_id, true),
            votingengine::Error::<Test>::NoPermission
        );
        assert_eq!(InternalTallies::<Test>::get(proposal_id).yes, 1);
    });
}

#[test]
fn internal_vote_rejects_double_vote() {
    new_test_ext().execute_with(|| {
        reset_internal_callback_state();
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());
        assert_noop!(
            cast_internal_vote_via_extrinsic(nrc_admin(0), proposal_id, false),
            votingengine::Error::<Test>::AlreadyVoted
        );
    });
}

#[test]
fn internal_vote_passes_triggers_callback_approved_true() {
    new_test_ext().execute_with(|| {
        reset_internal_callback_state();
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());

        // NRC 阈值 13 票;投 13 票赞成使提案进入 STATUS_PASSED。
        for i in 1..13 {
            assert_ok!(cast_internal_vote_via_extrinsic(
                nrc_admin(i),
                proposal_id,
                true
            ));
        }

        // 回调被触发且 approved = true。
        let log = INTERNAL_CALLBACK_LOG.with(|log| log.borrow().clone());
        assert_eq!(log, vec![(proposal_id, true)]);
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal exists")
                .status,
            STATUS_PASSED
        );
    });
}

#[test]
fn internal_vote_early_rejection_triggers_callback_approved_false() {
    new_test_ext().execute_with(|| {
        reset_internal_callback_state();
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());

        // NRC 总管理员 19 人,阈值 13 票。7 票反对 → 剩余 12 人全同意也到不了 13,
        // 触发提前否决。
        for i in 1..8 {
            assert_ok!(cast_internal_vote_via_extrinsic(
                nrc_admin(i),
                proposal_id,
                false
            ));
        }

        // 回调被触发且 approved = false。
        let log = INTERNAL_CALLBACK_LOG.with(|log| log.borrow().clone());
        assert_eq!(log, vec![(proposal_id, false)]);
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal exists")
                .status,
            STATUS_REJECTED
        );
    });
}

#[test]
fn internal_vote_callback_not_called_before_threshold() {
    new_test_ext().execute_with(|| {
        reset_internal_callback_state();
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());

        // 投 12 票赞成(阈值 13),未达阈值不应触发回调。
        for i in 1..12 {
            assert_ok!(cast_internal_vote_via_extrinsic(
                nrc_admin(i),
                proposal_id,
                true
            ));
        }

        let log = INTERNAL_CALLBACK_LOG.with(|log| log.borrow().clone());
        assert!(log.is_empty(), "未达阈值回调不应被调用: {:?}", log);
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal exists")
                .status,
            STATUS_VOTING
        );
    });
}

#[test]
fn internal_vote_callback_err_defers_execution_without_reverting_vote() {
    new_test_ext().execute_with(|| {
        reset_internal_callback_state();
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());

        // 前 12 票赞成(未达阈值,不触发回调,不受 SHOULD_FAIL 影响)。
        for i in 1..12 {
            assert_ok!(cast_internal_vote_via_extrinsic(
                nrc_admin(i),
                proposal_id,
                true
            ));
        }

        // 第 13 票达阈值后先提交投票结果；异步回调失败只进入退避重试。
        INTERNAL_CALLBACK_SHOULD_FAIL.with(|flag| *flag.borrow_mut() = true);
        assert_ok!(cast_internal_vote_via_extrinsic(
            nrc_admin(12),
            proposal_id,
            true
        ));

        // 投票判定不可被业务执行故障撤销；最后一票和 PASSED 状态都保留。
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal exists")
                .status,
            STATUS_PASSED
        );
        assert_eq!(InternalTallies::<Test>::get(proposal_id).yes, 13);
        let ticket = InternalVoteTicket::Institution(votingengine::InstitutionVoteTicket {
            role_subject: RoleSubject {
                cid_number: nrc_cid(),
                role_code: test_institution_role(NRC)
                    .to_vec()
                    .try_into()
                    .expect("测试岗位码合法"),
            },
            voter_account_id: nrc_admin(12),
        });
        assert!(InternalVotesByTicket::<Test>::contains_key(
            proposal_id,
            ticket
        ));
        let pending = votingengine::pallet::PendingProposalExecutions::<Test>::get(proposal_id)
            .expect("failed callback should remain queued");
        assert_eq!(pending.attempts, 1);
    });
}

#[test]
fn asynchronous_callback_errors_dead_letter_after_bounded_retries() {
    new_test_ext().execute_with(|| {
        reset_internal_callback_state();
        INTERNAL_CALLBACK_SHOULD_FAIL.with(|flag| *flag.borrow_mut() = true);
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());

        assert_ok!(VotingEngine::set_status_and_emit(
            proposal_id,
            STATUS_PASSED
        ));
        process_current_block();
        let second_attempt_at =
            votingengine::pallet::PendingProposalExecutions::<Test>::get(proposal_id)
                .expect("first failure should defer")
                .next_attempt_at;

        System::set_block_number(second_attempt_at);
        process_current_block();
        let third_attempt_at =
            votingengine::pallet::PendingProposalExecutions::<Test>::get(proposal_id)
                .expect("second failure should defer")
                .next_attempt_at;

        System::set_block_number(third_attempt_at);
        process_current_block();

        assert!(
            votingengine::pallet::PendingProposalExecutions::<Test>::get(proposal_id).is_none()
        );
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal should remain until retention cleanup")
                .status,
            STATUS_EXECUTION_FAILED
        );
        assert!(System::events().into_iter().any(|record| matches!(
            record.event,
            RuntimeEvent::VotingEngine(votingengine::Event::ProposalExecutionDeadLettered {
                proposal_id: event_id,
                attempts: 3,
            }) if event_id == proposal_id
        )));
    });
}

#[test]
fn ignored_callback_outcome_dead_letters_after_bounded_retries() {
    new_test_ext().execute_with(|| {
        reset_internal_callback_state();
        // 未识别状态映射为 Ignored，复现结果应用阶段返回 Err 的确定性失败。
        set_internal_callback_override_status(Some(0xff));
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());

        assert_ok!(VotingEngine::set_status_and_emit(
            proposal_id,
            STATUS_PASSED
        ));
        process_current_block();
        let second_attempt_at =
            votingengine::pallet::PendingProposalExecutions::<Test>::get(proposal_id)
                .expect("Ignored first failure should defer")
                .next_attempt_at;

        System::set_block_number(second_attempt_at);
        process_current_block();
        let third_attempt_at =
            votingengine::pallet::PendingProposalExecutions::<Test>::get(proposal_id)
                .expect("Ignored second failure should defer")
                .next_attempt_at;

        System::set_block_number(third_attempt_at);
        process_current_block();

        assert!(
            votingengine::pallet::PendingProposalExecutions::<Test>::get(proposal_id).is_none(),
            "业务执行达到上限后必须永久停止"
        );
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal should remain until retention cleanup")
                .status,
            STATUS_EXECUTION_FAILED
        );
        assert!(System::events().into_iter().any(|record| matches!(
            record.event,
            RuntimeEvent::VotingEngine(votingengine::Event::ProposalExecutionDeadLettered {
                proposal_id: event_id,
                attempts: 3,
            }) if event_id == proposal_id
        )));
    });
}

#[test]
fn orphan_pending_execution_is_removed_instead_of_retried() {
    new_test_ext().execute_with(|| {
        let proposal_id = 9_999;
        votingengine::pallet::PendingProposalExecutions::<Test>::insert(
            proposal_id,
            votingengine::PendingExecutionState {
                attempts: 0,
                next_attempt_at: System::block_number(),
            },
        );

        process_current_block();

        assert!(
            votingengine::pallet::PendingProposalExecutions::<Test>::get(proposal_id).is_none(),
            "不存在提案的孤儿队列项不得永久消耗执行预算"
        );
    });
}

#[test]
fn manual_retry_third_failure_marks_execution_failed() {
    new_test_ext().execute_with(|| {
        reset_internal_callback_state();
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());

        assert_ok!(VotingEngine::set_status_and_emit(
            proposal_id,
            STATUS_PASSED
        ));
        process_current_block();
        assert_eq!(
            ProposalExecutionRetryStates::<Test>::get(proposal_id)
                .expect("retry state should exist")
                .manual_attempts,
            0
        );

        assert_ok!(VotingEngine::retry_passed_proposal(
            RuntimeOrigin::signed(nrc_admin(0)),
            proposal_id
        ));
        assert_eq!(
            ProposalExecutionRetryStates::<Test>::get(proposal_id)
                .expect("retry state should remain")
                .manual_attempts,
            1
        );
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal exists")
                .status,
            STATUS_PASSED
        );

        assert_ok!(VotingEngine::retry_passed_proposal(
            RuntimeOrigin::signed(nrc_admin(1)),
            proposal_id
        ));
        assert_eq!(
            ProposalExecutionRetryStates::<Test>::get(proposal_id)
                .expect("retry state should remain")
                .manual_attempts,
            2
        );

        assert_ok!(VotingEngine::retry_passed_proposal(
            RuntimeOrigin::signed(nrc_admin(2)),
            proposal_id
        ));
        assert!(ProposalExecutionRetryStates::<Test>::get(proposal_id).is_none());
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal exists")
                .status,
            STATUS_EXECUTION_FAILED
        );

        let third_retry_outcome = System::events()
            .into_iter()
            .rev()
            .find_map(|record| match record.event {
                RuntimeEvent::VotingEngine(votingengine::Event::ProposalExecutionRetried {
                    proposal_id: event_id,
                    manual_attempts: 3,
                    outcome,
                }) if event_id == proposal_id => Some(outcome),
                _ => None,
            })
            .expect("third retry event should exist");
        assert_eq!(third_retry_outcome, STATUS_EXECUTION_FAILED);
    });
}

#[test]
fn default_cancel_callback_rejects_passed_retry_proposal() {
    new_test_ext().execute_with(|| {
        reset_internal_callback_state();
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());

        assert_ok!(VotingEngine::set_status_and_emit(
            proposal_id,
            STATUS_PASSED
        ));
        process_current_block();
        assert_noop!(
            VotingEngine::cancel_passed_proposal(
                RuntimeOrigin::signed(nrc_admin(0)),
                proposal_id,
                b"not allowed"
                    .to_vec()
                    .try_into()
                    .expect("reason should fit")
            ),
            votingengine::Error::<Test>::ProposalCancellationNotAllowed
        );
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal exists")
                .status,
            STATUS_PASSED
        );
    });
}

#[test]
fn automatic_fatal_failed_runs_execution_failed_terminal_hook() {
    new_test_ext().execute_with(|| {
        reset_internal_callback_state();
        set_internal_callback_override_status(Some(STATUS_EXECUTION_FAILED));
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());

        assert_ok!(VotingEngine::set_status_and_emit(
            proposal_id,
            STATUS_PASSED
        ));
        process_current_block();
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal exists")
                .status,
            STATUS_EXECUTION_FAILED
        );
        let cleanup_log = INTERNAL_TERMINAL_CLEANUP_LOG.with(|log| log.borrow().clone());
        assert_eq!(cleanup_log, vec![proposal_id]);
    });
}

#[test]
fn execution_failed_terminal_cleanup_error_is_queued_and_retried() {
    new_test_ext().execute_with(|| {
        reset_internal_callback_state();
        set_internal_callback_override_status(Some(STATUS_EXECUTION_FAILED));
        set_internal_terminal_cleanup_should_fail(true);
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());

        // 终态可以成立，但业务侧执行失败清理通知失败时必须留下重试入口。
        assert_ok!(VotingEngine::set_status_and_emit(
            proposal_id,
            STATUS_PASSED
        ));
        process_current_block();
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal exists")
                .status,
            STATUS_EXECUTION_FAILED
        );
        assert!(PendingTerminalCleanups::<Test>::contains_key(proposal_id));
        assert!(INTERNAL_TERMINAL_CLEANUP_LOG.with(|log| log.borrow().is_empty()));

        set_internal_terminal_cleanup_should_fail(false);
        System::set_block_number(2);
        <VotingEngine as Hooks<u64>>::on_initialize(2);

        assert!(!PendingTerminalCleanups::<Test>::contains_key(proposal_id));
        let cleanup_log = INTERNAL_TERMINAL_CLEANUP_LOG.with(|log| log.borrow().clone());
        assert_eq!(cleanup_log, vec![proposal_id]);
        assert!(System::events().into_iter().any(|record| matches!(
            record.event,
            RuntimeEvent::VotingEngine(votingengine::Event::ProposalTerminalCleanupCompleted {
                proposal_id: event_id
            }) if event_id == proposal_id
        )));
    });
}

#[test]
fn joint_retryable_outcome_is_forced_to_execution_failed() {
    new_test_ext().execute_with(|| {
        set_joint_callback_override_status(Some(STATUS_PASSED));
        let proposal_id = create_joint_proposal_for(nrc_admin(0), nrc_cid(), 10);

        assert_ok!(VotingEngine::set_status_and_emit(
            proposal_id,
            STATUS_PASSED
        ));
        process_current_block();
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal exists")
                .status,
            STATUS_EXECUTION_FAILED
        );
        assert!(
            ProposalExecutionRetryStates::<Test>::get(proposal_id).is_none(),
            "joint proposal must not enter internal retry state"
        );
    });
}

#[test]
fn execution_retry_deadline_expires_to_execution_failed() {
    new_test_ext().execute_with(|| {
        reset_internal_callback_state();
        let proposal_id = create_internal_proposal_via_engine(nrc_admin(0), NRC, nrc_cid());

        assert_ok!(VotingEngine::set_status_and_emit(
            proposal_id,
            STATUS_PASSED
        ));
        process_current_block();
        let deadline = ProposalExecutionRetryStates::<Test>::get(proposal_id)
            .expect("retry state should exist")
            .retry_deadline;

        System::set_block_number(deadline);
        <VotingEngine as Hooks<u64>>::on_initialize(deadline);

        assert!(ProposalExecutionRetryStates::<Test>::get(proposal_id).is_none());
        assert_eq!(
            VotingEngine::proposals(proposal_id)
                .expect("proposal exists")
                .status,
            STATUS_EXECUTION_FAILED
        );
        assert!(System::events().into_iter().any(|record| matches!(
            record.event,
            RuntimeEvent::VotingEngine(votingengine::Event::ProposalExecutionRetryExpired {
                proposal_id: event_id
            }) if event_id == proposal_id
        )));
    });
}

#[test]
fn internal_vote_rejects_wrong_stage_joint_proposal() {
    new_test_ext().execute_with(|| {
        reset_internal_callback_state();
        // 手工写一个 kind=JOINT 的提案,用 internal_vote 去投 → 应拒绝。
        let proposal_id = 999u64;
        let now = <frame_system::Pallet<Test>>::block_number();
        Proposals::<Test>::insert(
            proposal_id,
            Proposal {
                kind: PROPOSAL_KIND_JOINT,
                stage: STAGE_JOINT,
                status: STATUS_VOTING,
                internal_code: None,
                actor_cid_number: Some(
                    CHINA_CB[0]
                        .cid_number
                        .as_bytes()
                        .to_vec()
                        .try_into()
                        .expect("NRC CID fits runtime bound"),
                ),
                execution_account_id: None,
                subject_cid_numbers: Default::default(),
                start: now,
                end: now + 100,
            },
        );

        assert_noop!(
            cast_internal_vote_via_extrinsic(nrc_admin(0), proposal_id, true),
            votingengine::Error::<Test>::InvalidProposalKind
        );
    });
}
