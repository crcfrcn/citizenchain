//! 公平就绪 FIFO 的有界清理状态机。

use crate::pallet::*;
use crate::*;
use frame_support::{traits::Get, weights::Weight};
use frame_system::pallet_prelude::BlockNumberFor;

impl<T: Config> Pallet<T> {
    pub(crate) fn process_pending_cleanup_steps(max_weight: Weight) -> Weight {
        let max_steps = T::MaxCleanupStepsPerBlock::get();
        if max_steps == 0 {
            return Weight::zero();
        }

        let cleanup_limit = T::CleanupKeysPerStep::get().max(1);
        let db = T::DbWeight::get();
        let mut weight = db.reads(2);
        let track_upper_bound = <T::TrackHandlers as crate::tracks::ProposalTracks<
            BlockNumberFor<T>,
            T::AccountId,
        >>::max_cleanup_chunk_weight(cleanup_limit)
        .max(<T::TrackHandlers as crate::tracks::ProposalTracks<
            BlockNumberFor<T>,
            T::AccountId,
        >>::max_cleanup_terminal_weight());
        let step_upper_bound = track_upper_bound.saturating_add(db.reads_writes(
            u64::from(cleanup_limit).saturating_add(8),
            u64::from(cleanup_limit).saturating_add(20),
        ));

        for _ in 0..max_steps {
            if weight.saturating_add(step_upper_bound).any_gt(max_weight) {
                break;
            }
            let head = PendingCleanupQueueHead::<T>::get();
            let tail = PendingCleanupQueueTail::<T>::get();
            if head >= tail {
                break;
            }

            let next_head = head.saturating_add(1);
            let proposal_id = PendingCleanupQueue::<T>::take(head);
            PendingCleanupQueueHead::<T>::put(next_head);
            weight = weight.saturating_add(db.reads_writes(3, 2));
            let Some(proposal_id) = proposal_id else {
                continue;
            };
            let Some(stage) = PendingProposalCleanups::<T>::get(proposal_id) else {
                continue;
            };

            let (next_stage, step_weight) =
                Self::process_pending_cleanup_step(proposal_id, stage, cleanup_limit);
            weight = weight.saturating_add(step_weight);
            match next_stage {
                Some(next) => {
                    PendingProposalCleanups::<T>::insert(proposal_id, next);
                    if crate::cleanup::enqueue_pending_cleanup::<T>(proposal_id).is_err() {
                        // u64 序号耗尽时保留状态；创世重置前不可能达到该边界。
                        break;
                    }
                    weight = weight.saturating_add(db.reads_writes(2, 3));
                }
                None => {
                    PendingProposalCleanups::<T>::remove(proposal_id);
                    weight = weight.saturating_add(db.writes(1));
                }
            }
        }
        weight
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
