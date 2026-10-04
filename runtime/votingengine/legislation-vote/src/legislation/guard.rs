//! 修宪提案的护宪大法官终审表决。
//!
//! 这里只承载护宪表决程序，不保存宪法正文或法律版本。

/// 护宪终审达到法定多数。
pub(crate) fn decided(approve_or_reject: usize) -> bool {
    approve_or_reject >= primitives::constitution::CONSTITUTION_GUARD_APPROVAL_THRESHOLD as usize
}
use crate::*;

impl<T: Config> Pallet<T> {
    /// 成功终态统一出口:修宪(needs_guard)→ 进护宪大法官终审;否则直接 PASSED。
    pub(crate) fn finalize_or_guard(proposal_id: u64, needs_guard: bool) -> DispatchResult {
        if needs_guard {
            Self::advance_to_guard(proposal_id)
        } else {
            <votingengine::Pallet<T>>::set_status_and_emit(
                proposal_id,
                crate::result::approved_status(),
            )
        }
    }

    /// 修宪现有流程通过 → 进入护宪大法官终审阶段(宪法第21条)。
    pub(crate) fn advance_to_guard(proposal_id: u64) -> DispatchResult {
        pallet::LegGuardSigns::<T>::remove(proposal_id);
        Self::transition_stage(proposal_id, STAGE_LEG_CONSTITUTION_GUARD)?;
        Self::deposit_event(pallet::Event::<T>::LegislationAdvancedToGuard { proposal_id });
        Ok(())
    }

    /// 护宪大法官终审表决(仅修宪):7 人一人一票,4 名及以上赞成→生效;4 名及以上反对→否决。
    pub fn do_guard_vote(who: T::AccountId, proposal_id: u64, approve: bool) -> DispatchResult {
        let proposal =
            Proposals::<T>::get(proposal_id).ok_or(votingengine::Error::<T>::ProposalNotFound)?;
        ensure!(
            proposal.status == STATUS_VOTING,
            Error::<T>::NotInExpectedStage
        );
        ensure!(
            proposal.stage == STAGE_LEG_CONSTITUTION_GUARD,
            Error::<T>::NotInExpectedStage
        );
        let meta = pallet::LegislationMetas::<T>::get(proposal_id)
            .ok_or(Error::<T>::ProposalMetaMissing)?;
        let guard_subject = meta.guard.ok_or(Error::<T>::NotConstitutionGuard)?;
        let authorization_subject = AuthorizationSubject::Institution(guard_subject);
        let members_len = <votingengine::Pallet<T>>::subject_voters_len(
            proposal_id,
            authorization_subject.clone(),
        )
        .ok_or(Error::<T>::InvalidGuardMembersLen)?;
        ensure!(
            members_len == CONSTITUTION_GUARD_MEMBERS,
            Error::<T>::InvalidGuardMembersLen
        );
        ensure!(
            <votingengine::Pallet<T>>::is_subject_voter_in_snapshot(
                proposal_id,
                authorization_subject,
                &who,
            ),
            Error::<T>::NotConstitutionGuard
        );
        let mut signs = pallet::LegGuardSigns::<T>::get(proposal_id);
        ensure!(
            !signs.iter().any(|(s, _)| s == &who),
            Error::<T>::AlreadySigned
        );
        Self::deposit_event(pallet::Event::<T>::LegislationGuardVoted {
            proposal_id,
            who: who.clone(),
            approve,
        });
        signs
            .try_push((who, approve))
            .map_err(|_| Error::<T>::AlreadySigned)?;
        let yes = signs.iter().filter(|(_, a)| *a).count();
        let no = signs.iter().filter(|(_, a)| !*a).count();
        pallet::LegGuardSigns::<T>::insert(proposal_id, signs);
        if crate::legislation::guard::decided(yes) {
            // 7 人多数通过:4 名及以上赞成 → 生效。
            <votingengine::Pallet<T>>::set_status_and_emit(proposal_id, STATUS_PASSED)
        } else if crate::legislation::guard::decided(no) {
            // 4 名及以上反对 → 已不可能达到 4 名赞成,否决。
            <votingengine::Pallet<T>>::set_status_and_emit(proposal_id, STATUS_REJECTED)
        } else {
            Ok(())
        }
    }

    /// 护宪大法官终审超时:未获4名及以上赞成 → 否决。
    pub fn do_finalize_guard_timeout(
        proposal: &Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        proposal_id: u64,
    ) -> DispatchResult {
        ensure!(
            proposal.stage == STAGE_LEG_CONSTITUTION_GUARD,
            votingengine::Error::<T>::InvalidProposalStage
        );
        ensure!(
            proposal.status == STATUS_VOTING,
            votingengine::Error::<T>::ProposalAlreadyFinalized
        );
        ensure!(
            crate::legislation::signing::is_expired(
                <frame_system::Pallet<T>>::block_number(),
                proposal.end,
            ),
            votingengine::Error::<T>::VoteNotExpired
        );
        <votingengine::Pallet<T>>::set_status_and_emit(proposal_id, STATUS_REJECTED)
    }
}
