//! 到期队列、执行回滚重试和分块清理预算的行为回归。
use super::*;
use frame_support::traits::Hooks;

#[test]
fn expiry_bucket_uses_end_plus_one_and_enforces_capacity() {
    ext().execute_with(|| {
        assert_ok!(VotingEngine::schedule_proposal_expiry(1, 10));
        assert_ok!(VotingEngine::schedule_proposal_expiry(2, 10));
        assert_noop!(
            VotingEngine::schedule_proposal_expiry(3, 10),
            Error::<Test>::TooManyProposalsAtExpiry
        );
        assert!(ProposalsByExpiry::<Test>::get(10).is_empty());
        assert_eq!(ProposalsByExpiry::<Test>::get(11).as_slice(), &[1, 2]);
        seed(1, 10);
        seed(2, 10);
        System::set_block_number(10);
        <VotingEngine as Hooks<u64>>::on_initialize(10);
        assert_eq!(Proposals::<Test>::get(1).unwrap().status, STATUS_VOTING);
        System::set_block_number(11);
        <VotingEngine as Hooks<u64>>::on_initialize(11);
        assert_eq!(Proposals::<Test>::get(1).unwrap().status, STATUS_REJECTED);
        assert_eq!(Proposals::<Test>::get(2).unwrap().status, STATUS_REJECTED);
    });
}

#[test]
fn zero_auto_budget_keeps_due_work_pending_until_budget_returns() {
    ext().execute_with(|| {
        seed(1, 1);
        assert_ok!(VotingEngine::schedule_proposal_expiry(1, 1));
        AutoBudget::set(Weight::zero());
        System::set_block_number(2);
        <VotingEngine as Hooks<u64>>::on_initialize(2);
        assert_eq!(Proposals::<Test>::get(1).unwrap().status, STATUS_VOTING);
        AutoBudget::set(Weight::MAX);
        System::set_block_number(3);
        <VotingEngine as Hooks<u64>>::on_initialize(3);
        assert_eq!(Proposals::<Test>::get(1).unwrap().status, STATUS_REJECTED);
    });
}

#[test]
fn failed_execution_rolls_back_effect_and_retries_only_when_due() {
    ext().execute_with(|| {
        seed(1, 10);
        assert_ok!(VotingEngine::set_status_and_emit(1, STATUS_PASSED));
        ExecutionBudget::set(Weight::zero());
        VotingEngine::process_pending_proposal_executions(1, ExecutionBudget::get());
        assert_eq!(CALLBACK_COUNT.with(Cell::get), 0);
        ExecutionBudget::set(Weight::MAX);
        FAIL_CALLBACK.with(|v| v.set(true));
        VotingEngine::process_pending_proposal_executions(1, ExecutionBudget::get());
        let state = PendingProposalExecutions::<Test>::get(1).unwrap();
        assert_eq!(state.attempts, 1);
        assert!(state.next_attempt_at > 1);
        assert_eq!(Proposals::<Test>::get(1).unwrap().status, STATUS_PASSED);
        assert!(sp_io::storage::get(b"test:execution-effect").is_none());
        FAIL_CALLBACK.with(|v| v.set(false));
        VotingEngine::process_pending_proposal_executions(
            state.next_attempt_at - 1,
            ExecutionBudget::get(),
        );
        assert_eq!(CALLBACK_COUNT.with(Cell::get), 1);
        System::set_block_number(state.next_attempt_at);
        VotingEngine::process_pending_proposal_executions(
            state.next_attempt_at,
            ExecutionBudget::get(),
        );
        assert_eq!(CALLBACK_COUNT.with(Cell::get), 2);
        assert_eq!(Proposals::<Test>::get(1).unwrap().status, STATUS_EXECUTED);
        assert!(!PendingProposalExecutions::<Test>::contains_key(1));
        assert_eq!(
            sp_io::storage::get(b"test:execution-effect")
                .unwrap()
                .as_ref(),
            b"written"
        );
    });
}

#[test]
fn delayed_cleanup_waits_for_retention_and_respects_zero_budget() {
    ext().execute_with(|| {
        seed(1, 1);
        assert_ok!(VotingEngine::register_proposal_data(
            1,
            b"owner",
            vec![1],
            1
        ));
        assert_ok!(VotingEngine::set_status_and_emit(1, STATUS_REJECTED));
        let task = ScheduledCleanups::<Test>::get(0).unwrap();
        assert_eq!(
            task.cleanup_at,
            1 + 90 * u64::from(primitives::pow_const::BLOCKS_PER_DAY)
        );
        cleanup::process_scheduled_cleanups::<Test>(task.cleanup_at - 1, Weight::MAX);
        assert_eq!(ScheduledCleanupHead::<Test>::get(), 0);
        cleanup::process_scheduled_cleanups::<Test>(task.cleanup_at, Weight::zero());
        assert_eq!(ScheduledCleanupHead::<Test>::get(), 0);
        cleanup::process_scheduled_cleanups::<Test>(task.cleanup_at, Weight::MAX);
        assert!(PendingProposalCleanups::<Test>::contains_key(1));
        VotingEngine::process_pending_cleanup_steps(Weight::zero());
        assert!(Proposals::<Test>::contains_key(1));
        for _ in 0..8 {
            VotingEngine::process_pending_cleanup_steps(Weight::MAX);
        }
        assert!(!PendingProposalCleanups::<Test>::contains_key(1));
        assert!(!Proposals::<Test>::contains_key(1));
        assert!(!ProposalOwner::<Test>::contains_key(1));
        assert!(!ProposalData::<Test>::contains_key(1));
    });
}

#[test]
fn queue_sequence_exhaustion_rejects_without_overwriting_existing_work() {
    ext().execute_with(|| {
        ScheduledCleanupTail::<Test>::put(u64::MAX);
        assert_noop!(
            cleanup::schedule_cleanup::<Test>(1, 1),
            Error::<Test>::CleanupQueueSequenceExhausted
        );
        PendingCleanupQueueTail::<Test>::put(u64::MAX);
        assert_noop!(
            cleanup::enqueue_pending_cleanup::<Test>(1),
            Error::<Test>::CleanupQueueSequenceExhausted
        );
    });
}

// 小任务必须在小于满256互斥模型的预算中完成，不能被全局最大模型永久挡住。
#[test]
fn small_terminal_cleanup_finishes_with_a_small_proof_budget() {
    ext().execute_with(|| {
        seed(1, 1);
        Proposals::<Test>::mutate(1, |proposal| {
            proposal.as_mut().unwrap().status = STATUS_REJECTED
        });
        for index in 1..=2 {
            assert_ok!(VotingEngine::acquire_internal_proposal_mutex(
                1,
                ProposalSubject::PersonalAccount(index),
                InternalProposalMutexKind::Regular
            ));
        }
        PendingProposalCleanups::<Test>::insert(1, PendingCleanupStage::FinalCleanup);
        assert_ok!(cleanup::enqueue_pending_cleanup::<Test>(1));
        VotingEngine::process_pending_cleanup_steps(Weight::zero());
        assert!(Proposals::<Test>::contains_key(1));
        let budget = Weight::from_parts(100_000_000_000, 300_000);
        let used = VotingEngine::process_pending_cleanup_steps(budget);
        assert!(!used.any_gt(budget));
        assert!(!Proposals::<Test>::contains_key(1));
        assert!(ProposalMutexBindings::<Test>::get(1).is_empty());
        assert!(!PendingProposalCleanups::<Test>::contains_key(1));
    });
}

#[test]
fn expiry_backlog_does_not_skip_intermediate_blocks() {
    ext().execute_with(|| {
        for block in 1..=3 {
            for offset in 0..2 {
                let id = block * 2 + offset;
                seed(id, block);
                assert_ok!(VotingEngine::schedule_proposal_expiry(id, block));
            }
        }
        AutoBudget::set(Weight::zero());
        System::set_block_number(2);
        <VotingEngine as Hooks<u64>>::on_initialize(2);
        AutoBudget::set(Weight::MAX);
        for now in 3..=6 {
            System::set_block_number(now);
            <VotingEngine as Hooks<u64>>::on_initialize(now);
        }
        for id in 2..8 {
            assert_eq!(Proposals::<Test>::get(id).unwrap().status, STATUS_REJECTED);
        }
    });
}
#[test]
fn maintenance_categories_share_one_budget() {
    ext().execute_with(|| {
        let budget = Weight::from_parts(5_000_000_000_000, 2_000_000);
        AutoBudget::set(budget);
        ExecutionBudget::set(budget);
        CleanupBudget::set(budget);
        for id in 1..=2 {
            seed(id, 1);
            assert_ok!(VotingEngine::schedule_proposal_expiry(id, 1));
        }
        for now in 2..=5 {
            System::set_block_number(now);
            assert!(!<VotingEngine as Hooks<u64>>::on_initialize(now).any_gt(budget));
        }
    });
}

#[test]
fn pending_expiry_rotates_past_future_deadlines() {
    ext().execute_with(|| {
        for id in 10..13 {
            seed(id, 1);
            Proposals::<Test>::mutate(id, |p| p.as_mut().unwrap().status = STATUS_PASSED);
            PendingExecutionRetryExpirations::<Test>::insert(id, 1000);
            ProposalExecutionRetryStates::<Test>::insert(
                id,
                ExecutionRetryState {
                    manual_attempts: 0,
                    first_auto_failed_at: 0,
                    retry_deadline: 1000,
                    last_attempt_at: None,
                },
            );
        }
        let ordered = PendingExecutionRetryExpirations::<Test>::iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        let due = *ordered.last().unwrap();
        PendingExecutionRetryExpirations::<Test>::insert(due, 1);
        ProposalExecutionRetryStates::<Test>::mutate(due, |state| {
            state.as_mut().unwrap().retry_deadline = 1
        });
        for _ in 0..3 {
            VotingEngine::process_pending_execution_retry_expirations(2, Weight::MAX);
        }
        assert_eq!(
            Proposals::<Test>::get(due).unwrap().status,
            STATUS_EXECUTION_FAILED
        );
    });
}
