//! 提案状态迁移、终态副作用、回调作用域和统一事件。

use crate::pallet::*;
use crate::*;
use frame_support::{
    ensure,
    pallet_prelude::*,
    storage::{with_transaction, TransactionOutcome},
    traits::Get,
    weights::Weight,
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::{
    traits::{One, Saturating},
    DispatchError,
};
use sp_std::vec::Vec;

/// 状态推进时一次性带出的提案类型、阶段、主体和回调标记。
type StatusTransitionOutcome<AccountId> =
    (u8, u8, sp_std::vec::Vec<ProposalSubject<AccountId>>, bool);

impl<T: Config> Pallet<T> {
    /// 根据 citizen-identity 已维护的四级人口数据生成提案人口快照。
    pub fn create_population_snapshot(
        proposal_id: u64,
        scope: &PopulationScope,
    ) -> Result<u64, DispatchError> {
        ensure!(
            Proposals::<T>::contains_key(proposal_id),
            Error::<T>::ProposalNotFound
        );
        ensure!(
            !ProposalPopulationSnapshots::<T>::contains_key(proposal_id),
            Error::<T>::InvalidProposalStatus
        );
        let population_data = T::CitizenIdentityReader::population_data(scope)
            .ok_or(Error::<T>::PopulationDataNotReady)?;
        let eligible_total = population_data.eligible_total;
        ProposalPopulationSnapshots::<T>::insert(
            proposal_id,
            crate::types::ProposalPopulationSnapshot {
                population_data,
                created_at: frame_system::Pallet::<T>::block_number(),
            },
        );
        Ok(eligible_total)
    }

    /// 按投票引擎保存的提案人口快照返回建案时完整公民主体。
    pub fn voting_subject_at_population_snapshot(
        proposal_id: u64,
        who: &T::AccountId,
    ) -> Option<citizen_identity::CitizenSubject<T::AccountId>> {
        ProposalPopulationSnapshots::<T>::get(proposal_id).and_then(|snapshot| {
            T::CitizenIdentityReader::voting_subject_at(who, &snapshot.population_data)
        })
    }

    /// 读取提案人口快照的公投选民总数；没有人口快照时返回 `None`。
    /// 供立法业务壳在写入核心修宪版本时取永久公投凭据(见 legislation-vote `referendum_result`)。
    /// 读已终结提案亦可(不校验 open 状态),故与 `ensure_open_proposal` 分开。
    pub fn population_eligible_total_of(proposal_id: u64) -> Option<u64> {
        ProposalPopulationSnapshots::<T>::get(proposal_id)
            .map(|snapshot| snapshot.population_data.eligible_total)
    }

    pub fn ensure_open_proposal(
        proposal_id: u64,
    ) -> Result<Proposal<BlockNumberFor<T>, T::AccountId>, DispatchError> {
        let proposal = Proposals::<T>::get(proposal_id).ok_or(Error::<T>::ProposalNotFound)?;

        ensure!(
            proposal.status == STATUS_VOTING,
            Error::<T>::InvalidProposalStatus
        );
        ensure!(
            <frame_system::Pallet<T>>::block_number() <= proposal.end,
            Error::<T>::VoteClosed
        );

        Ok(proposal)
    }

    pub(crate) fn should_release_internal_proposal_mutexes(
        kind: u8,
        stage: u8,
        final_status: u8,
    ) -> bool {
        matches!(
            final_status,
            STATUS_REJECTED | STATUS_EXECUTED | STATUS_EXECUTION_FAILED
        ) || (kind == PROPOSAL_KIND_JOINT && stage == STAGE_JOINT && final_status == STATUS_PASSED)
    }

    pub(crate) fn ensure_valid_status_transition(old_status: u8, new_status: u8) -> DispatchResult {
        ensure!(
            matches!(
                (old_status, new_status),
                (STATUS_VOTING, STATUS_PASSED)
                    | (STATUS_VOTING, STATUS_REJECTED)
                    | (STATUS_PASSED, STATUS_EXECUTED)
                    | (STATUS_PASSED, STATUS_EXECUTION_FAILED)
            ),
            Error::<T>::InvalidProposalStatus
        );
        Ok(())
    }

    pub(crate) fn is_terminal_status(status: u8) -> bool {
        matches!(
            status,
            STATUS_REJECTED | STATUS_EXECUTED | STATUS_EXECUTION_FAILED
        )
    }

    pub fn mark_proposal_passed_at(proposal_id: u64, block: BlockNumberFor<T>) {
        ProposalMeta::<T>::mutate(proposal_id, |meta| {
            if let Some(m) = meta {
                if m.passed_at.is_none() {
                    m.passed_at = Some(block);
                }
            }
        });
    }

    pub(crate) fn set_proposal_status(proposal_id: u64, status: u8) -> DispatchResult {
        Proposals::<T>::try_mutate(proposal_id, |maybe| {
            let proposal = maybe.as_mut().ok_or(Error::<T>::ProposalNotFound)?;
            Self::ensure_valid_status_transition(proposal.status, status)?;
            proposal.status = status;
            Ok(())
        })
    }

    pub(crate) fn apply_terminal_side_effects(proposal_id: u64, status: u8) -> DispatchResult {
        ensure!(
            Self::is_terminal_status(status),
            Error::<T>::InvalidProposalStatus
        );
        let now = frame_system::Pallet::<T>::block_number();
        cleanup::schedule_cleanup::<T>(proposal_id, now)?;
        ProposalExecutionRetryStates::<T>::remove(proposal_id);
        PendingExecutionRetryExpirations::<T>::remove(proposal_id);
        PendingTerminalCleanups::<T>::remove(proposal_id);
        if status == STATUS_EXECUTION_FAILED {
            if let Some(proposal) = Proposals::<T>::get(proposal_id) {
                // 清理登记成功后再通知业务模块释放 pending 锁，
                // 避免先产生业务侧副作用、再发现链上清理无法登记。通知失败
                // 不再吞掉，而是进入有界重试队列。
                Self::notify_execution_failed_terminal_or_queue(proposal_id, proposal.kind);
            }
        }
        if let Some(proposal) = Proposals::<T>::get(proposal_id) {
            <T::TrackHandlers as crate::tracks::ProposalTracks<
                BlockNumberFor<T>,
                T::AccountId,
            >>::on_proposal_terminal(proposal.kind, proposal_id, status)
            .ok_or(Error::<T>::InvalidProposalStage)??;
            if Self::should_release_internal_proposal_mutexes(proposal.kind, proposal.stage, status)
            {
                Self::release_internal_proposal_mutexes(proposal_id);
            }
        }
        Ok(())
    }

    pub(crate) fn queue_execution_retry_deadline(
        proposal_id: u64,
        target: BlockNumberFor<T>,
    ) -> DispatchResult {
        Ok(ExecutionRetryDeadlines::<T>::try_mutate(target, |ids| {
            ids.try_push(proposal_id)
                .map_err(|_| Error::<T>::TooManyExecutionRetryDeadlines)
        })?)
    }

    pub(crate) fn reschedule_execution_retry_deadline(
        proposal_id: u64,
        from: BlockNumberFor<T>,
    ) -> DispatchResult {
        let mut target = from;
        for _ in 0..100u32 {
            if Self::queue_execution_retry_deadline(proposal_id, target).is_ok() {
                return Ok(());
            }
            target = target.saturating_add(BlockNumberFor::<T>::one());
        }
        Err(Error::<T>::TooManyExecutionRetryDeadlines.into())
    }

    pub(crate) fn queue_pending_retry_expiration(
        proposal_id: u64,
        retry_deadline: BlockNumberFor<T>,
    ) {
        PendingExecutionRetryExpirations::<T>::insert(proposal_id, retry_deadline);
        Self::deposit_event(Event::<T>::ProposalExecutionRetryExpirationQueued {
            proposal_id,
            retry_deadline,
        });
    }

    pub(crate) fn finish_terminal_status(proposal_id: u64, status: u8) -> DispatchResult {
        Self::apply_terminal_side_effects(proposal_id, status)?;
        Self::deposit_event(Event::<T>::ProposalFinalized {
            proposal_id,
            status,
        });
        Ok(())
    }

    pub(crate) fn ensure_retry_admin(who: &T::AccountId, proposal_id: u64) -> DispatchResult {
        let proposal = Proposals::<T>::get(proposal_id).ok_or(Error::<T>::ProposalNotFound)?;
        let authorized = if proposal.kind == PROPOSAL_KIND_JOINT
            || (proposal.kind == PROPOSAL_KIND_INTERNAL && proposal.actor_cid_number.is_some())
        {
            Self::is_any_institution_voter_in_snapshot(proposal_id, who)
        } else {
            let subject = proposal
                .subject_keys()
                .into_iter()
                .next()
                .ok_or(Error::<T>::InvalidInstitution)?;
            Self::is_admin_in_snapshot(proposal_id, subject, who)
        };
        ensure!(authorized, Error::<T>::NoPermission);
        Ok(())
    }

    pub(crate) fn invoke_execution_callback(
        proposal_id: u64,
        kind: u8,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError> {
        match kind {
            PROPOSAL_KIND_INTERNAL => {
                T::InternalVoteResultCallback::on_internal_vote_finalized(proposal_id, approved)
            }
            PROPOSAL_KIND_JOINT => {
                T::JointVoteResultCallback::on_joint_vote_finalized(proposal_id, approved)
            }
            PROPOSAL_KIND_LEGISLATION => {
                T::LegislationVoteResultCallback::on_legislation_vote_finalized(
                    proposal_id,
                    approved,
                )
            }
            PROPOSAL_KIND_ELECTION => {
                T::ElectionVoteResultCallback::on_election_vote_finalized(proposal_id, approved)
            }
            _ => Err(Error::<T>::InvalidProposalKind.into()),
        }
    }

    pub(crate) fn can_cancel_passed_proposal_by_owner(
        proposal_id: u64,
        kind: u8,
    ) -> DispatchResult {
        let decision = match kind {
            PROPOSAL_KIND_INTERNAL => {
                T::InternalVoteResultCallback::can_cancel_passed_proposal(proposal_id)
            }
            PROPOSAL_KIND_JOINT => {
                T::JointVoteResultCallback::can_cancel_passed_proposal(proposal_id)
            }
            PROPOSAL_KIND_LEGISLATION => {
                T::LegislationVoteResultCallback::can_cancel_passed_proposal(proposal_id)
            }
            PROPOSAL_KIND_ELECTION => {
                T::ElectionVoteResultCallback::can_cancel_passed_proposal(proposal_id)
            }
            _ => Err(Error::<T>::InvalidProposalKind.into()),
        }?;
        ensure!(
            decision == ProposalCancelDecision::Allow,
            Error::<T>::ProposalCancellationNotAllowed
        );
        Ok(())
    }

    pub(crate) fn notify_execution_failed_terminal(proposal_id: u64, kind: u8) -> DispatchResult {
        match kind {
            PROPOSAL_KIND_INTERNAL => {
                T::InternalVoteResultCallback::on_execution_failed_terminal(proposal_id)
            }
            PROPOSAL_KIND_JOINT => {
                T::JointVoteResultCallback::on_execution_failed_terminal(proposal_id)
            }
            PROPOSAL_KIND_LEGISLATION => {
                T::LegislationVoteResultCallback::on_execution_failed_terminal(proposal_id)
            }
            PROPOSAL_KIND_ELECTION => {
                T::ElectionVoteResultCallback::on_execution_failed_terminal(proposal_id)
            }
            _ => Err(Error::<T>::InvalidProposalKind.into()),
        }
    }

    pub(crate) fn queue_terminal_cleanup(proposal_id: u64) {
        let already_pending = PendingTerminalCleanups::<T>::contains_key(proposal_id);
        PendingTerminalCleanups::<T>::insert(proposal_id, ());
        if !already_pending {
            Self::deposit_event(Event::<T>::ProposalTerminalCleanupQueued { proposal_id });
        }
    }

    pub(crate) fn notify_execution_failed_terminal_or_queue(proposal_id: u64, kind: u8) {
        let result = Self::with_callback_execution_scope(proposal_id, || {
            Self::notify_execution_failed_terminal(proposal_id, kind)
        });
        if result.is_ok() {
            PendingTerminalCleanups::<T>::remove(proposal_id);
            return;
        }
        Self::queue_terminal_cleanup(proposal_id);
    }

    pub(crate) fn process_pending_terminal_cleanups() -> Weight {
        let db_weight = T::DbWeight::get();
        let mut weight = db_weight.reads(1);
        let max = T::MaxPendingRetryExpirationsPerBlock::get() as usize;
        if max == 0 {
            return weight;
        }

        let pending: Vec<u64> = PendingTerminalCleanups::<T>::iter()
            .take(max)
            .map(|(proposal_id, _)| proposal_id)
            .collect();
        for proposal_id in pending {
            weight = weight.saturating_add(db_weight.reads_writes(2, 3));
            let Some(proposal) = Proposals::<T>::get(proposal_id) else {
                PendingTerminalCleanups::<T>::remove(proposal_id);
                continue;
            };
            if proposal.status != STATUS_EXECUTION_FAILED {
                PendingTerminalCleanups::<T>::remove(proposal_id);
                continue;
            }
            let result = Self::with_callback_execution_scope(proposal_id, || {
                Self::notify_execution_failed_terminal(proposal_id, proposal.kind)
            });
            if result.is_ok() {
                PendingTerminalCleanups::<T>::remove(proposal_id);
                Self::deposit_event(Event::<T>::ProposalTerminalCleanupCompleted { proposal_id });
            }
        }
        weight
    }

    pub(crate) fn schedule_execution_retry(proposal_id: u64) -> DispatchResult {
        if ProposalExecutionRetryStates::<T>::contains_key(proposal_id) {
            return Ok(());
        }
        let now = frame_system::Pallet::<T>::block_number();
        let retry_deadline = now.saturating_add(T::ExecutionRetryGraceBlocks::get());
        let state = ExecutionRetryState {
            manual_attempts: 0,
            first_auto_failed_at: now,
            retry_deadline,
            last_attempt_at: None,
        };
        if Self::reschedule_execution_retry_deadline(proposal_id, retry_deadline).is_err() {
            Self::queue_pending_retry_expiration(proposal_id, retry_deadline);
        }
        ProposalExecutionRetryStates::<T>::insert(proposal_id, state);
        Self::deposit_event(Event::<T>::ProposalExecutionRetryScheduled {
            proposal_id,
            retry_deadline,
        });
        Ok(())
    }
}

impl<T: Config> Pallet<T> {
    /// 查询当前是否处于某个提案的业务回调/终态清理作用域。
    ///
    /// 业务 pallet 用它保护敏感生命周期写入，避免普通 runtime 调用绕过投票引擎。
    pub fn is_callback_execution_scope(proposal_id: u64) -> bool {
        CallbackExecutionScopes::<T>::contains_key(proposal_id)
    }

    pub(crate) fn with_callback_execution_scope<F, R>(
        proposal_id: u64,
        callback: F,
    ) -> Result<R, DispatchError>
    where
        F: FnOnce() -> Result<R, DispatchError>,
    {
        CallbackExecutionScopes::<T>::insert(proposal_id, ());
        let result = callback();
        CallbackExecutionScopes::<T>::remove(proposal_id);
        result
    }

    /// 更新提案状态，并按统一 executor 结果推进业务执行状态。
    pub fn set_status_and_emit(proposal_id: u64, status: u8) -> DispatchResult {
        with_transaction(|| {
            let (kind, stage, subjects, should_run_callback) = match Proposals::<T>::try_mutate(
                proposal_id,
                |maybe| -> Result<StatusTransitionOutcome<T::AccountId>, DispatchError> {
                    let proposal = maybe.as_mut().ok_or(Error::<T>::ProposalNotFound)?;
                    let old_status = proposal.status;
                    Self::ensure_valid_status_transition(old_status, status)?;
                    let kind = proposal.kind;
                    let stage = proposal.stage;
                    let subjects = proposal.subject_keys();
                    proposal.status = status;
                    if old_status == STATUS_VOTING && status == STATUS_PASSED {
                        let now = frame_system::Pallet::<T>::block_number();
                        Self::mark_proposal_passed_at(proposal_id, now);
                    }
                    Ok((
                        kind,
                        stage,
                        subjects,
                        old_status == STATUS_VOTING
                            && matches!(status, STATUS_PASSED | STATUS_REJECTED),
                    ))
                },
            ) {
                Ok(v) => v,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };

            // 提案结束（通过或拒绝），立即释放活跃提案名额
            if status != STATUS_VOTING {
                for subject in subjects {
                    limit::remove_active_proposal::<T>(subject, proposal_id);
                }
                AutoFinalizeRetryStates::<T>::remove(proposal_id);
                AutoFinalizeDeadLetters::<T>::remove(proposal_id);
            }

            if should_run_callback && status == STATUS_REJECTED {
                let outcome = match Self::with_callback_execution_scope(proposal_id, || {
                    Self::invoke_execution_callback(proposal_id, kind, status == STATUS_PASSED)
                }) {
                    Ok(outcome) => outcome,
                    Err(err) => return TransactionOutcome::Rollback(Err(err)),
                };
                let _ = outcome;
            } else if should_run_callback && status == STATUS_PASSED {
                let now = frame_system::Pallet::<T>::block_number();
                PendingProposalExecutions::<T>::insert(
                    proposal_id,
                    crate::types::PendingExecutionState {
                        attempts: 0,
                        next_attempt_at: now,
                    },
                );
                Self::deposit_event(Event::<T>::ProposalExecutionQueued { proposal_id });
            }

            let final_status = match Proposals::<T>::get(proposal_id) {
                Some(proposal) => proposal.status,
                None => {
                    return TransactionOutcome::Rollback(Err(Error::<T>::ProposalNotFound.into()))
                }
            };
            // PASSED 是执行授权/可重试态，不再视为终态。
            // 90 天延迟清理只登记 REJECTED / EXECUTED / EXECUTION_FAILED。
            if Self::is_terminal_status(final_status) {
                if let Err(err) = Self::apply_terminal_side_effects(proposal_id, final_status) {
                    return TransactionOutcome::Rollback(Err(err));
                }
            } else if Self::should_release_internal_proposal_mutexes(kind, stage, final_status) {
                Self::release_internal_proposal_mutexes(proposal_id);
            }
            Self::deposit_event(Event::<T>::ProposalFinalized {
                proposal_id,
                status: final_status,
            });

            TransactionOutcome::Commit(Ok(()))
        })
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
