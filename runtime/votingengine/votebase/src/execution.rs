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
use sp_runtime::traits::{SaturatedConversion, Saturating};

impl<T: Config> Pallet<T> {
    /// 在 Runtime 共享维护预算与既有条数上界内处理通过提案。
    ///
    /// 每项组合业务实测权重、系统换码、对象读取及终态成本，避免重回调未计费。
    pub(crate) fn process_pending_proposal_executions(
        now: BlockNumberFor<T>,
        execution_budget: Weight,
    ) -> Weight {
        let max = T::MaxAutoFinalizePerBlock::get() as usize;
        if max == 0 {
            return Weight::zero();
        }
        let scan_weight = T::WeightInfo::terminal_base();
        let mut weight = T::WeightInfo::terminal_base();
        let mut pending = sp_std::vec::Vec::new();
        let mut entries = PendingProposalExecutions::<T>::iter();
        for _ in 0..max {
            if weight.saturating_add(scan_weight).any_gt(execution_budget) {
                break;
            }
            let Some((proposal_id, state)) = entries.next() else {
                break;
            };
            weight = weight.saturating_add(scan_weight);
            if state.next_attempt_at > now {
                continue;
            }
            // 基准计入框架/常用业务，系统换码与完整对象读取在消费处单独预留。
            let inspect = T::WeightInfo::terminal_item();
            if weight.saturating_add(inspect).any_gt(execution_budget) {
                break;
            }
            weight = weight.saturating_add(inspect);
            let object = ProposalObjectMeta::<T>::get(proposal_id)
                .map_or(0, |meta| u64::from(meta.object_len));
            let kind = Proposals::<T>::get(proposal_id)
                .map_or(PROPOSAL_KIND_INTERNAL, |proposal| proposal.kind);
            let item_weight = T::WeightInfo::process_pending_execution()
                .saturating_add(T::WeightInfo::cleanup_final(T::MaxInternalProposalMutexBindings::get()))
                .saturating_add(Self::terminal_weight(kind))
                .saturating_add(<<T as frame_system::Config>::SystemWeightInfo as frame_system::weights::WeightInfo>::set_code())
                .max(Self::execution_peak()).saturating_add(Weight::from_parts(0,object));
            if weight.saturating_add(item_weight).any_gt(execution_budget) {
                break;
            }
            weight = weight.saturating_add(item_weight);
            pending.push((proposal_id, state));
        }
        drop(entries);
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
    pub(crate) fn process_pending_terminal_finalizations(
        now: BlockNumberFor<T>,
        budget: Weight,
    ) -> Weight {
        let max = T::MaxPendingRetryExpirationsPerBlock::get() as usize;
        let mut weight = T::WeightInfo::terminal_base();
        let mut pending = sp_std::vec::Vec::new();
        let mut entries = PendingTerminalFinalizations::<T>::iter();
        for _ in 0..max {
            let scan = T::WeightInfo::terminal_base();
            if weight.saturating_add(scan).any_gt(budget) {
                break;
            }
            let Some((id, state)) = entries.next() else {
                break;
            };
            weight = weight.saturating_add(scan);
            if state.next_attempt_at > now {
                continue;
            }
            let per = Self::terminal_bound();
            if weight.saturating_add(per).any_gt(budget) {
                break;
            }
            weight = weight.saturating_add(per);
            pending.push((id, state));
        }
        drop(entries);
        for (proposal_id, mut state) in pending {
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

    /// 固定有界的截止桶先搬入既有待过期表；本块预算不足也不会丢掉到期项。
    pub(crate) fn process_execution_retry_deadlines(now: BlockNumberFor<T>) -> Weight {
        let queue = ExecutionRetryDeadlines::<T>::take(now);
        let count = queue.len() as u32;
        for proposal_id in queue {
            PendingExecutionRetryExpirations::<T>::insert(proposal_id, now);
        }
        T::WeightInfo::retry_bucket(count)
    }

    pub(crate) fn process_pending_execution_retry_expirations(
        now: BlockNumberFor<T>,
        budget: Weight,
    ) -> Weight {
        let mut weight = T::WeightInfo::terminal_base();
        let max = T::MaxPendingRetryExpirationsPerBlock::get() as usize;
        let mut pending = sp_std::vec::Vec::new();
        let cursor = RetryCursor::<T>::get();
        let mut entries = match cursor.as_ref() {
            Some(key) => PendingExecutionRetryExpirations::<T>::iter_from(key.to_vec()),
            None => PendingExecutionRetryExpirations::<T>::iter(),
        };
        let mut wrapped = false;
        let mut last = None;
        let mut scanned = 0usize;
        while scanned < max {
            let scan = T::WeightInfo::terminal_base();
            if weight.saturating_add(scan).any_gt(budget) {
                break;
            }
            let Some((id, deadline)) = entries.next() else {
                if cursor.is_some() && !wrapped && scanned == 0 {
                    entries = PendingExecutionRetryExpirations::<T>::iter();
                    wrapped = true;
                    continue;
                }
                break;
            };
            scanned += 1;
            weight = weight.saturating_add(scan);
            last = Some(id);
            if deadline > now {
                continue;
            }
            let per = Self::terminal_bound();
            if weight.saturating_add(per).any_gt(budget) {
                break;
            }
            weight = weight.saturating_add(per);
            pending.push((id, deadline));
        }
        drop(entries);
        if let Some(id) = last {
            RetryCursor::<T>::put(
                BoundedVec::<u8, ConstU32<64>>::try_from(
                    PendingExecutionRetryExpirations::<T>::hashed_key_for(id),
                )
                .expect("固定u64键游标不超过64字节"),
            );
        } else if wrapped {
            RetryCursor::<T>::kill();
        }
        for (proposal_id, retry_deadline) in pending {
            let Some(state) = ProposalExecutionRetryStates::<T>::get(proposal_id) else {
                PendingExecutionRetryExpirations::<T>::remove(proposal_id);
                continue;
            };
            if state.retry_deadline > now {
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
                let next = now.saturating_add(1u32.into());
                if Self::queue_execution_retry_deadline(proposal_id, next).is_ok() {
                    PendingExecutionRetryExpirations::<T>::remove(proposal_id);
                } else {
                    PendingExecutionRetryExpirations::<T>::insert(proposal_id, retry_deadline);
                }
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
