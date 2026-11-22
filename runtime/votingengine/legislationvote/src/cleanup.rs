//! 按提案分块清理全部代表机构选票、公投及终态账本。

pub(crate) const fn has_more(cursor_present: bool) -> bool {
    cursor_present
}
use crate::weights::WeightInfo;
use crate::*;
use frame_support::weights::Weight;

impl<T: Config> engingcore::traits::LegislationCleanupHandler for Pallet<T> {
    fn cleanup_legislation_representative_votes_chunk(
        proposal_id: u128,
        limit: u32,
    ) -> engingcore::traits::CleanupChunkResult {
        let result =
            pallet::RepresentativeVotesByTicket::<T>::clear_prefix(proposal_id, limit, None);
        (
            result.unique,
            crate::cleanup::has_more(result.maybe_cursor.is_some()),
        )
    }

    fn cleanup_legislation_referendum_votes_chunk(
        proposal_id: u128,
        limit: u32,
    ) -> engingcore::traits::CleanupChunkResult {
        let result = pallet::LegReferendumVotesByCid::<T>::clear_prefix(proposal_id, limit, None);
        (
            result.unique,
            crate::cleanup::has_more(result.maybe_cursor.is_some()),
        )
    }

    fn cleanup_legislation_terminal(proposal_id: u128) {
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
    engingcore::ProposalTrackHandler<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>
    for Pallet<T>
{
    fn handles(kind: u8) -> bool {
        kind == engingcore::PROPOSAL_KIND_LEGISLATION
    }

    fn finalize_timeout(
        proposal: &Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        proposal_id: u128,
    ) -> Option<DispatchResult> {
        use engingcore::traits::LegislationProposalFinalizer;

        if !Self::handles(proposal.kind) {
            return None;
        }
        Some(match proposal.stage {
            engingcore::STAGE_LEG_REPRESENTATIVE => {
                Self::finalize_legislation_representative_timeout(proposal, proposal_id)
            }
            engingcore::STAGE_LEG_REFERENDUM => {
                Self::finalize_legislation_referendum_timeout(proposal, proposal_id)
            }
            engingcore::STAGE_LEG_SIGN => {
                Self::finalize_legislation_sign_timeout(proposal, proposal_id)
            }
            engingcore::STAGE_LEG_OVERRIDE => {
                Self::finalize_legislation_override_timeout(proposal, proposal_id)
            }
            engingcore::STAGE_LEG_CONSTITUTION_GUARD => {
                Self::finalize_legislation_guard_timeout(proposal, proposal_id)
            }
            _ => Err(engingcore::Error::<T>::InvalidProposalStage.into()),
        })
    }

    fn cleanup_chunk(
        kind: u8,
        proposal_id: u128,
        limit: u32,
    ) -> Option<engingcore::CleanupChunkResult> {
        if !Self::handles(kind) {
            return None;
        }
        let limit = limit.max(1);
        let (first, first_more) = <Self as engingcore::LegislationCleanupHandler>::cleanup_legislation_representative_votes_chunk(
            proposal_id,
            limit,
        );
        if first_more || first >= limit {
            return Some((first, true));
        }
        let (second, second_more) = <Self as engingcore::LegislationCleanupHandler>::cleanup_legislation_referendum_votes_chunk(
            proposal_id,
            limit.saturating_sub(first),
        );
        Some((first.saturating_add(second), second_more))
    }

    fn cleanup_terminal(kind: u8, proposal_id: u128) -> Option<()> {
        Self::handles(kind).then(|| {
            <Self as engingcore::LegislationCleanupHandler>::cleanup_legislation_terminal(proposal_id)
        })
    }

    fn timeout_weight(stage: u8) -> Option<Weight> {
        let weight = match stage {
            engingcore::STAGE_LEG_REPRESENTATIVE => {
                <T as Config>::WeightInfo::cast_representative_vote()
            }
            engingcore::STAGE_LEG_REFERENDUM => <T as Config>::WeightInfo::cast_referendum_vote(),
            engingcore::STAGE_LEG_SIGN => <T as Config>::WeightInfo::executive_sign(),
            engingcore::STAGE_LEG_OVERRIDE => <T as Config>::WeightInfo::override_sign(),
            engingcore::STAGE_LEG_CONSTITUTION_GUARD => <T as Config>::WeightInfo::guard_vote(),
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
        matches!(kind, engingcore::PROPOSAL_KIND_LEGISLATION | u8::MAX).then(|| {
            <T as crate::pallet::Config>::WeightInfo::cleanup_representative(limit.max(1))
                .max(<T as crate::pallet::Config>::WeightInfo::cleanup_referendum(limit.max(1)))
        })
    }

    fn cleanup_terminal_weight(kind: u8) -> Option<Weight> {
        use crate::weights::WeightInfo;
        matches!(kind, engingcore::PROPOSAL_KIND_LEGISLATION | u8::MAX)
            .then(<T as crate::pallet::Config>::WeightInfo::cleanup_terminal)
    }
}
