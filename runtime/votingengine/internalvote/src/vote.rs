//! 机构岗位票据资格校验；个人多签投票归personal.rs。
use super::*;

impl<T: Config> Pallet<T> {
    pub(crate) fn vote_institution(
        who: T::AccountId,
        proposal_id: u128,
        role_code: RoleCode,
        approve: bool,
    ) -> DispatchResult {
        let proposal = engingcore::Pallet::<T>::ensure_open_proposal(proposal_id)?;
        ensure!(
            proposal.kind == PROPOSAL_KIND_INTERNAL,
            engingcore::Error::<T>::InvalidProposalKind
        );
        ensure!(
            proposal.stage == STAGE_INTERNAL,
            engingcore::Error::<T>::InvalidProposalStage
        );
        let cid = proposal
            .actor_cid_number
            .ok_or(engingcore::Error::<T>::NoPermission)?;
        let role_subject = engingcore::types::RoleSubject {
            cid_number: cid.clone(),
            role_code: role_code.clone(),
        };
        let subject = AuthorizationSubject::Institution(role_subject.clone());
        let person_cid =
            engingcore::Pallet::<T>::resolve_snapshot_voter(proposal_id, &subject, &who)
                .ok_or(engingcore::Error::<T>::NoPermission)?;
        ensure!(
            engingcore::Pallet::<T>::is_subject_voter_in_snapshot(proposal_id, subject, &who),
            engingcore::Error::<T>::NoPermission
        );
        let eligible_total = engingcore::Pallet::<T>::institution_ticket_count(proposal_id, cid)
            .ok_or(engingcore::Error::<T>::MissingVoterSnapshot)?;
        Self::record_internal_vote(
            who,
            proposal_id,
            InternalVoteTicket::Institution(InstitutionVoteTicket {
                role_subject,
                person_cid,
            }),
            Some(role_code),
            approve,
            eligible_total,
        )
    }
}
