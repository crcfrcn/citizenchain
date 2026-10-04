//! 互选入口。
//!
//! 互选的选民岗位主体由业务模块通过 `VotePlan` 提交；本引擎只从 entity 任职真源
//! 读取这些岗位当前有效任职并冻结，调用方不能提交账户名单。

use frame_support::pallet_prelude::DispatchResult;

use crate::pallet::{CitizenSubjectOf, Config, Pallet};

impl<T: Config> Pallet<T> {
    #[allow(clippy::too_many_arguments)]
    pub fn do_create_mutual_election(
        who: T::AccountId,
        vote_plan: votingengine::types::VotePlanOf<T::AccountId>,
        actor_cid_number: votingengine::types::CidNumber,
        role_code: votingengine::types::RoleCode,
        seat_count: u16,
        term_start: u32,
        term_end: u32,
        candidates: sp_std::vec::Vec<CitizenSubjectOf<T>>,
    ) -> Result<u64, sp_runtime::DispatchError> {
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
        proposal_id: u64,
        voter_role_code: votingengine::types::RoleCode,
        candidate_subject: CitizenSubjectOf<T>,
    ) -> DispatchResult {
        Self::do_cast_election_vote(
            who,
            proposal_id,
            votingengine::STAGE_ELECTION_MUTUAL,
            Some(voter_role_code),
            candidate_subject,
        )
    }
}
