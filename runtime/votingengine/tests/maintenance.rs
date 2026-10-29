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
        VotingEngine::process_pending_proposal_executions(1);
        assert_eq!(CALLBACK_COUNT.with(Cell::get), 0);
        ExecutionBudget::set(Weight::MAX);
        FAIL_CALLBACK.with(|v| v.set(true));
        VotingEngine::process_pending_proposal_executions(1);
        let state = PendingProposalExecutions::<Test>::get(1).unwrap();
        assert_eq!(state.attempts, 1);
        assert!(state.next_attempt_at > 1);
        assert_eq!(Proposals::<Test>::get(1).unwrap().status, STATUS_PASSED);
        assert!(sp_io::storage::get(b"test:execution-effect").is_none());
        FAIL_CALLBACK.with(|v| v.set(false));
        VotingEngine::process_pending_proposal_executions(state.next_attempt_at - 1);
        assert_eq!(CALLBACK_COUNT.with(Cell::get), 1);
        System::set_block_number(state.next_attempt_at);
        VotingEngine::process_pending_proposal_executions(state.next_attempt_at);
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
