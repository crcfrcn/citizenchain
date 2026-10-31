//! 公平就绪 FIFO 的有界清理状态机。

use crate::pallet::*;
use crate::weights::WeightInfo;
use crate::*;
use frame_support::{traits::Get, weights::Weight};
use frame_system::pallet_prelude::BlockNumberFor;

impl<T: Config> Pallet<T> {
    pub(crate) fn process_pending_cleanup_steps(max_weight: Weight) -> Weight {
        let max_steps = T::MaxCleanupStepsPerBlock::get();
        if max_steps == 0 {
            return Weight::zero();
        }

        let configured = T::CleanupKeysPerStep::get().max(1);
        let mut weight = T::WeightInfo::cleanup_base();
        for _ in 0..max_steps {
            // 先预留读取队列、阶段和互斥数量的成本，预算不足不弹出就绪任务。
            let inspect = T::WeightInfo::cleanup_item();
            if weight.saturating_add(inspect).any_gt(max_weight) {
                break;
            }
            let head = PendingCleanupQueueHead::<T>::get();
            let tail = PendingCleanupQueueTail::<T>::get();
            if head >= tail {
                break;
            }
            weight = weight.saturating_add(inspect);
            let proposal_id = PendingCleanupQueue::<T>::get(head);
            let stage = proposal_id.and_then(PendingProposalCleanups::<T>::get);
            let bindings = if matches!(stage, Some(PendingCleanupStage::FinalCleanup)) {
                proposal_id
                    .and_then(ProposalMutexBindings::<T>::decode_len)
                    .unwrap_or(0) as u32
            } else {
                0
            };
            let available = max_weight.saturating_sub(weight);
            let mut low = 0u32;
            let mut high = configured;
            while low < high {
                let mid = low + (high - low + 1) / 2;
                if Self::cleanup_bound(stage, mid, bindings).any_gt(available) {
                    high = mid - 1;
                } else {
                    low = mid;
                }
            }
            if low == 0 {
                break;
            }
            let cleanup_limit = low;
            weight = weight.saturating_add(Self::cleanup_bound(stage, low, bindings));
            PendingCleanupQueue::<T>::remove(head);
            PendingCleanupQueueHead::<T>::put(head.saturating_add(1));
            let Some(proposal_id) = proposal_id else {
                continue;
            };
            let Some(stage) = stage else {
                continue;
            };

            let (next_stage, _step_weight) =
                Self::process_pending_cleanup_step(proposal_id, stage, cleanup_limit);
            match next_stage {
                Some(next) => {
                    PendingProposalCleanups::<T>::insert(proposal_id, next);
                    if crate::cleanup::enqueue_pending_cleanup::<T>(proposal_id).is_err() {
                        // u64 序号耗尽时保留状态；创世重置前不可能达到该边界。
                        break;
                    }
                }
                None => {
                    PendingProposalCleanups::<T>::remove(proposal_id);
                }
            }
        }
        weight
    }

    /// 按当前阶段和实际互斥数量预留，不让空快照承担最大终结成本。
    pub(crate) fn cleanup_bound(
        stage: Option<PendingCleanupStage>,
        limit: u32,
        bindings: u32,
    ) -> Weight {
        match stage {
            Some(
                PendingCleanupStage::AdminSnapshots
                | PendingCleanupStage::VoterSnapshots
                | PendingCleanupStage::InstitutionTicketCounts,
            ) => T::WeightInfo::cleanup_step(limit),
            Some(PendingCleanupStage::TrackData) => {
                <T::TrackHandlers as crate::tracks::ProposalTracks<
                    BlockNumberFor<T>,
                    T::AccountId,
                >>::max_cleanup_chunk_weight(limit)
            }
            Some(PendingCleanupStage::ProposalObject) => T::WeightInfo::cleanup_object(),
            Some(PendingCleanupStage::FinalCleanup) => T::WeightInfo::cleanup_final(bindings)
                .saturating_add(<T::TrackHandlers as crate::tracks::ProposalTracks<
                    BlockNumberFor<T>,
                    T::AccountId,
                >>::max_cleanup_terminal_weight()),
            None => Weight::zero(),
        }
    }

    pub(crate) fn process_pending_cleanup_step(
        proposal_id: u64,
        stage: PendingCleanupStage,
        cleanup_limit: u32,
    ) -> (Option<PendingCleanupStage>, Weight) {
        let db = T::DbWeight::get();
        match stage {
            PendingCleanupStage::AdminSnapshots => {
                let result = AdminSnapshot::<T>::clear_prefix(proposal_id, cleanup_limit, None);
                let next = if result.maybe_cursor.is_some() {
                    PendingCleanupStage::AdminSnapshots
                } else {
                    PendingCleanupStage::VoterSnapshots
                };
                (
                    Some(next),
                    db.reads_writes(u64::from(result.loops), u64::from(result.unique)),
                )
            }
            PendingCleanupStage::VoterSnapshots => {
                let result = VoterSnapshot::<T>::clear_prefix(proposal_id, cleanup_limit, None);
                let next = if result.maybe_cursor.is_some() {
                    PendingCleanupStage::VoterSnapshots
                } else {
                    PendingCleanupStage::InstitutionTicketCounts
                };
                (
                    Some(next),
                    db.reads_writes(u64::from(result.loops), u64::from(result.unique)),
                )
            }
            PendingCleanupStage::InstitutionTicketCounts => {
                let result = InstitutionTicketCountSnapshot::<T>::clear_prefix(
                    proposal_id,
                    cleanup_limit,
                    None,
                );
                let next = if result.maybe_cursor.is_some() {
                    PendingCleanupStage::InstitutionTicketCounts
                } else {
                    PendingCleanupStage::TrackData
                };
                (
                    Some(next),
                    db.reads_writes(u64::from(result.loops), u64::from(result.unique)),
                )
            }
            PendingCleanupStage::TrackData => {
                let Some(proposal) = Proposals::<T>::get(proposal_id) else {
                    return (Some(PendingCleanupStage::ProposalObject), db.reads(1));
                };
                let Some((removed, has_remaining)) =
                    <T::TrackHandlers as crate::tracks::ProposalTracks<
                        BlockNumberFor<T>,
                        T::AccountId,
                    >>::cleanup_chunk(proposal.kind, proposal_id, cleanup_limit)
                else {
                    // 配置缺失时保留任务，但公平 FIFO 仍允许其它提案继续清理。
                    return (Some(PendingCleanupStage::TrackData), db.reads(1));
                };
                let next = if has_remaining {
                    PendingCleanupStage::TrackData
                } else {
                    PendingCleanupStage::ProposalObject
                };
                let track_weight =
                    <T::TrackHandlers as crate::tracks::ProposalTracks<
                        BlockNumberFor<T>,
                        T::AccountId,
                    >>::cleanup_chunk_weight(proposal.kind, cleanup_limit)
                    .unwrap_or_default();
                (
                    Some(next),
                    track_weight.saturating_add(
                        db.reads_writes(u64::from(removed) + 1, u64::from(removed)),
                    ),
                )
            }
            PendingCleanupStage::ProposalObject => {
                ProposalObject::<T>::remove(proposal_id);
                ProposalObjectMeta::<T>::remove(proposal_id);
                (Some(PendingCleanupStage::FinalCleanup), db.writes(2))
            }
            PendingCleanupStage::FinalCleanup => {
                let Some(proposal) = Proposals::<T>::get(proposal_id) else {
                    PendingProposalExecutions::<T>::remove(proposal_id);
                    PendingTerminalFinalizations::<T>::remove(proposal_id);
                    TerminalFinalizationDeadLetters::<T>::remove(proposal_id);
                    AutoFinalizeRetryStates::<T>::remove(proposal_id);
                    AutoFinalizeDeadLetters::<T>::remove(proposal_id);
                    return (None, db.reads_writes(1, 5));
                };
                if !<T::TrackHandlers as crate::tracks::ProposalTracks<
                    BlockNumberFor<T>,
                    T::AccountId,
                >>::cleanup_terminal(proposal.kind, proposal_id)
                {
                    return (Some(PendingCleanupStage::FinalCleanup), db.reads(1));
                }
                let track_weight = <T::TrackHandlers as crate::tracks::ProposalTracks<
                    BlockNumberFor<T>,
                    T::AccountId,
                >>::cleanup_terminal_weight(proposal.kind)
                .unwrap_or_default();

                Self::release_internal_proposal_mutexes(proposal_id);
                Self::cleanup_proposal_indexes(proposal_id);
                Proposals::<T>::remove(proposal_id);
                ProposalData::<T>::remove(proposal_id);
                ProposalOwner::<T>::remove(proposal_id);
                ProposalVotePlans::<T>::remove(proposal_id);
                ProposalMeta::<T>::remove(proposal_id);
                ProposalPopulationSnapshots::<T>::remove(proposal_id);
                ProposalExecutionRetryStates::<T>::remove(proposal_id);
                PendingProposalExecutions::<T>::remove(proposal_id);
                PendingTerminalFinalizations::<T>::remove(proposal_id);
                TerminalFinalizationDeadLetters::<T>::remove(proposal_id);
                AutoFinalizeRetryStates::<T>::remove(proposal_id);
                AutoFinalizeDeadLetters::<T>::remove(proposal_id);
                (None, track_weight.saturating_add(db.reads_writes(2, 19)))
            }
        }
    }
}
