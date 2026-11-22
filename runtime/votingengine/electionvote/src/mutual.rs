//! 从岗位有效任职冻结互选资格，创建互选并执行投票。

use frame_support::pallet_prelude::DispatchResult;

use crate::pallet::{CitizenSubjectOf, Config, Pallet};

impl<T: Config> Pallet<T> {
    #[allow(clippy::too_many_arguments)]
    pub fn do_create_mutual_election(
        who: T::AccountId,
        vote_plan: engingcore::types::VotePlanOf<T::AccountId>,
        actor_cid_number: engingcore::types::CidNumber,
        role_code: engingcore::types::RoleCode,
        seat_count: u16,
        term_start: u32,
        term_end: u32,
        candidates: sp_std::vec::Vec<CitizenSubjectOf<T>>,
    ) -> Result<u128, sp_runtime::DispatchError> {
        Self::do_create_election(
            who,
            vote_plan,
            crate::types::ElectionMode::Mutual,
            actor_cid_number,
            role_code,
            seat_count,
            term_start,
            term_end,
            None,
            candidates,
        )
    }

    pub fn do_cast_mutual_vote(
        who: T::AccountId,
        proposal_id: u128,
        voter_role_code: engingcore::types::RoleCode,
        candidate_subject: CitizenSubjectOf<T>,
    ) -> DispatchResult {
        Self::do_cast_election_vote(
            who,
            proposal_id,
            engingcore::STAGE_ELECTION_MUTUAL,
            Some(voter_role_code),
            candidate_subject,
        )
    }
}
