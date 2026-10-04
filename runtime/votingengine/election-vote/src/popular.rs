//! 普选入口。
//!
//! 普选的职位、任期、候选来源、选民范围由业务模块解释后传入。
//! 本文件只把这些数据固化成快照并创建 election-vote 提案。

use frame_support::pallet_prelude::DispatchResult;

use crate::pallet::{CitizenSubjectOf, Config, Pallet};

impl<T: Config> Pallet<T> {
    #[allow(clippy::too_many_arguments)]
    pub fn do_create_popular_election(
        who: T::AccountId,
        vote_plan: votingengine::types::VotePlanOf<T::AccountId>,
        actor_cid_number: votingengine::types::CidNumber,
        role_code: votingengine::types::RoleCode,
        seat_count: u16,
        term_start: u32,
        term_end: u32,
        population_scope: votingengine::PopulationScope,
        candidates: sp_std::vec::Vec<CitizenSubjectOf<T>>,
    ) -> Result<u64, sp_runtime::DispatchError> {
        Self::do_create_election(
            who,
            vote_plan,
            crate::types::ElectionMode::Popular,
            actor_cid_number,
            role_code,
            seat_count,
            term_start,
            term_end,
            Some(population_scope),
            candidates,
        )
    }

    pub fn do_cast_popular_vote(
        who: T::AccountId,
        proposal_id: u64,
        candidate_subject: CitizenSubjectOf<T>,
    ) -> DispatchResult {
        Self::do_cast_election_vote(
            who,
            proposal_id,
            votingengine::STAGE_ELECTION_POPULAR,
            None,
            candidate_subject,
        )
    }
}
