//! 公投数学判定统一委托 constitution 单源。

pub(crate) fn passed(eligible: u64, yes: u64, no: u64) -> bool {
    primitives::constitution::referendum_passed(eligible, yes, no)
}
use crate::*;

impl<T: Config> Pallet<T> {
    /// 内部全过 → 推进至强制公投阶段(对标 joint advance_to_referendum)。
    pub(crate) fn advance_to_referendum(proposal_id: u64) -> DispatchResult {
        let now = <frame_system::Pallet<T>>::block_number();
        let end = now.saturating_add(Self::stage_duration());
        with_transaction(|| {
            let eligible_total =
                match <votingengine::Pallet<T>>::population_eligible_total_of(proposal_id) {
                    Some(total) => total,
                    None => {
                        return TransactionOutcome::Rollback(Err(
                            Error::<T>::CitizenEligibleTotalNotSet.into(),
                        ))
                    }
                };
            let old_end = match Proposals::<T>::try_mutate(
                proposal_id,
                |maybe| -> Result<frame_system::pallet_prelude::BlockNumberFor<T>, DispatchError> {
                    let p = maybe
                        .as_mut()
                        .ok_or(votingengine::Error::<T>::ProposalNotFound)?;
                    let old = p.end;
                    p.stage = STAGE_LEG_REFERENDUM;
                    p.start = now;
                    p.end = end;
                    Ok(old)
                },
            ) {
                Ok(v) => v,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };
            let old_expiry = old_end.saturating_add(One::one());
            ProposalsByExpiry::<T>::mutate(old_expiry, |ids| ids.retain(|&i| i != proposal_id));
            if let Err(err) = <votingengine::Pallet<T>>::schedule_proposal_expiry(proposal_id, end)
            {
                return TransactionOutcome::Rollback(Err(err));
            }
            <votingengine::Pallet<T>>::release_internal_proposal_mutexes(proposal_id);
            <votingengine::Pallet<T>>::emit_proposal_advanced_to_referendum(
                proposal_id,
                end,
                eligible_total,
            );
            Self::deposit_event(pallet::Event::<T>::LegislationAdvancedToReferendum {
                proposal_id,
                eligible_total,
            });
            TransactionOutcome::Commit(Ok(()))
        })
    }
}

impl<T: Config> Pallet<T> {
    /// 公投投票：读取快照时完整公民主体并按永久 CID 去重（期满计票）。
    pub fn do_cast_referendum_vote(
        who: T::AccountId,
        proposal_id: u64,
        approve: bool,
    ) -> DispatchResult {
        let proposal = <votingengine::Pallet<T>>::ensure_open_proposal(proposal_id)?;
        ensure!(
            proposal.kind == PROPOSAL_KIND_LEGISLATION,
            votingengine::Error::<T>::InvalidProposalKind
        );
        ensure!(
            proposal.stage == STAGE_LEG_REFERENDUM,
            votingengine::Error::<T>::InvalidProposalStage
        );
        let eligible_total = <votingengine::Pallet<T>>::population_eligible_total_of(proposal_id)
            .ok_or(Error::<T>::CitizenEligibleTotalNotSet)?;
        ensure!(eligible_total > 0, Error::<T>::CitizenEligibleTotalNotSet);
        let voter_subject =
            <votingengine::Pallet<T>>::voting_subject_at_population_snapshot(proposal_id, &who)
                .ok_or(Error::<T>::CitizenNotEligible)?;
        let voter_cid_number = voter_subject.cid_number.clone();
        ensure!(
            !pallet::LegReferendumVotesByCid::<T>::contains_key(proposal_id, &voter_cid_number,),
            votingengine::Error::<T>::AlreadyVoted
        );
        let current_tally = pallet::LegReferendumTally::<T>::get(proposal_id);
        ensure!(
            current_tally.yes.saturating_add(current_tally.no) < eligible_total,
            Error::<T>::ReferendumSnapshotExhausted
        );

        pallet::LegReferendumVotesByCid::<T>::insert(
            proposal_id,
            voter_cid_number,
            votingengine::CitizenReferendumTicket {
                voter_subject: voter_subject.clone(),
                approve,
            },
        );
        pallet::LegReferendumTally::<T>::mutate(proposal_id, |t| {
            if approve {
                t.yes = t.yes.saturating_add(1);
            } else {
                t.no = t.no.saturating_add(1);
            }
        });
        Self::deposit_event(pallet::Event::<T>::LegislationReferendumVoteCast {
            proposal_id,
            voter_subject,
            approve,
        });
        Ok(())
    }

    /// 公投阶段超时结算:按宪法 ≥70% 参与 + ≥70% 赞成判定。
    pub fn do_finalize_referendum_timeout(
        proposal: &Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        proposal_id: u64,
    ) -> DispatchResult {
        ensure!(
            proposal.stage == STAGE_LEG_REFERENDUM,
            votingengine::Error::<T>::InvalidProposalStage
        );
        ensure!(
            proposal.status == STATUS_VOTING,
            votingengine::Error::<T>::ProposalAlreadyFinalized
        );
        ensure!(
            <frame_system::Pallet<T>>::block_number() > proposal.end,
            votingengine::Error::<T>::VoteNotExpired
        );
        let tally = pallet::LegReferendumTally::<T>::get(proposal_id);
        let eligible_total = <votingengine::Pallet<T>>::population_eligible_total_of(proposal_id)
            .ok_or(Error::<T>::CitizenEligibleTotalNotSet)?;
        if crate::legislation::referendum::passed(eligible_total, tally.yes, tally.no) {
            // 公投通过:修宪(特别案)转护宪大法官终审,否则直接生效。
            let meta = pallet::LegislationMetas::<T>::get(proposal_id)
                .ok_or(Error::<T>::ProposalMetaMissing)?;
            Self::finalize_or_guard(proposal_id, meta.needs_guard)
        } else {
            <votingengine::Pallet<T>>::set_status_and_emit(proposal_id, STATUS_REJECTED)
        }
    }
}
