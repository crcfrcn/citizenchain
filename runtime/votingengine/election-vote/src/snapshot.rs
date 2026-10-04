//! 选举快照校验与写入。
//!
//! 投票期间只读取创建时固化的候选人/选民快照，
//! 不实时回读业务模块状态，避免换届、管理员变更或 CID 数据更新影响已发起选举。

use frame_support::{ensure, pallet_prelude::*};
use sp_runtime::DispatchError;
use votingengine::CitizenIdentityReader;

use crate::pallet::{
    CitizenSubjectOf, Config, ElectionCandidates, Error, MaxElectionCandidatesOf, Pallet,
};

impl<T: Config> Pallet<T> {
    pub(crate) fn ensure_unique_candidate_cids(
        candidates: &[CitizenSubjectOf<T>],
    ) -> DispatchResult {
        for i in 0..candidates.len() {
            for j in i.saturating_add(1)..candidates.len() {
                ensure!(
                    candidates[i].cid_number != candidates[j].cid_number,
                    Error::<T>::DuplicateCandidateCid
                );
            }
        }
        Ok(())
    }

    pub(crate) fn bounded_candidates(
        candidates: sp_std::vec::Vec<CitizenSubjectOf<T>>,
        scope: &votingengine::PopulationScope,
    ) -> Result<BoundedVec<CitizenSubjectOf<T>, MaxElectionCandidatesOf<T>>, DispatchError> {
        ensure!(!candidates.is_empty(), Error::<T>::EmptyCandidateSnapshot);
        Self::ensure_unique_candidate_cids(&candidates)?;
        ensure!(
            candidates.iter().all(|candidate| {
                <T as votingengine::Config>::CitizenIdentityReader::candidate_subject(
                    &candidate.account_id,
                    scope,
                )
                .as_ref()
                    == Some(candidate)
            }),
            Error::<T>::CandidateSubjectInvalid
        );
        candidates
            .try_into()
            .map_err(|_| Error::<T>::TooManyCandidates.into())
    }

    pub(crate) fn candidate_exists(
        proposal_id: u64,
        candidate_subject: &CitizenSubjectOf<T>,
    ) -> bool {
        ElectionCandidates::<T>::get(proposal_id)
            .map(|items| items.iter().any(|item| item == candidate_subject))
            .unwrap_or(false)
    }
}
