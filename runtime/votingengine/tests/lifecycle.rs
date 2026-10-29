//! 核心数据登记、状态迁移、活跃上限与互斥约束。
use super::*;

#[test]
fn proposal_data_is_bound_once_and_rejects_missing_or_oversized_input() {
    ext().execute_with(|| {
        assert_noop!(
            VotingEngine::register_proposal_data(9, b"owner", vec![1], 1),
            Error::<Test>::ProposalNotFound
        );
        seed(1, 10);
        assert_noop!(
            VotingEngine::register_proposal_data(1, b"owner", vec![1; 33], 1),
            DispatchError::Other("ProposalDataTooLarge")
        );
        assert_noop!(
            VotingEngine::register_proposal_data(1, &[b'x'; 33], vec![1], 1),
            DispatchError::Other("ModuleTagTooLarge")
        );
        assert_ok!(VotingEngine::register_proposal_data(
            1,
            b"owner",
            vec![1; 32],
            1
        ));
        assert_eq!(ProposalData::<Test>::get(1).unwrap().len(), 32);
        assert_eq!(ProposalOwner::<Test>::get(1).unwrap().as_slice(), b"owner");
        assert_noop!(
            VotingEngine::register_proposal_data(1, b"other", vec![2], 1),
            Error::<Test>::ProposalDataAlreadyRegistered
        );
    });
}

#[test]
fn voting_end_is_inclusive_and_unknown_proposals_are_rejected() {
    ext().execute_with(|| {
        assert_eq!(
            VotingEngine::ensure_open_proposal(1),
            Err(Error::<Test>::ProposalNotFound.into())
        );
        seed(1, 10);
        System::set_block_number(10);
        assert!(VotingEngine::ensure_open_proposal(1).is_ok());
        System::set_block_number(11);
        assert_eq!(
            VotingEngine::ensure_open_proposal(1),
            Err(Error::<Test>::VoteClosed.into())
        );
    });
}

#[test]
fn terminal_state_cannot_be_reopened_or_executed_again() {
    ext().execute_with(|| {
        seed(1, 10);
        assert_noop!(
            VotingEngine::set_status_and_emit(1, STATUS_EXECUTED),
            Error::<Test>::InvalidProposalStatus
        );
        assert_ok!(VotingEngine::set_status_and_emit(1, STATUS_PASSED));
        assert!(PendingProposalExecutions::<Test>::contains_key(1));
        assert_noop!(
            VotingEngine::set_status_and_emit(1, STATUS_VOTING),
            Error::<Test>::InvalidProposalStatus
        );
        VotingEngine::process_pending_proposal_executions(1);
        assert_eq!(Proposals::<Test>::get(1).unwrap().status, STATUS_EXECUTED);
        assert_noop!(
            VotingEngine::set_status_and_emit(1, STATUS_PASSED),
            Error::<Test>::InvalidProposalStatus
        );
        VotingEngine::process_pending_proposal_executions(2);
        assert_eq!(CALLBACK_COUNT.with(Cell::get), 1);
    });
}

#[test]
fn active_limit_releases_only_the_removed_proposal() {
    ext().execute_with(|| {
        let subject = ProposalSubject::PersonalAccount(1);
        assert_ok!(limit::try_add_active_proposal::<Test>(subject.clone(), 1));
        assert_ok!(limit::try_add_active_proposal::<Test>(subject.clone(), 2));
        assert_noop!(
            limit::try_add_active_proposal::<Test>(subject.clone(), 3),
            Error::<Test>::ActiveProposalLimitReached
        );
        limit::remove_active_proposal::<Test>(subject.clone(), 1);
        assert_eq!(limit::active_proposal_count::<Test>(subject.clone()), 1);
        assert_ok!(limit::try_add_active_proposal::<Test>(subject, 3));
    });
}

#[test]
fn exclusive_admin_mutex_conflicts_and_releases_exact_owner() {
    ext().execute_with(|| {
        let subject = ProposalSubject::PersonalAccount(1);
        assert_ok!(VotingEngine::acquire_internal_proposal_mutex(
            1,
            subject.clone(),
            InternalProposalMutexKind::Regular
        ));
        assert_noop!(
            VotingEngine::acquire_internal_proposal_mutex(
                2,
                subject.clone(),
                InternalProposalMutexKind::AdminSetMutationExclusive
            ),
            Error::<Test>::RegularInternalProposalActive
        );
        VotingEngine::release_internal_proposal_mutexes(1);
        assert_ok!(VotingEngine::acquire_internal_proposal_mutex(
            2,
            subject.clone(),
            InternalProposalMutexKind::AdminSetMutationExclusive
        ));
        assert_noop!(
            VotingEngine::ensure_admin_set_mutation_lock_owner(subject.clone(), 3),
            Error::<Test>::InternalProposalMutexOwnerMismatch
        );
        assert_noop!(
            VotingEngine::acquire_internal_proposal_mutex(
                3,
                subject.clone(),
                InternalProposalMutexKind::Regular
            ),
            Error::<Test>::AdminSetMutationProposalActive
        );
        assert_ok!(VotingEngine::ensure_admin_set_mutation_lock_owner(
            subject.clone(),
            2
        ));
        VotingEngine::release_internal_proposal_mutexes(2);
        assert!(InternalProposalMutexes::<Test>::get(subject).is_none());
    });
}

// 原cfg(test)专属辅助随测试移出src，保持唯一实现和原行为。
use frame_support::{ensure, BoundedVec};
use frame_system::pallet_prelude::BlockNumberFor;
impl<T: Config> crate::Pallet<T> {
    /// 存储提案业务数据（供 votingengine crate 内部测试与统一登记流程使用）。
    #[cfg(test)]
    pub fn store_proposal_data(proposal_id: u64, data: sp_std::vec::Vec<u8>) -> DispatchResult {
        let bounded: BoundedVec<u8, T::MaxProposalDataLen> = data
            .try_into()
            .map_err(|_| DispatchError::Other("ProposalDataTooLarge"))?;
        ProposalData::<T>::insert(proposal_id, bounded);
        Ok(())
    }

    /// 删除提案对象层数据与元数据。
    #[cfg(test)]
    pub fn remove_proposal_object(proposal_id: u64) {
        ProposalObject::<T>::remove(proposal_id);
        ProposalObjectMeta::<T>::remove(proposal_id);
    }

    /// 存储提案辅助元数据(创建时间)。
    #[cfg(test)]
    pub fn store_proposal_meta(proposal_id: u64, created_at: BlockNumberFor<T>) {
        ProposalMeta::<T>::insert(
            proposal_id,
            ProposalMetadata {
                created_at,
                passed_at: None,
            },
        );
    }

    /// 标记提案通过时间。
    #[cfg(test)]
    pub fn set_proposal_passed(proposal_id: u64, block: BlockNumberFor<T>) {
        Self::mark_proposal_passed_at(proposal_id, block);
    }

    /// 回调专用执行结果写入。
    ///
    /// 仅供单测验证回调作用域保护；生产业务回调应直接返回
    /// `ProposalExecutionOutcome`，由异步执行队列统一收口状态、事件和清理。
    #[cfg(test)]
    pub fn set_callback_execution_result(proposal_id: u64, final_status: u8) -> DispatchResult {
        ensure!(
            CallbackExecutionScopes::<T>::contains_key(proposal_id),
            Error::<T>::InvalidProposalStatus
        );
        ensure!(
            matches!(final_status, STATUS_EXECUTED | STATUS_EXECUTION_FAILED),
            Error::<T>::InvalidProposalStatus
        );
        Proposals::<T>::try_mutate(proposal_id, |maybe| {
            let proposal = maybe.as_mut().ok_or(Error::<T>::ProposalNotFound)?;
            Self::ensure_valid_status_transition(proposal.status, final_status)?;
            proposal.status = final_status;
            Ok(())
        })
    }
}
