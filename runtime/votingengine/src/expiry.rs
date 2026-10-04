//! 提案到期索引、自动终结、有限退避和 dead-letter。

use crate::pallet::*;
use crate::weights::WeightInfo;
use crate::*;
use frame_support::{ensure, pallet_prelude::*, traits::Get, weights::Weight};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::{
    traits::{One, SaturatedConversion, Saturating},
    DispatchError,
};
use sp_std::vec::Vec;

impl<T: Config> Pallet<T> {
    /// 按提案 kind/stage 查询 Track 的超时成本；配置缺失时返回零，dispatch
    /// 随后仍会以 `InvalidProposalStage` 失败，不会伪造成功。
    pub fn track_timeout_weight(proposal_id: u64) -> Weight {
        let Some(proposal) = Proposals::<T>::get(proposal_id) else {
            return Weight::zero();
        };
        <T::TrackHandlers as crate::tracks::ProposalTracks<
            BlockNumberFor<T>,
            T::AccountId,
        >>::timeout_weight(proposal.kind, proposal.stage)
        .unwrap_or_default()
    }

    /// 把业务模块传入的机构 CID 列表转为链上有界主体集合。
    ///
    /// CID 是机构类提案归属唯一真源;机构码和账户都不能替代 CID。
    /// 个人多签没有 CID,调用方应传空列表。
    pub fn bound_subject_cid_numbers(
        subject_cid_numbers: Vec<Vec<u8>>,
    ) -> Result<ProposalSubjectCidNumbers, DispatchError> {
        let mut out = ProposalSubjectCidNumbers::default();
        for raw in subject_cid_numbers {
            ensure!(!raw.is_empty(), Error::<T>::InvalidInstitution);
            let cid: CidNumber = raw.try_into().map_err(|_| Error::<T>::InvalidInstitution)?;
            if !out.iter().any(|existing| existing == &cid) {
                out.try_push(cid)
                    .map_err(|_| Error::<T>::InvalidInstitution)?;
            }
        }
        Ok(out)
    }
    // sub-pallet 调用的事件 emit helper(do_X 搬到 sub-pallet 后,
    // 仍需要发 votingengine 自己的 lifecycle event)
    pub fn emit_proposal_created(proposal_id: u64, kind: u8, stage: u8, end: BlockNumberFor<T>) {
        Self::deposit_event(Event::<T>::ProposalCreated {
            proposal_id,
            kind,
            stage,
            end,
        });
    }

    pub fn emit_proposal_advanced_to_referendum(
        proposal_id: u64,
        referendum_end: BlockNumberFor<T>,
        eligible_total: u64,
    ) {
        Self::deposit_event(Event::<T>::ProposalAdvancedToReferendum {
            proposal_id,
            referendum_end,
            eligible_total,
        });
    }

    pub fn schedule_proposal_expiry(proposal_id: u64, end: BlockNumberFor<T>) -> DispatchResult {
        // end 表示“最后一个仍可投票区块”，因此超时结算应在 end+1 触发。
        let expiry = end.saturating_add(One::one());
        ProposalsByExpiry::<T>::try_mutate(expiry, |ids| {
            ids.try_push(proposal_id)
                .map_err(|_| Error::<T>::TooManyProposalsAtExpiry.into())
        })
    }

    pub(crate) fn auto_finalize_expiry_bucket(
        expiry: BlockNumberFor<T>,
        now: BlockNumberFor<T>,
        max_count: usize,
        max_weight: Weight,
    ) -> (usize, bool, Weight) {
        let db_weight = T::DbWeight::get();
        let mut weight = db_weight.reads_writes(1, 1);
        let mut proposal_ids = ProposalsByExpiry::<T>::take(expiry);
        if proposal_ids.is_empty() {
            return (0, false, weight);
        }

        let max_items = core::cmp::min(max_count, proposal_ids.len());
        let item_weight = T::WeightInfo::finalize_proposal()
            .saturating_add(<T::TrackHandlers as crate::tracks::ProposalTracks<
                BlockNumberFor<T>,
                T::AccountId,
            >>::max_timeout_weight())
            .saturating_add(db_weight.reads_writes(4, 5));
        let mut process_count = 0usize;
        let mut reserved = weight;
        while process_count < max_items {
            let next = reserved.saturating_add(item_weight);
            if next.any_gt(max_weight) {
                break;
            }
            reserved = next;
            process_count = process_count.saturating_add(1);
        }
        for proposal_id in proposal_ids.drain(..process_count) {
            weight = weight.saturating_add(db_weight.reads(1));
            let Some(proposal) = Proposals::<T>::get(proposal_id) else {
                AutoFinalizeRetryStates::<T>::remove(proposal_id);
                AutoFinalizeDeadLetters::<T>::remove(proposal_id);
                continue;
            };
            if proposal.status != STATUS_VOTING || proposal.end >= now {
                AutoFinalizeRetryStates::<T>::remove(proposal_id);
                AutoFinalizeDeadLetters::<T>::remove(proposal_id);
                continue;
            }

            weight = weight.saturating_add(
                <T::TrackHandlers as crate::tracks::ProposalTracks<
                    BlockNumberFor<T>,
                    T::AccountId,
                >>::timeout_weight(proposal.kind, proposal.stage)
                .unwrap_or_default(),
            );

            let finalize_result = <T::TrackHandlers as crate::tracks::ProposalTracks<
                BlockNumberFor<T>,
                T::AccountId,
            >>::finalize_timeout(&proposal, proposal_id)
            .unwrap_or(Err(DispatchError::Other("ProposalTrackNotConfigured")));
            if finalize_result.is_ok() {
                AutoFinalizeRetryStates::<T>::remove(proposal_id);
                AutoFinalizeDeadLetters::<T>::remove(proposal_id);
                weight = weight.saturating_add(db_weight.writes(2));
            } else {
                let mut state = AutoFinalizeRetryStates::<T>::get(proposal_id).unwrap_or(
                    crate::types::PendingExecutionState {
                        attempts: 0,
                        next_attempt_at: now,
                    },
                );
                state.attempts = state.attempts.saturating_add(1);
                let shift = core::cmp::min(u32::from(state.attempts), 6);
                let delay: BlockNumberFor<T> = (1u64 << shift).saturated_into();
                state.next_attempt_at = now.saturating_add(delay);
                let retry_scheduled = u32::from(state.attempts)
                    < T::MaxManualExecutionAttempts::get()
                    && ProposalsByExpiry::<T>::try_mutate(state.next_attempt_at, |ids| {
                        ids.try_push(proposal_id)
                            .map_err(|_| Error::<T>::TooManyProposalsAtExpiry)
                    })
                    .is_ok();
                if retry_scheduled {
                    AutoFinalizeRetryStates::<T>::insert(proposal_id, state);
                    Self::deposit_event(Event::<T>::ProposalAutoFinalizeDeferred {
                        proposal_id,
                        attempts: state.attempts,
                        next_attempt_at: state.next_attempt_at,
                    });
                } else {
                    AutoFinalizeRetryStates::<T>::remove(proposal_id);
                    AutoFinalizeDeadLetters::<T>::insert(proposal_id, state.attempts);
                    Self::deposit_event(Event::<T>::ProposalAutoFinalizeDeadLettered {
                        proposal_id,
                        attempts: state.attempts,
                    });
                }
                weight = weight.saturating_add(db_weight.reads_writes(2, 3));
            }
        }
        let has_remaining = !proposal_ids.is_empty();
        if has_remaining {
            ProposalsByExpiry::<T>::insert(expiry, proposal_ids);
            weight = weight.saturating_add(db_weight.writes(1));
        }

        // 引擎公共调度成本按实际处理条数计入；mode-specific 成本已在每条
        // proposal 分派前按 kind/stage 单独计入。
        let finalize_weight =
            T::WeightInfo::finalize_proposal().saturating_mul(process_count as u64);
        weight = weight.saturating_add(finalize_weight);

        (process_count, has_remaining, weight)
    }
}
