//! 内部投票票据、阈值和终态辅助数据清理。

use super::*;
use crate::weights::WeightInfo;
use frame_support::weights::Weight;

impl<T: Config> votebase::traits::InternalCleanupHandler for Pallet<T> {
    fn on_internal_proposal_executed(proposal_id: u64) -> DispatchResult {
        Self::apply_executed_threshold_side_effect(proposal_id)
    }

    fn on_internal_proposal_terminal(proposal_id: u64, status: u8) -> DispatchResult {
        Self::apply_terminal_threshold_cleanup(proposal_id, status)
    }

    fn cleanup_internal_votes_chunk(
        proposal_id: u64,
        limit: u32,
    ) -> votebase::traits::CleanupChunkResult {
        let result = InternalVotesByTicket::<T>::clear_prefix(proposal_id, limit, None);
        let has_remaining = result.maybe_cursor.is_some();
        (result.unique, has_remaining)
    }

    fn cleanup_internal_terminal(proposal_id: u64) {
        InternalTallies::<T>::remove(proposal_id);
        InternalThresholdSnapshot::<T>::remove(proposal_id);
        PendingPersonalThresholds::<T>::remove(proposal_id);
        PendingPersonalAdminChangeThresholds::<T>::remove(proposal_id);
        InternalProposalRoles::<T>::remove(proposal_id);
    }
}

impl<T: Config>
    votebase::ProposalTrackHandler<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>
    for Pallet<T>
{
    fn handles(kind: u8) -> bool {
        kind == votebase::PROPOSAL_KIND_INTERNAL
    }

    fn finalize_timeout(
        proposal: &Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        proposal_id: u64,
    ) -> Option<DispatchResult> {
        if !Self::handles(proposal.kind) {
            return None;
        }
        Some(match proposal.stage {
            votebase::STAGE_INTERNAL => Self::do_finalize_internal_timeout(proposal, proposal_id),
            _ => Err(votebase::Error::<T>::InvalidProposalStage.into()),
        })
    }

    fn cleanup_chunk(
        kind: u8,
        proposal_id: u64,
        limit: u32,
    ) -> Option<votebase::CleanupChunkResult> {
        Self::handles(kind).then(|| {
            <Self as votebase::InternalCleanupHandler>::cleanup_internal_votes_chunk(
                proposal_id,
                limit,
            )
        })
    }

    fn cleanup_terminal(kind: u8, proposal_id: u64) -> Option<()> {
        Self::handles(kind).then(|| {
            <Self as votebase::InternalCleanupHandler>::cleanup_internal_terminal(proposal_id)
        })
    }

    fn timeout_weight(stage: u8) -> Option<Weight> {
        matches!(stage, votebase::STAGE_INTERNAL | u8::MAX)
            .then(<T as Config>::WeightInfo::finalize_internal_timeout)
    }

    fn cleanup_chunk_weight(kind: u8, limit: u32) -> Option<Weight> {
        use crate::weights::WeightInfo;
        matches!(kind, votebase::PROPOSAL_KIND_INTERNAL | u8::MAX)
            .then(|| <T as crate::pallet::Config>::WeightInfo::cleanup_votes(limit.max(1)))
    }

    fn cleanup_terminal_weight(kind: u8) -> Option<Weight> {
        use crate::weights::WeightInfo;
        matches!(kind, votebase::PROPOSAL_KIND_INTERNAL | u8::MAX)
            .then(<T as crate::pallet::Config>::WeightInfo::cleanup_terminal)
    }

    fn on_proposal_executed(kind: u8, proposal_id: u64) -> Option<DispatchResult> {
        Self::handles(kind).then(|| {
            <Self as votebase::InternalCleanupHandler>::on_internal_proposal_executed(proposal_id)
        })
    }

    fn on_proposal_terminal(kind: u8, proposal_id: u64, status: u8) -> Option<DispatchResult> {
        Self::handles(kind).then(|| {
            <Self as votebase::InternalCleanupHandler>::on_internal_proposal_terminal(
                proposal_id,
                status,
            )
        })
    }
}
