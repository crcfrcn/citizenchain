use super::*;

#[test]
fn joint_proposers_can_propose_runtime_upgrade() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            RuntimeUpgrade::propose_runtime_upgrade(
                RuntimeOrigin::signed(outsider()),
                nrc_cid(),
                committee_role(),
                reason_ok(),
                code_ok(),
                pow_difficulty::PowDifficultyParams::genesis_default()
            ),
            sp_runtime::DispatchError::BadOrigin
        );

        assert_ok!(RuntimeUpgrade::propose_runtime_upgrade(
            RuntimeOrigin::signed(nrc_admin()),
            nrc_cid(),
            committee_role(),
            reason_ok(),
            code_ok(),
            pow_difficulty::PowDifficultyParams::genesis_default()
        ));

        assert_ok!(RuntimeUpgrade::propose_runtime_upgrade(
            RuntimeOrigin::signed(prc_admin()),
            prc_cid(),
            committee_role(),
            reason_ok(),
            code_ok(),
            pow_difficulty::PowDifficultyParams::genesis_default()
        ));

        assert!(
            votingengine::Pallet::<Test>::get_proposal_data(100).is_some(),
            "NRC proposer_account_id should create proposal data"
        );
        assert!(
            votingengine::Pallet::<Test>::get_proposal_data(101).is_some(),
            "PRC proposer_account_id should create proposal data"
        );
        for proposal_id in [100, 101] {
            let plan = votingengine::ProposalVotePlans::<Test>::get(proposal_id)
                .expect("runtime upgrade must bind vote plan");
            assert_eq!(
                plan.business_action_id.action_code,
                entity_primitives::business_action::ACTION_RUNTIME_UPGRADE
            );
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
        }
    });
}

#[test]
fn administrator_without_committee_role_cannot_propose_runtime_upgrade() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            RuntimeUpgrade::propose_runtime_upgrade(
                RuntimeOrigin::signed(ordinary_staff_admin()),
                nrc_cid(),
                committee_role(),
                reason_ok(),
                code_ok(),
                pow_difficulty::PowDifficultyParams::genesis_default()
            ),
            pallet::Error::<Test>::UnauthorizedActorRole
        );
    });
}

#[test]
fn proposal_data_stored_in_votingengine() {
    new_test_ext().execute_with(|| {
        propose_ok();
        // proposal_id comes from NEXT_JOINT_ID which starts at 100
        assert!(
            votingengine::Pallet::<Test>::get_proposal_data(100).is_some(),
            "proposal data should be stored in voting engine"
        );
        let proposal = decode_proposal(100);
        assert_eq!(proposal.proposer_account_id, nrc_admin());
        assert!(
            votingengine::Pallet::<Test>::get_proposal_object(100).is_some(),
            "runtime wasm should be stored in proposal object layer"
        );
    });
}

#[test]
fn rejected_joint_vote_marks_proposal_rejected() {
    new_test_ext().execute_with(|| {
        propose_ok();
        insert_engine_proposal_with_status(100, votingengine::STATUS_REJECTED);
        // proposal_id == joint_vote_id == 100
        let outcome = call_joint_callback(100, false).expect("callback should succeed");
        assert_eq!(outcome, votingengine::ProposalExecutionOutcome::Executed);
        assert!(votingengine::Pallet::<Test>::get_proposal_data(100).is_some());
    });
}

#[test]
fn approved_joint_vote_executes_runtime_upgrade() {
    new_test_ext().execute_with(|| {
        propose_ok();
        insert_engine_proposal(100);
        assert_ok!(call_joint_callback(100, true));

        assert_eq!(
            decode_proposal(100).code_hash,
            <Test as frame_system::Config>::Hashing::hash(&code_ok())
        );
        assert!(
            votingengine::Pallet::<Test>::get_proposal_object(100).is_some(),
            "approved proposal should still keep object data for unified cleanup"
        );
        let code_executed = RUNTIME_CODE_EXECUTED.with(|v| *v.borrow());
        assert!(code_executed, "runtime code executor should be called");
    });
}

#[test]
fn approved_referendum_executes_runtime_upgrade() {
    new_test_ext().execute_with(|| {
        propose_ok();
        insert_engine_proposal_with_stage_and_status(
            100,
            votingengine::STAGE_REFERENDUM,
            votingengine::STATUS_PASSED,
        );
        assert_ok!(call_joint_callback(100, true));
        assert!(
            RUNTIME_CODE_EXECUTED.with(|executed| *executed.borrow()),
            "联合公投通过后必须继续执行绑定的 runtime code"
        );
    });
}

#[test]
fn approved_upgrade_atomically_stages_versioned_pow_params() {
    new_test_ext().execute_with(|| {
        System::set_block_number(5);
        let mut next = pow_difficulty::PowDifficultyParams::genesis_default();
        next.params_version += 1;
        next.adjustment_interval = 20;
        next.target_block_time_ms = 120_000;
        assert_ok!(RuntimeUpgrade::propose_runtime_upgrade(
            RuntimeOrigin::signed(nrc_admin()),
            nrc_cid(),
            committee_role(),
            reason_ok(),
            code_ok(),
            next,
        ));
        insert_engine_proposal(100);
        assert_ok!(call_joint_callback(100, true));

        assert_eq!(
            pow_difficulty::PendingParams::<Test>::get(),
            Some(pow_difficulty::PendingPowDifficultyParams {
                params: next,
                activate_at: 6,
            })
        );
        let audit = pallet::LastRuntimeUpgradeAudit::<Test>::get().expect("upgrade audit");
        assert_eq!(audit.proposal_id, Some(100));
        assert_eq!(audit.executed_at, 5);
        assert_eq!(audit.activate_at, 6);
    });
}

#[test]
fn approved_joint_vote_execution_failure_emits_event() {
    new_test_ext().execute_with(|| {
        propose_ok();
        insert_engine_proposal(100);
        EXEC_SHOULD_FAIL.with(|v| *v.borrow_mut() = true);

        assert_ok!(call_joint_callback(100, true));

        assert_eq!(decode_proposal(100).proposer_account_id, nrc_admin());
        let code_executed = RUNTIME_CODE_EXECUTED.with(|v| *v.borrow());
        assert!(
            !code_executed,
            "runtime code executor should fail in this test"
        );
        // 投票引擎侧应为 STATUS_EXECUTION_FAILED
        let engine_proposal = votingengine::pallet::Proposals::<Test>::get(100).unwrap();
        assert_eq!(
            engine_proposal.status,
            votingengine::STATUS_EXECUTION_FAILED
        );
    });
}

#[test]
fn rejected_joint_vote_retains_object_for_unified_cleanup() {
    new_test_ext().execute_with(|| {
        propose_ok();
        insert_engine_proposal_with_status(100, votingengine::STATUS_REJECTED);
        assert_ok!(call_joint_callback(100, false));

        assert!(votingengine::Pallet::<Test>::get_proposal_data(100).is_some());
        assert!(
            votingengine::Pallet::<Test>::get_proposal_object(100).is_some(),
            "rejected proposal object should stay until unified cleanup"
        );
    });
}

#[test]
fn owns_proposal_returns_true_for_own_proposals() {
    new_test_ext().execute_with(|| {
        propose_ok();
        assert!(pallet::Pallet::<Test>::owns_proposal(100));
        assert!(!pallet::Pallet::<Test>::owns_proposal(999));
    });
}

#[test]
fn approved_success_marks_engine_status_executed() {
    new_test_ext().execute_with(|| {
        propose_ok();
        insert_engine_proposal(100);
        assert_ok!(call_joint_callback(100, true));

        // 执行成功时在回调作用域内静默写入 EXECUTED，最终事件由投票引擎外层发出。
        let engine_proposal = votingengine::pallet::Proposals::<Test>::get(100).unwrap();
        assert_eq!(
            engine_proposal.status,
            votingengine::STATUS_EXECUTED,
            "success path should mark engine status executed"
        );
    });
}

#[test]
fn joint_vote_callback_requires_voting_status() {
    new_test_ext().execute_with(|| {
        propose_ok();
        insert_engine_proposal(100);
        // First finalize
        assert_ok!(call_joint_callback(100, true));
        // Second finalize should fail - no longer voting
        assert_noop!(
            call_joint_callback(100, true),
            pallet::Error::<Test>::ProposalNotVoting
        );
    });
}

#[test]
fn finalize_nonexistent_proposal_fails() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            call_joint_callback(999, true),
            pallet::Error::<Test>::ProposalNotFound
        );
    });
}

// ─── developer_direct_upgrade 测试 ─────────────────────────────────

#[test]
fn developer_direct_upgrade_allows_nrc_admin_when_enabled() {
    new_test_ext().execute_with(|| {
        assert_ok!(RuntimeUpgrade::developer_direct_upgrade(
            RuntimeOrigin::signed(nrc_admin()),
            nrc_cid(),
            committee_role(),
            code_ok(),
            pow_difficulty::PowDifficultyParams::genesis_default(),
        ));
        let code_executed = RUNTIME_CODE_EXECUTED.with(|v| *v.borrow());
        assert!(code_executed, "runtime code executor should be called");
    });
}

#[test]
fn developer_direct_upgrade_fails_when_disabled() {
    new_test_ext().execute_with(|| {
        DEV_UPGRADE_ENABLED.with(|v| *v.borrow_mut() = false);
        assert_noop!(
            RuntimeUpgrade::developer_direct_upgrade(
                RuntimeOrigin::signed(nrc_admin()),
                nrc_cid(),
                committee_role(),
                code_ok(),
                pow_difficulty::PowDifficultyParams::genesis_default(),
            ),
            pallet::Error::<Test>::DeveloperUpgradeDisabled
        );
    });
}

#[test]
fn developer_direct_upgrade_rejects_prc_admin() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            RuntimeUpgrade::developer_direct_upgrade(
                RuntimeOrigin::signed(prc_admin()),
                nrc_cid(),
                committee_role(),
                code_ok(),
                pow_difficulty::PowDifficultyParams::genesis_default(),
            ),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn developer_direct_upgrade_rejects_non_nrc_admin() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            RuntimeUpgrade::developer_direct_upgrade(
                RuntimeOrigin::signed(outsider()),
                nrc_cid(),
                committee_role(),
                code_ok(),
                pow_difficulty::PowDifficultyParams::genesis_default(),
            ),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn developer_direct_upgrade_rejects_empty_code() {
    new_test_ext().execute_with(|| {
        let empty_code: pallet::CodeOf<Test> = vec![].try_into().expect("empty code");
        assert_noop!(
            RuntimeUpgrade::developer_direct_upgrade(
                RuntimeOrigin::signed(nrc_admin()),
                nrc_cid(),
                committee_role(),
                empty_code,
                pow_difficulty::PowDifficultyParams::genesis_default(),
            ),
            pallet::Error::<Test>::EmptyRuntimeCode
        );
    });
}
