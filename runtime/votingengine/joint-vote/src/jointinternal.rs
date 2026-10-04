//! 联合投票 — 内部投票阶段。
//!
//! 国家储委会 / 省储委会 / 省储行管理员按机构投票,任一机构反对或超时都进入联合公投阶段(jointreferendum)。
//!
//! 业务函数挂在 `super::Pallet<T>` 上,在 super(lib.rs)的 #[pallet::call]
//! `cast_admin` extrinsic 与 `JointVoteEngine` / `JointProposalFinalizer`
//! trait 实现中被调用。

use frame_support::{
    ensure,
    pallet_prelude::DispatchResult,
    storage::{with_transaction, TransactionOutcome},
};
use sp_runtime::traits::{SaturatedConversion, Saturating};
use sp_runtime::DispatchError;

use primitives::cid::china::china_cb::CHINA_CB;
use primitives::cid::china::china_ch::CHINA_CH;
use primitives::count_const::{
    JOINT_VOTE_TOTAL, NRC_JOINT_VOTE_WEIGHT, PRB_JOINT_VOTE_WEIGHT, PRC_JOINT_VOTE_WEIGHT,
    VOTING_DURATION_BLOCKS,
};

use votingengine::{
    pallet::{Proposals, ProposalsByExpiry},
    types::{
        fixed_governance_pass_threshold, AuthorizationSubject, CidNumber, InstitutionCode,
        InstitutionVoteTicket, ProposalSubjectCidNumbers, RoleCode, RoleSubject, VotePlanOf,
        VotingEngineKind, NRC, PRB, PRC,
    },
    InstitutionRoleProvider, InternalProposalMutexKind, PopulationScope, Proposal,
    PROPOSAL_KIND_JOINT, STAGE_JOINT, STATUS_PASSED,
};

use super::pallet::{
    Config, Error, Event, JointInstitutionTallies, JointTallies, JointVotesByInstitution,
    JointVotesByTicket, Pallet,
};
use super::{institution_info, is_joint_unanimous};

// 私有 helper:发起人机构解析 + (institution_code, weight) profile
pub(super) fn institution_profile(cid_number: &[u8]) -> Option<(InstitutionCode, u32)> {
    if CHINA_CB
        .first()
        .map(|n| n.cid_number.as_bytes() == cid_number)
        .unwrap_or(false)
    {
        return Some((NRC, NRC_JOINT_VOTE_WEIGHT));
    }
    if CHINA_CB
        .iter()
        .skip(1)
        .any(|n| n.cid_number.as_bytes() == cid_number)
    {
        return Some((PRC, PRC_JOINT_VOTE_WEIGHT));
    }
    if CHINA_CH
        .iter()
        .any(|n| n.cid_number.as_bytes() == cid_number)
    {
        return Some((PRB, PRB_JOINT_VOTE_WEIGHT));
    }
    None
}

fn ensure_vote_plan<T: Config>(
    actor_cid_number: &CidNumber,
    who: &T::AccountId,
    vote_plan: &VotePlanOf<T::AccountId>,
) -> Result<(), DispatchError> {
    ensure!(
        vote_plan.voting_engine == VotingEngineKind::Joint,
        votingengine::Error::<T>::InvalidVotePlan
    );
    ensure!(
        institution_profile(actor_cid_number.as_slice()).is_some(),
        votingengine::Error::<T>::InvalidInstitution
    );
    let proposer_role = match &vote_plan.proposer_subject {
        AuthorizationSubject::Institution(role_subject) => role_subject,
        AuthorizationSubject::PersonalMultisig(_) => {
            return Err(votingengine::Error::<T>::InvalidVotePlan.into())
        }
    };
    ensure!(
        proposer_role.cid_number == *actor_cid_number,
        votingengine::Error::<T>::InvalidVotePlan
    );
    ensure!(
        T::InstitutionRoleProvider::is_active_assignment(
            proposer_role.cid_number.as_slice(),
            who,
            proposer_role.role_code.as_slice(),
        ),
        votingengine::Error::<T>::NoPermission
    );

    let expected_cids = joint_subject_cid_numbers::<T>()?;
    for expected_cid in expected_cids.iter() {
        ensure!(
            vote_plan.voter_subjects.iter().any(|subject| matches!(
                subject,
                AuthorizationSubject::Institution(role_subject)
                    if role_subject.cid_number == *expected_cid
            )),
            votingengine::Error::<T>::InvalidVotePlan
        );
    }
    for subject in vote_plan.voter_subjects.iter() {
        let role_subject = match subject {
            AuthorizationSubject::Institution(role_subject) => role_subject,
            AuthorizationSubject::PersonalMultisig(_) => {
                return Err(votingengine::Error::<T>::InvalidVotePlan.into())
            }
        };
        ensure!(
            expected_cids
                .iter()
                .any(|expected_cid| expected_cid == &role_subject.cid_number),
            votingengine::Error::<T>::InvalidVotePlan
        );
    }
    Ok(())
}

fn joint_subject_cid_numbers<T: Config>() -> Result<ProposalSubjectCidNumbers, DispatchError> {
    let mut raw = sp_runtime::sp_std::vec::Vec::new();
    for entry in CHINA_CB.iter() {
        raw.push(entry.cid_number.as_bytes().to_vec());
    }
    for entry in CHINA_CH.iter() {
        raw.push(entry.cid_number.as_bytes().to_vec());
    }
    <votingengine::Pallet<T>>::bound_subject_cid_numbers(raw)
}
// 业务方法 — 挂在 super::Pallet<T> 上
impl<T: Config> Pallet<T> {
    pub(super) fn joint_stage_duration() -> frame_system::pallet_prelude::BlockNumberFor<T> {
        (VOTING_DURATION_BLOCKS as u64).saturated_into()
    }

    pub(super) fn referendum_stage_duration() -> frame_system::pallet_prelude::BlockNumberFor<T> {
        (VOTING_DURATION_BLOCKS as u64).saturated_into()
    }

    /// 创建联合投票提案。锁定全部参与机构（NRC + 43 PRC 委员、43 PRB 董事）
    /// 岗位有效选民快照，
    /// 同一事务内创建并绑定全国人口快照，后续阶段切换不再改写。
    pub fn do_create_joint_proposal(
        who: T::AccountId,
        actor_cid_number: CidNumber,
        vote_plan: VotePlanOf<T::AccountId>,
    ) -> Result<u64, DispatchError> {
        ensure_vote_plan::<T>(&actor_cid_number, &who, &vote_plan)?;
        let now = <frame_system::Pallet<T>>::block_number();
        let end = now.saturating_add(Self::joint_stage_duration());
        let subject_cid_numbers = joint_subject_cid_numbers::<T>()?;

        with_transaction(|| {
            let proposal = Proposal {
                kind: PROPOSAL_KIND_JOINT,
                stage: STAGE_JOINT,
                status: votingengine::STATUS_VOTING,
                internal_code: None,
                actor_cid_number: Some(actor_cid_number.clone()),
                execution_account_id: None,
                subject_cid_numbers,
                start: now,
                end,
            };
            let id = match <votingengine::Pallet<T>>::allocate_proposal_id() {
                Ok(id) => id,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };

            if let Err(err) =
                votingengine::limit::try_add_active_proposals::<T>(proposal.subject_keys(), id)
            {
                return TransactionOutcome::Rollback(Err(err));
            }

            // 联合提案关联全部固定治理机构,互斥锁按机构 CID 而非账户占用。
            for subject in proposal.subject_keys() {
                if let Err(err) = <votingengine::Pallet<T>>::acquire_internal_proposal_mutex(
                    id,
                    subject,
                    InternalProposalMutexKind::Regular,
                ) {
                    return TransactionOutcome::Rollback(Err(err));
                }
            }

            // 按完整岗位主体冻结当前有效任职；同一账户担任多个岗位时各形成一张票据。
            for subject in vote_plan.voter_subjects.iter() {
                let role_subject = match subject {
                    AuthorizationSubject::Institution(role_subject) => role_subject,
                    AuthorizationSubject::PersonalMultisig(_) => {
                        return TransactionOutcome::Rollback(Err(
                            votingengine::Error::<T>::InvalidVotePlan.into(),
                        ))
                    }
                };
                let voters = T::InstitutionRoleProvider::active_accounts_for_role(
                    role_subject.cid_number.as_slice(),
                    role_subject.role_code.as_slice(),
                );
                if let Err(err) =
                    <votingengine::Pallet<T>>::snapshot_role_voters(id, subject.clone(), voters)
                {
                    return TransactionOutcome::Rollback(Err(err));
                }
            }
            Proposals::<T>::insert(id, proposal);
            if let Err(err) = <votingengine::Pallet<T>>::bind_vote_plan(id, vote_plan) {
                return TransactionOutcome::Rollback(Err(err));
            }
            // 联合治理协议固定使用全国人口。投票引擎只读取 citizen-identity 已维护的
            // 人口数据并在本提案下生成快照；后续失败由外层事务整体回滚。
            match <votingengine::Pallet<T>>::create_population_snapshot(
                id,
                &PopulationScope::Country,
            ) {
                Ok(0) => {
                    return TransactionOutcome::Rollback(Err(
                        Error::<T>::CitizenEligibleTotalNotSet.into(),
                    ))
                }
                Ok(_) => {}
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            }
            if let Err(err) = <votingengine::Pallet<T>>::schedule_proposal_expiry(id, end) {
                return TransactionOutcome::Rollback(Err(err));
            }
            <votingengine::Pallet<T>>::emit_proposal_created(
                id,
                PROPOSAL_KIND_JOINT,
                STAGE_JOINT,
                end,
            );
            TransactionOutcome::Commit(Ok(id))
        })
    }

    /// 联合投票:管理员按机构投票。机构内达阈值后写入 `JointVotesByInstitution`,
    /// 全部机构票权累加判断是否全票通过(105 票)或推进至联合公投阶段。
    pub fn do_joint_vote(
        who: T::AccountId,
        proposal_id: u64,
        cid_number: CidNumber,
        voter_role_code: RoleCode,
        approve: bool,
    ) -> DispatchResult {
        let proposal = <votingengine::Pallet<T>>::ensure_open_proposal(proposal_id)?;

        ensure!(
            proposal.kind == PROPOSAL_KIND_JOINT,
            votingengine::Error::<T>::InvalidProposalKind
        );
        ensure!(
            proposal.stage == STAGE_JOINT,
            votingengine::Error::<T>::InvalidProposalStage
        );
        ensure!(
            !JointVotesByInstitution::<T>::contains_key(proposal_id, cid_number.clone()),
            votingengine::Error::<T>::AlreadyVoted
        );
        let (institution_code, _) = institution_profile(cid_number.as_slice())
            .ok_or(votingengine::Error::<T>::InvalidInstitution)?;
        let role_subject = RoleSubject {
            cid_number: cid_number.clone(),
            role_code: voter_role_code.clone(),
        };
        let subject = AuthorizationSubject::Institution(role_subject.clone());
        ensure!(
            <votingengine::Pallet<T>>::is_subject_voter_in_snapshot(
                proposal_id,
                subject.clone(),
                &who,
            ),
            votingengine::Error::<T>::NoPermission
        );
        // 票据按【规范账户】防重：换绑前后归并为同一张票，杜绝新旧钱包各投一票。
        let voter_account_id = <votingengine::Pallet<T>>::resolve_subject_voter(&subject, &who)
            .unwrap_or_else(|| who.clone());
        let ticket = InstitutionVoteTicket {
            role_subject,
            voter_account_id,
        };
        ensure!(
            !JointVotesByTicket::<T>::contains_key(proposal_id, &ticket),
            votingengine::Error::<T>::AlreadyVoted
        );

        JointVotesByTicket::<T>::insert(proposal_id, ticket, approve);
        let tally =
            JointInstitutionTallies::<T>::mutate(proposal_id, cid_number.clone(), |tally| {
                if approve {
                    tally.yes = tally.yes.saturating_add(1);
                } else {
                    tally.no = tally.no.saturating_add(1);
                }
                *tally
            });

        Self::deposit_event(Event::<T>::JointInstitutionTicketVoteCast {
            proposal_id,
            cid_number: cid_number.clone(),
            who,
            voter_role_code,
            approve,
        });

        let threshold = fixed_governance_pass_threshold(&institution_code)
            .ok_or(votingengine::Error::<T>::InvalidInstitution)?;
        let voters_len =
            <votingengine::Pallet<T>>::institution_ticket_count(proposal_id, cid_number.clone())
                .ok_or(votingengine::Error::<T>::InvalidInstitution)?;

        if tally.yes >= threshold {
            return Self::finalize_joint_institution_vote(proposal_id, cid_number, true);
        }
        let casted_votes = tally.yes.saturating_add(tally.no);
        let remaining_voters = voters_len.saturating_sub(casted_votes);
        if tally.yes.saturating_add(remaining_voters) < threshold {
            return Self::finalize_joint_institution_vote(proposal_id, cid_number, false);
        }

        Ok(())
    }

    fn finalize_joint_institution_vote(
        proposal_id: u64,
        cid_number: CidNumber,
        approved: bool,
    ) -> DispatchResult {
        ensure!(
            !JointVotesByInstitution::<T>::contains_key(proposal_id, cid_number.clone()),
            votingengine::Error::<T>::AlreadyVoted
        );
        let weight = institution_info(cid_number.as_slice())
            .ok_or(votingengine::Error::<T>::InvalidInstitution)?;

        JointVotesByInstitution::<T>::insert(proposal_id, cid_number.clone(), approved);

        let tally = JointTallies::<T>::mutate(proposal_id, |tally| {
            if approved {
                tally.yes = tally.yes.saturating_add(weight);
            } else {
                tally.no = tally.no.saturating_add(weight);
            }
            *tally
        });

        Self::deposit_event(Event::<T>::JointInstitutionVoteFinalized {
            proposal_id,
            cid_number,
            approved,
        });

        if approved {
            if is_joint_unanimous(tally.yes) {
                <votingengine::Pallet<T>>::set_status_and_emit(proposal_id, STATUS_PASSED)?;
                return Ok(());
            }
            if tally.yes.saturating_add(tally.no) >= JOINT_VOTE_TOTAL {
                return Self::advance_joint_to_referendum(proposal_id);
            }
            return Ok(());
        }
        Self::advance_joint_to_referendum(proposal_id)
    }

    /// 联合内部投票阶段超时结算:全票通过 → PASSED,否则进入联合公投阶段。
    pub fn do_finalize_joint_timeout(
        proposal: &Proposal<frame_system::pallet_prelude::BlockNumberFor<T>, T::AccountId>,
        proposal_id: u64,
    ) -> DispatchResult {
        ensure!(
            proposal.stage == STAGE_JOINT,
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

        let tally = JointTallies::<T>::get(proposal_id);
        if is_joint_unanimous(tally.yes) {
            return <votingengine::Pallet<T>>::set_status_and_emit(proposal_id, STATUS_PASSED);
        }
        Self::advance_joint_to_referendum(proposal_id)
    }

    fn advance_joint_to_referendum(proposal_id: u64) -> DispatchResult {
        let now = <frame_system::Pallet<T>>::block_number();
        let referendum_end = now.saturating_add(Self::referendum_stage_duration());
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
                    let proposal = maybe
                        .as_mut()
                        .ok_or(votingengine::Error::<T>::ProposalNotFound)?;
                    let old_end = proposal.end;
                    proposal.stage = votingengine::STAGE_REFERENDUM;
                    proposal.start = now;
                    proposal.end = referendum_end;
                    Ok(old_end)
                },
            ) {
                Ok(v) => v,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };

            let old_expiry = old_end.saturating_add(sp_runtime::traits::One::one());
            ProposalsByExpiry::<T>::mutate(old_expiry, |ids| {
                ids.retain(|&id| id != proposal_id);
            });

            if let Err(err) =
                <votingengine::Pallet<T>>::schedule_proposal_expiry(proposal_id, referendum_end)
            {
                return TransactionOutcome::Rollback(Err(err));
            }
            <votingengine::Pallet<T>>::release_internal_proposal_mutexes(proposal_id);

            <votingengine::Pallet<T>>::emit_proposal_advanced_to_referendum(
                proposal_id,
                referendum_end,
                eligible_total,
            );
            TransactionOutcome::Commit(Ok(()))
        })
    }
}
