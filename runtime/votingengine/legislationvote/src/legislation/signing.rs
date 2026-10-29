//! 法律行政签署与省级、国家级三人共同签署程序。
//!
//! 本模块保存签署阶段可复用的纯规则，storage 状态推进仍由 `Pallet` 负责。

/// 超时入口只允许在截止区块之后执行。
pub(crate) fn is_expired<BlockNumber: PartialOrd>(now: BlockNumber, end: BlockNumber) -> bool {
    now > end
}

/// 三人会签必须三人全部批准。
pub(crate) fn override_approved(approvals: usize) -> bool {
    approvals >= 3
}
use crate::*;

impl<T: Config> Pallet<T> {
    /// 行政签署：只认建案时冻结的行政机构 LR 岗位任职。
    pub fn do_executive_sign(who: T::AccountId, proposal_id: u64, approve: bool) -> DispatchResult {
        let proposal =
            Proposals::<T>::get(proposal_id).ok_or(votingengine::Error::<T>::ProposalNotFound)?;
        ensure!(
            proposal.status == STATUS_VOTING,
            Error::<T>::NotInExpectedStage
        );
        ensure!(
            proposal.stage == STAGE_LEG_SIGN,
            Error::<T>::NotInExpectedStage
        );
        let meta = pallet::LegislationMetas::<T>::get(proposal_id)
            .ok_or(Error::<T>::ProposalMetaMissing)?;
        let executive = meta.executive.ok_or(Error::<T>::NotLegalRepresentative)?;
        ensure!(
            <votingengine::Pallet<T>>::is_subject_voter_in_snapshot(
                proposal_id,
                AuthorizationSubject::Institution(executive),
                &who,
            ),
            Error::<T>::NotLegalRepresentative
        );
        Self::deposit_event(pallet::Event::<T>::LegislationExecutiveSigned {
            proposal_id,
            who,
            approve,
        });
        if approve {
            Self::finalize_or_guard(proposal_id, meta.needs_guard)
        } else if !meta.override_signers.is_empty() {
            // 省行政区/国家:否决 → 退回三人会签救济。
            Self::advance_to_override(proposal_id)
        } else {
            // 市行政区:无救济,否决即否决。
            <votingengine::Pallet<T>>::set_status_and_emit(proposal_id, STATUS_REJECTED)
        }
    }

    /// 三人会签:院长/参议长/众议长各一票,任一否决=否决,集齐 3 个不同身份赞成=生效。
    pub fn do_override_sign(who: T::AccountId, proposal_id: u64, approve: bool) -> DispatchResult {
        let proposal =
            Proposals::<T>::get(proposal_id).ok_or(votingengine::Error::<T>::ProposalNotFound)?;
        ensure!(
            proposal.status == STATUS_VOTING,
            Error::<T>::NotInExpectedStage
        );
        ensure!(
            proposal.stage == STAGE_LEG_OVERRIDE,
            Error::<T>::NotInExpectedStage
        );
        let meta = pallet::LegislationMetas::<T>::get(proposal_id)
            .ok_or(Error::<T>::ProposalMetaMissing)?;
        ensure!(
            meta.override_signers.iter().any(|subject| {
                <votingengine::Pallet<T>>::is_subject_voter_in_snapshot(
                    proposal_id,
                    AuthorizationSubject::Institution(subject.clone()),
                    &who,
                )
            }),
            Error::<T>::NotOverrideSigner
        );
        let mut signs = pallet::LegOverrideSigns::<T>::get(proposal_id);
        ensure!(
            !signs.iter().any(|(s, _)| s == &who),
            Error::<T>::AlreadySigned
        );
        Self::deposit_event(pallet::Event::<T>::LegislationOverrideSigned {
            proposal_id,
            who: who.clone(),
            approve,
        });
        if !approve {
            // 任一否决即否决。
            return <votingengine::Pallet<T>>::set_status_and_emit(proposal_id, STATUS_REJECTED);
        }
        signs
            .try_push((who, true))
            .map_err(|_| Error::<T>::AlreadySigned)?;
        let approvals = signs.iter().filter(|(_, a)| *a).count();
        pallet::LegOverrideSigns::<T>::insert(proposal_id, signs);
        // 三人(院长+参议长+众议长)全批准 → 生效(修宪则转护宪终审)。
        if crate::legislation::signing::override_approved(approvals) {
            Self::finalize_or_guard(proposal_id, meta.needs_guard)
        } else {
            Ok(())
        }
    }

    /// 行政签署阶段超时:市行政区(无 legislature)= 视为通过;省行政区/国家 = 退回三人会签。
    pub fn do_finalize_sign_timeout(
        proposal: &Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        proposal_id: u64,
    ) -> DispatchResult {
        ensure!(
            proposal.stage == STAGE_LEG_SIGN,
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
        let meta = pallet::LegislationMetas::<T>::get(proposal_id)
            .ok_or(Error::<T>::ProposalMetaMissing)?;
        if !meta.override_signers.is_empty() {
            Self::advance_to_override(proposal_id)
        } else {
            Self::finalize_or_guard(proposal_id, meta.needs_guard)
        }
    }

    /// 三人会签阶段超时:法案否决。
    pub fn do_finalize_override_timeout(
        proposal: &Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        proposal_id: u64,
    ) -> DispatchResult {
        ensure!(
            proposal.stage == STAGE_LEG_OVERRIDE,
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
