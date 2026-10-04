//! 多候选、多席位计票。
//!
//! 当前实现“得票多数当选”的最小规则：并列组能够完整落入剩余席位时，
//! 并列账户共同当选；只有并列组跨越席位边界时才拒绝结果。

use frame_support::pallet_prelude::*;
use sp_std::vec::Vec;

use crate::pallet::{
    Config, ElectionCandidateTallies, ElectionCandidates, ElectionResults, Error,
    MaxElectionCandidatesOf, Pallet,
};
use crate::types::ElectionWinner;

/// 纯函数：从候选人顺序快照和票数中选出席位。
pub(crate) fn select_winners<AccountId: Clone + PartialEq>(
    candidates: &[(votingengine::CitizenSubject<AccountId>, u32)],
    seat_count: u16,
) -> Result<Vec<ElectionWinner<AccountId>>, ()> {
    let mut remaining: Vec<(votingengine::CitizenSubject<AccountId>, u32)> = candidates.to_vec();
    let mut winners: Vec<ElectionWinner<AccountId>> = Vec::new();

    let mut seat_index = 0u16;
    while seat_index < seat_count {
        let Some(max_votes) = remaining.iter().map(|(_, votes)| *votes).max() else {
            return Err(());
        };
        if max_votes == 0 {
            return Err(());
        }

        let tied: Vec<usize> = remaining
            .iter()
            .enumerate()
            .filter_map(|(idx, (_, votes))| (*votes == max_votes).then_some(idx))
            .collect();
        let remaining_seats = seat_count.saturating_sub(seat_index) as usize;
        if tied.len() > remaining_seats {
            return Err(());
        }

        let tied_count = tied.len() as u16;
        for (offset, idx) in tied.into_iter().enumerate() {
            let (candidate_subject, votes) = remaining.remove(idx - offset);
            winners.push(ElectionWinner {
                candidate_subject,
                votes,
                seat_index: seat_index + offset as u16,
            });
        }
        seat_index = seat_index.saturating_add(tied_count);
    }

    Ok(winners)
}

impl<T: Config> Pallet<T> {
    pub(crate) fn finalize_election_result(proposal_id: u64) -> DispatchResult {
        let meta = crate::pallet::ElectionMetaStore::<T>::get(proposal_id)
            .ok_or(Error::<T>::ElectionMetaMissing)?;
        let candidates =
            ElectionCandidates::<T>::get(proposal_id).ok_or(Error::<T>::EmptyCandidateSnapshot)?;

        let candidate_votes: Vec<(votingengine::CitizenSubject<T::AccountId>, u32)> = candidates
            .iter()
            .cloned()
            .map(|candidate_subject| {
                let votes =
                    ElectionCandidateTallies::<T>::get(proposal_id, &candidate_subject.cid_number);
                (candidate_subject, votes)
            })
            .collect();

        let winners = match select_winners(&candidate_votes, meta.seat_count) {
            Ok(winners) => winners,
            Err(()) => {
                Self::deposit_event(crate::pallet::Event::<T>::ElectionRejectedByTieOrNoVotes {
                    proposal_id,
                });
                return votingengine::Pallet::<T>::set_status_and_emit(
                    proposal_id,
                    votingengine::STATUS_REJECTED,
                );
            }
        };

        let bounded: BoundedVec<ElectionWinner<T::AccountId>, MaxElectionCandidatesOf<T>> = winners
            .try_into()
            .map_err(|_| Error::<T>::TooManyCandidates)?;
        ElectionResults::<T>::insert(proposal_id, bounded);
        Self::deposit_event(crate::pallet::Event::<T>::ElectionResultReady { proposal_id });
        votingengine::Pallet::<T>::set_status_and_emit(proposal_id, votingengine::STATUS_PASSED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject(id: u8) -> votingengine::CitizenSubject<u8> {
        votingengine::CitizenSubject {
            cid_number: vec![id].try_into().expect("test CID fits"),
            account_id: id,
        }
    }

    #[test]
    fn selects_top_seats_without_tie() {
        let winners = select_winners(&[(subject(1), 9), (subject(2), 5), (subject(3), 7)], 2)
            .expect("unique winners");
        assert_eq!(winners[0].candidate_subject, subject(1));
        assert_eq!(winners[1].candidate_subject, subject(3));
    }

    #[test]
    fn accepts_tie_that_fits_remaining_seats() {
        let winners = select_winners(&[(subject(1), 9), (subject(2), 9), (subject(3), 1)], 2)
            .expect("tie fits");
        assert_eq!(winners.len(), 2);
        assert_eq!(winners[0].seat_index, 0);
        assert_eq!(winners[1].seat_index, 1);
    }

    #[test]
    fn rejects_tie_that_crosses_seat_boundary() {
        assert!(select_winners(&[(subject(1), 9), (subject(2), 8), (subject(3), 8)], 2).is_err());
    }
}
