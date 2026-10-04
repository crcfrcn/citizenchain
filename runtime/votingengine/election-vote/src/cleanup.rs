//! election-vote 存储清理。
//!
//! 核心 votingengine 只维护清理状态机，具体选票、候选、计票账本
//! 住在 election-vote，因此通过 `ElectionCleanupHandler` 派发到这里分块删除。

use crate::pallet::{
    ElectionCandidateTallies, ElectionCandidates, ElectionMetaStore, ElectionResults,
    ElectionTallyStore, MutualElectionVotesByTicket, PopularElectionVotesByCid,
};
use crate::weights::WeightInfo;
use frame_support::{traits::Get, weights::Weight};

impl<T: crate::pallet::Config> votingengine::ElectionCleanupHandler for crate::pallet::Pallet<T> {
    fn cleanup_election_votes_chunk(
        proposal_id: u64,
        limit: u32,
    ) -> votingengine::CleanupChunkResult {
        let popular = PopularElectionVotesByCid::<T>::clear_prefix(proposal_id, limit, None);
        if popular.maybe_cursor.is_some() || popular.unique >= limit {
            return (popular.unique, true);
        }
        let remaining = limit.saturating_sub(popular.unique);
        let mutual = MutualElectionVotesByTicket::<T>::clear_prefix(proposal_id, remaining, None);
        (
            popular.unique.saturating_add(mutual.unique),
            mutual.maybe_cursor.is_some(),
        )
    }

    fn cleanup_election_voters_chunk(
        proposal_id: u64,
        limit: u32,
    ) -> votingengine::CleanupChunkResult {
        let _ = (proposal_id, limit);
        // 互选岗位快照统一由 votingengine 核心 VoterSnapshot 清理阶段处理。
        (0, false)
    }

    fn cleanup_election_tallies_chunk(
        proposal_id: u64,
        limit: u32,
    ) -> votingengine::CleanupChunkResult {
        let result = ElectionCandidateTallies::<T>::clear_prefix(proposal_id, limit, None);
        if result.maybe_cursor.is_none() {
            ElectionTallyStore::<T>::remove(proposal_id);
        }
        (result.unique, result.maybe_cursor.is_some())
    }

    fn cleanup_election_terminal(proposal_id: u64) {
        ElectionMetaStore::<T>::remove(proposal_id);
        ElectionCandidates::<T>::remove(proposal_id);
        ElectionResults::<T>::remove(proposal_id);
    }
}

impl<T: crate::pallet::Config>
    votingengine::ProposalTrackHandler<
        frame_system::pallet_prelude::BlockNumberFor<T>,
        T::AccountId,
    > for crate::pallet::Pallet<T>
{
    fn handles(kind: u8) -> bool {
        kind == votingengine::PROPOSAL_KIND_ELECTION
    }

    fn finalize_timeout(
        proposal: &votingengine::Proposal<
            frame_system::pallet_prelude::BlockNumberFor<T>,
            T::AccountId,
        >,
        proposal_id: u64,
    ) -> Option<frame_support::dispatch::DispatchResult> {
        use votingengine::ElectionProposalFinalizer;

        if !Self::handles(proposal.kind) {
            return None;
        }
        Some(match proposal.stage {
            votingengine::STAGE_ELECTION_POPULAR => {
                Self::finalize_election_popular_timeout(proposal, proposal_id)
            }
            votingengine::STAGE_ELECTION_MUTUAL => {
                Self::finalize_election_mutual_timeout(proposal, proposal_id)
            }
            _ => Err(votingengine::Error::<T>::InvalidProposalStage.into()),
        })
    }

    fn cleanup_chunk(
        kind: u8,
        proposal_id: u64,
        limit: u32,
    ) -> Option<votingengine::CleanupChunkResult> {
        if !Self::handles(kind) {
            return None;
        }
        let limit = limit.max(1);
        let (votes, votes_more) =
            <Self as votingengine::ElectionCleanupHandler>::cleanup_election_votes_chunk(
                proposal_id,
                limit,
            );
        if votes_more || votes >= limit {
            return Some((votes, true));
        }
        let (voters, voters_more) =
            <Self as votingengine::ElectionCleanupHandler>::cleanup_election_voters_chunk(
                proposal_id,
                limit.saturating_sub(votes),
            );
        let removed = votes.saturating_add(voters);
        if voters_more || removed >= limit {
            return Some((removed, true));
        }
        let (tallies, tallies_more) =
            <Self as votingengine::ElectionCleanupHandler>::cleanup_election_tallies_chunk(
                proposal_id,
                limit.saturating_sub(removed),
            );
        Some((removed.saturating_add(tallies), tallies_more))
    }

    fn cleanup_terminal(kind: u8, proposal_id: u64) -> Option<()> {
        Self::handles(kind).then(|| {
            <Self as votingengine::ElectionCleanupHandler>::cleanup_election_terminal(proposal_id)
        })
    }

    fn timeout_weight(stage: u8) -> Option<Weight> {
        let candidates = T::MaxElectionCandidates::get();
        match stage {
            votingengine::STAGE_ELECTION_POPULAR => Some(
                <T as crate::pallet::Config>::WeightInfo::cast_popular_vote(candidates),
            ),
            votingengine::STAGE_ELECTION_MUTUAL => Some(
                <T as crate::pallet::Config>::WeightInfo::cast_mutual_vote(candidates),
            ),
            u8::MAX => Some(
                <T as crate::pallet::Config>::WeightInfo::cast_popular_vote(candidates).max(
                    <T as crate::pallet::Config>::WeightInfo::cast_mutual_vote(candidates),
                ),
            ),
            _ => None,
        }
    }

    fn cleanup_chunk_weight(kind: u8, limit: u32) -> Option<Weight> {
        matches!(kind, votingengine::PROPOSAL_KIND_ELECTION | u8::MAX).then(|| {
            let limit = u64::from(limit.max(1));
            Weight::from_parts(12_000_000, 6_000)
                .saturating_add(Weight::from_parts(1_200_000, 2_700).saturating_mul(limit))
                .saturating_add(
                    T::DbWeight::get()
                        .reads_writes(limit.saturating_add(3), limit.saturating_add(1)),
                )
        })
    }

    fn cleanup_terminal_weight(kind: u8) -> Option<Weight> {
        matches!(kind, votingengine::PROPOSAL_KIND_ELECTION | u8::MAX).then(|| {
            Weight::from_parts(10_000_000, 10_000).saturating_add(T::DbWeight::get().writes(3))
        })
    }
}
