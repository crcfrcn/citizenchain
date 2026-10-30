//! 已通过提案的异步业务执行、重试期限和管理员恢复入口。

use crate::pallet::*;
use crate::weights::WeightInfo;
use crate::*;
use frame_support::{
    ensure,
    pallet_prelude::*,
    storage::{with_transaction, TransactionOutcome},
    traits::Get,
    weights::Weight,
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::traits::{One, SaturatedConversion, Saturating};

impl<T: Config> Pallet<T> {
    /// 在 Runtime 独立 weight 与固定条数双重预算内处理通过提案。
    ///
    /// 每项均按最重 finalize/set_code 保守值预留，避免 runtime 升级等重回调
    /// 在 `on_initialize` 内造成未计费执行或挤占整个区块。
    pub(crate) fn process_pending_proposal_executions(now: BlockNumberFor<T>) -> Weight {
        let max = T::MaxAutoFinalizePerBlock::get() as usize;
        if max == 0 {
            return Weight::zero();
        }
        let db = T::DbWeight::get();
        let execution_budget = T::MaxExecutionWeightPerBlock::get();
        let scan_weight = db.reads(1);
        let item_weight = T::WeightInfo::process_pending_execution()
            .saturating_add(T::WeightInfo::finalize_proposal())
            // 业务回调可以执行 runtime set_code；基准只测队列框架，最重业务动作
            // 必须在消费预算处显式叠加，且不能写进会被重生的 weights.rs。
            .saturating_add(
                <<T as frame_system::Config>::SystemWeightInfo as frame_system::weights::WeightInfo>::set_code(),
            );
        let mut weight = db.reads(1);
        let mut pending = sp_std::vec::Vec::new();
        for (proposal_id, state) in PendingProposalExecutions::<T>::iter() {
            let after_scan = weight.saturating_add(scan_weight);
            if after_scan.any_gt(execution_budget) {
                break;
            }
            weight = after_scan;
            if state.next_attempt_at > now {
                continue;
            }
            let after_item = weight.saturating_add(item_weight);
            if after_item.any_gt(execution_budget) {
                break;
            }
            pending.push((proposal_id, state));
            weight = after_item;
            if pending.len() >= max {
                break;
            }
        }
        for (proposal_id, mut state) in pending {
            let result = with_transaction(|| {
                let result = (|| -> DispatchResult {
                    let proposal =
                        Proposals::<T>::get(proposal_id).ok_or(Error::<T>::ProposalNotFound)?;
                    if proposal.status != STATUS_PASSED {
                        PendingProposalExecutions::<T>::remove(proposal_id);
                        return Ok(());
                    }
                    match Self::with_callback_execution_scope(proposal_id, || {
                        Self::invoke_execution_callback(proposal_id, proposal.kind, true)
                    }) {
                        Ok(outcome) => {
                            PendingProposalExecutions::<T>::remove(proposal_id);
                            Self::apply_automatic_execution_outcome(
                                proposal_id,
                                proposal.kind,
                                outcome,
                            )?;
                            let final_status = Proposals::<T>::get(proposal_id)
                                .ok_or(Error::<T>::ProposalNotFound)?
                                .status;
                            if Self::is_terminal_status(final_status) {
                                Self::finish_terminal_status(proposal_id, final_status)?;
                            }
                            Ok(())
                        }
                        Err(err) => Err(err),
                    }
                })();
                match result {
                    Ok(()) => TransactionOutcome::Commit(Ok(())),
                    Err(err) => TransactionOutcome::Rollback(Err(err)),
                }
            });
            if result.is_err() {
                Self::handle_automatic_execution_failure(proposal_id, &mut state, now);
            }
        }
        weight
    }

    /// 统一处理异步业务执行的所有错误。
    ///
    /// 回调 Err、Ignored、结果应用错误和 Track 后处理错误都必须走同一计数与退避，
    /// 禁止事务回滚后把原始 attempts 原样塞回下一块。
    fn handle_automatic_execution_failure(
        proposal_id: u64,
        state: &mut PendingExecutionState<BlockNumberFor<T>>,
        now: BlockNumberFor<T>,
    ) {
        let Some(proposal) = Proposals::<T>::get(proposal_id) else {
            // 孤儿队列没有可恢复对象，继续重试只会永久消耗执行预算。
            PendingProposalExecutions::<T>::remove(proposal_id);
            return;
        };
        if proposal.status != STATUS_PASSED {
            PendingProposalExecutions::<T>::remove(proposal_id);
            return;
        }

        state.attempts = state.attempts.saturating_add(1);
        if u32::from(state.attempts) >= T::MaxManualExecutionAttempts::get() {
            // 执行业务回调的重试在这里永久停止。终态副作用使用独立队列，
            // 即使 Track 终态钩子自身损坏，也不能重新执行业务动作。
            let terminalized = with_transaction(|| {
                let result = (|| -> DispatchResult {
                    Self::set_proposal_status(proposal_id, STATUS_EXECUTION_FAILED)?;
                    PendingProposalExecutions::<T>::remove(proposal_id);
                    PendingTerminalFinalizations::<T>::insert(
                        proposal_id,
                        PendingExecutionState {
                            attempts: 0,
                            next_attempt_at: now,
                        },
                    );
                    Self::deposit_event(Event::<T>::ProposalExecutionDeadLettered {
                        proposal_id,
                        attempts: state.attempts,
                    });
                    Ok(())
                })();
                match result {
                    Ok(()) => TransactionOutcome::Commit(Ok(())),
                    Err(err) => TransactionOutcome::Rollback(Err(err)),
                }
            });
            if terminalized.is_ok() {
                return;
            }
        }

        let shift = core::cmp::min(u32::from(state.attempts), 6);
        let delay: BlockNumberFor<T> = (1u64 << shift).saturated_into();
        state.next_attempt_at = now.saturating_add(delay);
        PendingProposalExecutions::<T>::insert(proposal_id, *state);
        Self::deposit_event(Event::<T>::ProposalExecutionDeferred {
            proposal_id,
            attempts: state.attempts,
            next_attempt_at: state.next_attempt_at,
        });
    }

    /// 为已经进入 EXECUTION_FAILED 的提案补齐终态副作用。
    ///
    /// 该队列绝不调用业务执行回调；连续失败后独立 dead-letter，避免重新污染
    /// PendingProposalExecutions 或形成每块热循环。
    pub(crate) fn process_pending_terminal_finalizations(now: BlockNumberFor<T>) -> Weight {
        let db = T::DbWeight::get();
        let max = T::MaxPendingRetryExpirationsPerBlock::get() as usize;
        let mut weight = db.reads(1);
        if max == 0 {
            return weight;
        }
        let pending: sp_std::vec::Vec<_> = PendingTerminalFinalizations::<T>::iter()
            .filter(|(_, state)| state.next_attempt_at <= now)
            .take(max)
            .collect();
        for (proposal_id, mut state) in pending {
            weight = weight.saturating_add(db.reads_writes(2, 3));
            let Some(proposal) = Proposals::<T>::get(proposal_id) else {
                PendingTerminalFinalizations::<T>::remove(proposal_id);
                continue;
            };
            if proposal.status != STATUS_EXECUTION_FAILED {
                PendingTerminalFinalizations::<T>::remove(proposal_id);
                continue;
            }
            let result = with_transaction(|| {
                let result = Self::finish_terminal_status(proposal_id, STATUS_EXECUTION_FAILED);
                match result {
                    Ok(()) => TransactionOutcome::Commit(Ok(())),
                    Err(err) => TransactionOutcome::Rollback(Err(err)),
                }
            });
            if result.is_ok() {
                PendingTerminalFinalizations::<T>::remove(proposal_id);
                TerminalFinalizationDeadLetters::<T>::remove(proposal_id);
                continue;
            }

            state.attempts = state.attempts.saturating_add(1);
            if u32::from(state.attempts) >= T::MaxManualExecutionAttempts::get() {
                PendingTerminalFinalizations::<T>::remove(proposal_id);
                TerminalFinalizationDeadLetters::<T>::insert(proposal_id, state.attempts);
                Self::deposit_event(Event::<T>::ProposalTerminalFinalizationDeadLettered {
                    proposal_id,
                    attempts: state.attempts,
                });
                continue;
            }
            let shift = core::cmp::min(u32::from(state.attempts), 6);
            let delay: BlockNumberFor<T> = (1u64 << shift).saturated_into();
            state.next_attempt_at = now.saturating_add(delay);
            PendingTerminalFinalizations::<T>::insert(proposal_id, state);
            Self::deposit_event(Event::<T>::ProposalTerminalFinalizationDeferred {
                proposal_id,
                attempts: state.attempts,
                next_attempt_at: state.next_attempt_at,
            });
        }
        weight
    }

    pub(crate) fn apply_automatic_execution_outcome(
        proposal_id: u64,
        kind: u8,
        outcome: ProposalExecutionOutcome,
    ) -> DispatchResult {
        match outcome {
            ProposalExecutionOutcome::Ignored => Err(Error::<T>::ProposalOwnerMissing.into()),
            ProposalExecutionOutcome::Executed => {
                Self::set_proposal_status(proposal_id, STATUS_EXECUTED)?;
                <T::TrackHandlers as crate::tracks::ProposalTracks<
                    BlockNumberFor<T>,
                    T::AccountId,
                >>::on_proposal_executed(kind, proposal_id)
                .ok_or(Error::<T>::InvalidProposalStage)??;
                Ok(())
            }
            ProposalExecutionOutcome::RetryableFailed => {
                if kind == PROPOSAL_KIND_INTERNAL {
                    Self::schedule_execution_retry(proposal_id)
                } else {
                    // 当前统一 retry/cancel 资格只支持内部提案；机构按岗位有效选民
                    // 快照、个人多签按管理员快照；
                    // joint callback 若误返回 RetryableFailed，立即失败终态，避免 PASSED 卡死。
                    Self::set_proposal_status(proposal_id, STATUS_EXECUTION_FAILED)
                }
            }
            ProposalExecutionOutcome::FatalFailed => {
                Self::set_proposal_status(proposal_id, STATUS_EXECUTION_FAILED)
            }
        }
    }

    pub(crate) fn process_execution_retry_deadlines(now: BlockNumberFor<T>) -> Weight {
        let db_weight = T::DbWeight::get();
        let mut weight = db_weight.reads_writes(1, 1);
        let queue = ExecutionRetryDeadlines::<T>::take(now);
        if queue.is_empty() {
            return weight;
        }

        for proposal_id in queue.into_iter() {
            weight = weight.saturating_add(db_weight.reads_writes(2, 3));
            let Some(state) = ProposalExecutionRetryStates::<T>::get(proposal_id) else {
                continue;
            };
            if state.retry_deadline > now {
                if Self::reschedule_execution_retry_deadline(proposal_id, state.retry_deadline)
                    .is_err()
                {
                    Self::queue_pending_retry_expiration(proposal_id, state.retry_deadline);
                }
                continue;
            }
            let Some(proposal) = Proposals::<T>::get(proposal_id) else {
                ProposalExecutionRetryStates::<T>::remove(proposal_id);
                continue;
            };
            if proposal.status != STATUS_PASSED {
                ProposalExecutionRetryStates::<T>::remove(proposal_id);
                continue;
            }
            let result = with_transaction(|| {
                let result = (|| -> DispatchResult {
                    Self::set_proposal_status(proposal_id, STATUS_EXECUTION_FAILED)?;
                    Self::deposit_event(Event::<T>::ProposalExecutionRetryExpired { proposal_id });
                    Self::finish_terminal_status(proposal_id, STATUS_EXECUTION_FAILED)
                })();
                match result {
                    Ok(()) => TransactionOutcome::Commit(Ok(())),
                    Err(err) => TransactionOutcome::Rollback(Err(err)),
                }
            });
            if result.is_err() {
                let next_block = now.saturating_add(BlockNumberFor::<T>::one());
                if Self::reschedule_execution_retry_deadline(proposal_id, next_block).is_err() {
                    Self::queue_pending_retry_expiration(proposal_id, state.retry_deadline);
                }
            }
        }
        weight
    }

    pub(crate) fn process_pending_execution_retry_expirations(now: BlockNumberFor<T>) -> Weight {
        let db_weight = T::DbWeight::get();
        let mut weight = db_weight.reads(1);
        let max = T::MaxPendingRetryExpirationsPerBlock::get() as usize;
        if max == 0 {
            return weight;
        }

        let pending: sp_std::vec::Vec<_> = PendingExecutionRetryExpirations::<T>::iter()
            .take(max)
            .collect();
        for (proposal_id, retry_deadline) in pending {
            weight = weight.saturating_add(db_weight.reads_writes(3, 4));
            let Some(state) = ProposalExecutionRetryStates::<T>::get(proposal_id) else {
                PendingExecutionRetryExpirations::<T>::remove(proposal_id);
                continue;
            };
            if state.retry_deadline > now {
                if Self::reschedule_execution_retry_deadline(proposal_id, state.retry_deadline)
                    .is_ok()
                {
                    PendingExecutionRetryExpirations::<T>::remove(proposal_id);
                } else {
                    PendingExecutionRetryExpirations::<T>::insert(
                        proposal_id,
                        state.retry_deadline,
                    );
                }
                continue;
            }
            let Some(proposal) = Proposals::<T>::get(proposal_id) else {
                ProposalExecutionRetryStates::<T>::remove(proposal_id);
                PendingExecutionRetryExpirations::<T>::remove(proposal_id);
                continue;
            };
            if proposal.status != STATUS_PASSED {
                ProposalExecutionRetryStates::<T>::remove(proposal_id);
                PendingExecutionRetryExpirations::<T>::remove(proposal_id);
                continue;
            }

            let result = with_transaction(|| {
                let result = (|| -> DispatchResult {
                    Self::set_proposal_status(proposal_id, STATUS_EXECUTION_FAILED)?;
                    Self::deposit_event(Event::<T>::ProposalExecutionRetryExpired { proposal_id });
                    Self::finish_terminal_status(proposal_id, STATUS_EXECUTION_FAILED)
                })();
                match result {
                    Ok(()) => TransactionOutcome::Commit(Ok(())),
                    Err(err) => TransactionOutcome::Rollback(Err(err)),
                }
            });
            if result.is_ok() {
                PendingExecutionRetryExpirations::<T>::remove(proposal_id);
            } else {
                PendingExecutionRetryExpirations::<T>::insert(proposal_id, retry_deadline);
            }
        }
        weight
    }

    pub(crate) fn retry_passed_proposal_inner(
        who: &T::AccountId,
        proposal_id: u64,
    ) -> DispatchResult {
        with_transaction(|| {
            let result = (|| -> DispatchResult {
                Self::ensure_retry_admin(who, proposal_id)?;
                let proposal =
                    Proposals::<T>::get(proposal_id).ok_or(Error::<T>::ProposalNotFound)?;
                ensure!(
                    proposal.status == STATUS_PASSED,
                    Error::<T>::ProposalNotRetryable
                );
                let mut state = ProposalExecutionRetryStates::<T>::get(proposal_id)
                    .ok_or(Error::<T>::ProposalNotRetryable)?;
                let now = frame_system::Pallet::<T>::block_number();
                ensure!(
                    now <= state.retry_deadline,
                    Error::<T>::ExecutionRetryDeadlinePassed
                );
                ensure!(
                    u32::from(state.manual_attempts) < T::MaxManualExecutionAttempts::get(),
                    Error::<T>::ManualExecutionAttemptsExceeded
                );

                let outcome = Self::with_callback_execution_scope(proposal_id, || {
                    Self::invoke_execution_callback(proposal_id, proposal.kind, true)
                })?;
                match outcome {
                    ProposalExecutionOutcome::Executed => {
                        Self::set_proposal_status(proposal_id, STATUS_EXECUTED)?;
                        <T::TrackHandlers as crate::tracks::ProposalTracks<
                            BlockNumberFor<T>,
                            T::AccountId,
                        >>::on_proposal_executed(proposal.kind, proposal_id)
                        .ok_or(Error::<T>::InvalidProposalStage)??;
                        Self::deposit_event(Event::<T>::ProposalExecutionRetried {
                            proposal_id,
                            manual_attempts: state.manual_attempts,
                            outcome: STATUS_EXECUTED,
                        });
                        Self::finish_terminal_status(proposal_id, STATUS_EXECUTED)
                    }
                    ProposalExecutionOutcome::RetryableFailed => {
                        state.manual_attempts = state.manual_attempts.saturating_add(1);
                        state.last_attempt_at = Some(now);
                        if u32::from(state.manual_attempts) >= T::MaxManualExecutionAttempts::get()
                        {
                            Self::set_proposal_status(proposal_id, STATUS_EXECUTION_FAILED)?;
                            Self::deposit_event(Event::<T>::ProposalExecutionRetried {
                                proposal_id,
                                manual_attempts: state.manual_attempts,
                                outcome: STATUS_EXECUTION_FAILED,
                            });
                            Self::finish_terminal_status(proposal_id, STATUS_EXECUTION_FAILED)
                        } else {
                            Self::deposit_event(Event::<T>::ProposalExecutionRetried {
                                proposal_id,
                                manual_attempts: state.manual_attempts,
                                outcome: STATUS_PASSED,
                            });
                            ProposalExecutionRetryStates::<T>::insert(proposal_id, state);
                            Ok(())
                        }
                    }
                    ProposalExecutionOutcome::FatalFailed => {
                        Self::set_proposal_status(proposal_id, STATUS_EXECUTION_FAILED)?;
                        Self::deposit_event(Event::<T>::ProposalExecutionRetried {
                            proposal_id,
                            manual_attempts: state.manual_attempts,
                            outcome: STATUS_EXECUTION_FAILED,
                        });
                        Self::finish_terminal_status(proposal_id, STATUS_EXECUTION_FAILED)
                    }
                    ProposalExecutionOutcome::Ignored => {
                        Err(Error::<T>::ProposalOwnerMissing.into())
                    }
                }
            })();
            match result {
                Ok(()) => TransactionOutcome::Commit(Ok(())),
                Err(err) => TransactionOutcome::Rollback(Err(err)),
            }
        })
    }

    pub(crate) fn cancel_passed_proposal_inner(
        who: &T::AccountId,
        proposal_id: u64,
    ) -> DispatchResult {
        with_transaction(|| {
            let result = (|| -> DispatchResult {
                Self::ensure_retry_admin(who, proposal_id)?;
                let proposal =
                    Proposals::<T>::get(proposal_id).ok_or(Error::<T>::ProposalNotFound)?;
                ensure!(
                    proposal.status == STATUS_PASSED,
                    Error::<T>::ProposalNotRetryable
                );
                Self::can_cancel_passed_proposal_by_owner(proposal_id, proposal.kind)?;
                Self::set_proposal_status(proposal_id, STATUS_EXECUTION_FAILED)?;
                Self::deposit_event(Event::<T>::ProposalExecutionCancelled { proposal_id });
                Self::finish_terminal_status(proposal_id, STATUS_EXECUTION_FAILED)
            })();
            match result {
                Ok(()) => TransactionOutcome::Commit(Ok(())),
                Err(err) => TransactionOutcome::Rollback(Err(err)),
            }
        })
    }
}
