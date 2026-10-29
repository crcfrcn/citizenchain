//! 内部投票写票、提前判定和超时终结。

use super::*;

impl<T: Config> Pallet<T> {
    pub fn do_internal_vote(
        who: T::AccountId,
        proposal_id: u64,
        ticket_claim: InternalVoteTicketClaim,
        approve: bool,
    ) -> DispatchResult {
        let proposal = <votingengine::Pallet<T>>::ensure_open_proposal(proposal_id)?;

        ensure!(
            proposal.kind == PROPOSAL_KIND_INTERNAL,
            votingengine::Error::<T>::InvalidProposalKind
        );
        ensure!(
            proposal.stage == STAGE_INTERNAL,
            votingengine::Error::<T>::InvalidProposalStage
        );
        let (ticket, voter_role_code, eligible, eligible_total) = if let Some(actor_cid_number) =
            proposal.actor_cid_number
        {
            let role_code = match ticket_claim {
                InternalVoteTicketClaim::InstitutionRole(role_code) => role_code,
                InternalVoteTicketClaim::Personal => {
                    return Err(votingengine::Error::<T>::NoPermission.into())
                }
            };
            let role_subject = votingengine::types::RoleSubject {
                cid_number: actor_cid_number.clone(),
                role_code: role_code.clone(),
            };
            let subject = AuthorizationSubject::Institution(role_subject.clone());
            // 票据按【规范账户】防重：换绑前后归并为同一张票，杜绝新旧钱包各投一票。
            // 与下面的资格校验同源，同样只解析原始 `who`（绝不二次解析规范账户）。
            // 解析为 None 时资格校验必然失败，此处的回退值不会被采用。
            let voter_account_id = <votingengine::Pallet<T>>::resolve_subject_voter(&subject, &who)
                .unwrap_or_else(|| who.clone());
            (
                InternalVoteTicket::Institution(InstitutionVoteTicket {
                    role_subject,
                    voter_account_id,
                }),
                Some(role_code),
                <votingengine::Pallet<T>>::is_subject_voter_in_snapshot(proposal_id, subject, &who),
                <votingengine::Pallet<T>>::institution_ticket_count(proposal_id, actor_cid_number)
                    .ok_or(votingengine::Error::<T>::MissingVoterSnapshot)?,
            )
        } else {
            ensure!(
                matches!(ticket_claim, InternalVoteTicketClaim::Personal),
                votingengine::Error::<T>::NoPermission
            );
            let personal_account_id = proposal
                .execution_account_id
                .ok_or(votingengine::Error::<T>::InvalidInstitution)?;
            let subject = ProposalSubject::PersonalAccount(personal_account_id);
            (
                InternalVoteTicket::Personal(who.clone()),
                None,
                <votingengine::Pallet<T>>::is_admin_in_snapshot(proposal_id, subject.clone(), &who),
                <votingengine::Pallet<T>>::snapshot_admins_len(proposal_id, subject)
                    .ok_or(votingengine::Error::<T>::MissingAdminSnapshot)?,
            )
        };
        ensure!(eligible, votingengine::Error::<T>::NoPermission);
        ensure!(
            !InternalVotesByTicket::<T>::contains_key(proposal_id, &ticket),
            votingengine::Error::<T>::AlreadyVoted
        );

        InternalVotesByTicket::<T>::insert(proposal_id, ticket, approve);
        let tally = InternalTallies::<T>::mutate(proposal_id, |tally| {
            if approve {
                tally.yes = tally.yes.saturating_add(1);
            } else {
                tally.no = tally.no.saturating_add(1);
            }
            *tally
        });

        Self::deposit_event(Event::<T>::InternalVoteCast {
            proposal_id,
            who,
            voter_role_code,
            approve,
        });

        let threshold = InternalThresholdSnapshot::<T>::get(proposal_id)
            .ok_or(Error::<T>::MissingThresholdSnapshot)?;
        if tally.yes >= threshold {
            <votingengine::Pallet<T>>::set_status_and_emit(proposal_id, STATUS_PASSED)?;
        } else {
            let casted = tally.yes.saturating_add(tally.no);
            let remaining = eligible_total.saturating_sub(casted);
            if tally.yes.saturating_add(remaining) < threshold {
                <votingengine::Pallet<T>>::set_status_and_emit(proposal_id, STATUS_REJECTED)?;
            }
        }

        Ok(())
    }

    pub fn do_finalize_internal_timeout(
        proposal: &Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        proposal_id: u64,
    ) -> DispatchResult {
        ensure!(
            proposal.stage == STAGE_INTERNAL,
            votingengine::Error::<T>::InvalidProposalStage
        );
        ensure!(
            proposal.status == votingengine::STATUS_VOTING,
            votingengine::Error::<T>::ProposalAlreadyFinalized
        );
        ensure!(
            <frame_system::Pallet<T>>::block_number() > proposal.end,
            votingengine::Error::<T>::VoteNotExpired
        );
        <votingengine::Pallet<T>>::set_status_and_emit(proposal_id, votingengine::STATUS_REJECTED)
    }
}
