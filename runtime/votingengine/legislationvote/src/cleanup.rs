//! 立法机关表决本地账本清理边界。
//!
//! 具体分块清理由 `LegislationCleanupHandler` 实现统一调用，所有代表机构阶段按
//! `proposal_id` 前缀清理，不能只清理当前机构。

/// clear_prefix 返回游标时必须保留清理任务，下一块继续。
pub(crate) const fn has_more(cursor_present: bool) -> bool {
    cursor_present
}
use crate::weights::WeightInfo;
use crate::*;
use frame_support::weights::Weight;

impl<T: Config> votebase::traits::LegislationCleanupHandler for Pallet<T> {
    fn cleanup_legislation_representative_votes_chunk(
        proposal_id: u64,
        limit: u32,
    ) -> votebase::traits::CleanupChunkResult {
        let result =
            pallet::RepresentativeVotesByTicket::<T>::clear_prefix(proposal_id, limit, None);
        (
            result.unique,
            crate::cleanup::has_more(result.maybe_cursor.is_some()),
        )
    }

    fn cleanup_legislation_referendum_votes_chunk(
        proposal_id: u64,
        limit: u32,
    ) -> votebase::traits::CleanupChunkResult {
        let result = pallet::LegReferendumVotesByCid::<T>::clear_prefix(proposal_id, limit, None);
        (
            result.unique,
            crate::cleanup::has_more(result.maybe_cursor.is_some()),
        )
    }

    fn cleanup_legislation_terminal(proposal_id: u64) {
        pallet::RepresentativeMetas::<T>::remove(proposal_id);
        pallet::LegislationMetas::<T>::remove(proposal_id);
        let _ = pallet::RepresentativeTallies::<T>::clear_prefix(
            proposal_id,
            MAX_REPRESENTATIVE_BODIES,
            None,
        );
        pallet::LegReferendumTally::<T>::remove(proposal_id);
        pallet::LegOverrideSigns::<T>::remove(proposal_id);
        pallet::LegGuardSigns::<T>::remove(proposal_id);
    }
}

impl<T: Config>
    votebase::ProposalTrackHandler<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>
    for Pallet<T>
{
    fn handles(kind: u8) -> bool {
        kind == votebase::PROPOSAL_KIND_LEGISLATION
    }

    fn finalize_timeout(
        proposal: &Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        proposal_id: u64,
    ) -> Option<DispatchResult> {
        use votebase::traits::LegislationProposalFinalizer;

        if !Self::handles(proposal.kind) {
            return None;
        }
        Some(match proposal.stage {
            votebase::STAGE_LEG_REPRESENTATIVE => {
                Self::finalize_legislation_representative_timeout(proposal, proposal_id)
            }
            votebase::STAGE_LEG_REFERENDUM => {
                Self::finalize_legislation_referendum_timeout(proposal, proposal_id)
            }
            votebase::STAGE_LEG_SIGN => {
                Self::finalize_legislation_sign_timeout(proposal, proposal_id)
            }
            votebase::STAGE_LEG_OVERRIDE => {
                Self::finalize_legislation_override_timeout(proposal, proposal_id)
            }
            votebase::STAGE_LEG_CONSTITUTION_GUARD => {
                Self::finalize_legislation_guard_timeout(proposal, proposal_id)
            }
            _ => Err(votebase::Error::<T>::InvalidProposalStage.into()),
        })
    }

    fn cleanup_chunk(
        kind: u8,
        proposal_id: u64,
        limit: u32,
    ) -> Option<votebase::CleanupChunkResult> {
        if !Self::handles(kind) {
            return None;
        }
        let limit = limit.max(1);
        let (first, first_more) = <Self as votebase::LegislationCleanupHandler>::cleanup_legislation_representative_votes_chunk(
            proposal_id,
            limit,
        );
        if first_more || first >= limit {
            return Some((first, true));
        }
        let (second, second_more) = <Self as votebase::LegislationCleanupHandler>::cleanup_legislation_referendum_votes_chunk(
            proposal_id,
            limit.saturating_sub(first),
        );
        Some((first.saturating_add(second), second_more))
    }

    fn cleanup_terminal(kind: u8, proposal_id: u64) -> Option<()> {
        Self::handles(kind).then(|| {
            <Self as votebase::LegislationCleanupHandler>::cleanup_legislation_terminal(proposal_id)
        })
    }

    fn timeout_weight(stage: u8) -> Option<Weight> {
        let weight = match stage {
            votebase::STAGE_LEG_REPRESENTATIVE => {
                <T as Config>::WeightInfo::cast_representative_vote()
            }
            votebase::STAGE_LEG_REFERENDUM => <T as Config>::WeightInfo::cast_referendum_vote(),
            votebase::STAGE_LEG_SIGN => <T as Config>::WeightInfo::executive_sign(),
            votebase::STAGE_LEG_OVERRIDE => <T as Config>::WeightInfo::override_sign(),
            votebase::STAGE_LEG_CONSTITUTION_GUARD => <T as Config>::WeightInfo::guard_vote(),
            u8::MAX => <T as Config>::WeightInfo::cast_representative_vote()
                .max(<T as Config>::WeightInfo::cast_referendum_vote())
                .max(<T as Config>::WeightInfo::executive_sign())
                .max(<T as Config>::WeightInfo::override_sign())
                .max(<T as Config>::WeightInfo::guard_vote()),
            _ => return None,
        };
        Some(weight)
    }

    fn cleanup_chunk_weight(kind: u8, limit: u32) -> Option<Weight> {
        use crate::weights::WeightInfo;
        matches!(kind, votebase::PROPOSAL_KIND_LEGISLATION | u8::MAX).then(|| {
            <T as crate::pallet::Config>::WeightInfo::cleanup_representative(limit.max(1))
                .max(<T as crate::pallet::Config>::WeightInfo::cleanup_referendum(limit.max(1)))
        })
    }

    fn cleanup_terminal_weight(kind: u8) -> Option<Weight> {
        use crate::weights::WeightInfo;
        matches!(kind, votebase::PROPOSAL_KIND_LEGISLATION | u8::MAX)
            .then(<T as crate::pallet::Config>::WeightInfo::cleanup_terminal)
    }
}
