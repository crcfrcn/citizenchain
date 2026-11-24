//! 机构内部投票提案创建、岗位快照和业务登记；个人提案归personal.rs。

use super::*;
use engingcore::InstitutionRoleProvider;

fn is_valid_institution_context(institution_code: InstitutionCode, cid_number: &[u8]) -> bool {
    !is_personal_code(&institution_code)
        && is_valid_governance_code(&institution_code)
        && institution_code_from_cid_number(core::str::from_utf8(cid_number).unwrap_or_default())
            == Some(institution_code)
}

fn active_institution_threshold<T: Config>(
    institution_code: InstitutionCode,
    cid_number: &CidNumber,
) -> Option<u32> {
    <T as engingcore::Config>::InternalVoteProvider::institution_threshold(
        institution_code,
        cid_number.as_slice(),
    )
}

impl<T: Config> Pallet<T> {
    pub(crate) fn institution_threshold(
        institution_code: InstitutionCode,
        cid_number: &[u8],
    ) -> Option<u32> {
        let cid_number = CidNumber::try_from(cid_number.to_vec()).ok()?;
        active_institution_threshold::<T>(institution_code, &cid_number)
    }

    /// 创建机构内部提案。机构身份只使用 CID，账户只在确有资产执行时写入。
    pub fn do_create_institution_proposal(
        who: T::AccountId,
        institution_code: InstitutionCode,
        actor_cid_number: sp_std::vec::Vec<u8>,
        execution_account_id: Option<T::AccountId>,
        subject_cid_numbers: sp_std::vec::Vec<sp_std::vec::Vec<u8>>,
        vote_plan: &VotePlanOf<T::AccountId>,
    ) -> Result<u128, DispatchError> {
        ensure!(
            is_valid_governance_code(&institution_code) && !is_personal_code(&institution_code),
            Error::<T>::InvalidInternalCode
        );
        let actor_cid_number = CidNumber::try_from(actor_cid_number)
            .map_err(|_| engingcore::Error::<T>::InvalidInstitution)?;
        ensure!(
            is_valid_institution_context(institution_code, actor_cid_number.as_slice()),
            engingcore::Error::<T>::InvalidInstitution
        );
        Self::ensure_institution_vote_plan(&actor_cid_number, &who, vote_plan)?;

        let now = <frame_system::Pallet<T>>::block_number();
        let end = now.saturating_add(Self::internal_stage_duration());
        let subject_cid_numbers =
            <engingcore::Pallet<T>>::bound_subject_cid_numbers(subject_cid_numbers)?;
        let proposal = Proposal {
            kind: PROPOSAL_KIND_INTERNAL,
            stage: STAGE_INTERNAL,
            status: engingcore::STATUS_VOTING,
            internal_code: Some(institution_code),
            actor_cid_number: Some(actor_cid_number.clone()),
            execution_account_id,
            subject_cid_numbers,
            start: now,
            end,
        };

        with_transaction(|| {
            let id = match Self::allocate_and_lock(&proposal, InternalProposalMutexKind::Regular) {
                Ok(id) => id,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };
            for subject in vote_plan.voter_subjects.iter() {
                let role_subject = match subject {
                    AuthorizationSubject::Institution(role_subject) => role_subject,
                    AuthorizationSubject::PersonalMultisig(_) => {
                        return TransactionOutcome::Rollback(Err(
                            engingcore::Error::<T>::InvalidVotePlan.into(),
                        ))
                    }
                };
                let voters = T::InstitutionRoleProvider::active_persons_for_role(
                    role_subject.cid_number.as_slice(),
                    role_subject.role_code.as_slice(),
                );
                if let Err(err) =
                    <engingcore::Pallet<T>>::snapshot_role_voters(id, subject.clone(), voters)
                {
                    return TransactionOutcome::Rollback(Err(err));
                }
            }
            let snapshot_size = match <engingcore::Pallet<T>>::institution_ticket_count(
                id,
                actor_cid_number.clone(),
            ) {
                Some(size) => size,
                None => {
                    return TransactionOutcome::Rollback(Err(
                        engingcore::Error::<T>::MissingVoterSnapshot.into(),
                    ))
                }
            };
            let threshold =
                match active_institution_threshold::<T>(institution_code, &actor_cid_number) {
                    Some(threshold) => threshold,
                    None => {
                        return TransactionOutcome::Rollback(Err(
                            Error::<T>::MissingDynamicThreshold.into(),
                        ))
                    }
                };
            let threshold_check = Self::ensure_dynamic_threshold(snapshot_size, threshold);
            if let Err(err) = threshold_check {
                return TransactionOutcome::Rollback(Err(err));
            }
            Self::finish_proposal_create(
                id,
                proposal,
                end,
                threshold,
                InternalProposalRole::General,
            )
        })
    }

    fn ensure_institution_vote_plan(
        actor_cid_number: &CidNumber,
        who: &T::AccountId,
        vote_plan: &VotePlanOf<T::AccountId>,
    ) -> DispatchResult {
        ensure!(
            vote_plan.voting_engine == VotingEngineKind::Internal,
            engingcore::Error::<T>::InvalidVotePlan
        );
        let proposer_role = match &vote_plan.proposer_subject {
            AuthorizationSubject::Institution(role_subject) => role_subject,
            AuthorizationSubject::PersonalMultisig(_) => {
                return Err(engingcore::Error::<T>::InvalidVotePlan.into())
            }
        };
        ensure!(
            proposer_role.cid_number == *actor_cid_number,
            engingcore::Error::<T>::InvalidVotePlan
        );
        ensure!(
            T::InstitutionRoleProvider::is_active_assignment(
                proposer_role.cid_number.as_slice(),
                who,
                proposer_role.role_code.as_slice(),
            ),
            engingcore::Error::<T>::NoPermission
        );
        for subject in vote_plan.voter_subjects.iter() {
            ensure!(
                matches!(
                    subject,
                    AuthorizationSubject::Institution(role_subject)
                        if role_subject.cid_number == *actor_cid_number
                ),
                engingcore::Error::<T>::InvalidVotePlan
            );
        }
        Ok(())
    }

    pub(crate) fn create_institution(
        who: T::AccountId,
        institution_code: InstitutionCode,
        actor_cid_number: sp_std::vec::Vec<u8>,
        execution_account_id: Option<T::AccountId>,
        subject_cid_numbers: sp_std::vec::Vec<sp_std::vec::Vec<u8>>,
        vote_plan: engingcore::types::VotePlanOf<T::AccountId>,
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError> {
        with_transaction(|| {
            if sp_io::hashing::blake2_256(&data) != vote_plan.business_object_hash {
                return TransactionOutcome::Rollback(Err(
                    engingcore::Error::<T>::InvalidVotePlan.into()
                ));
            }
            let proposal_id = match Self::do_create_institution_proposal(
                who.clone(),
                institution_code,
                actor_cid_number,
                execution_account_id,
                subject_cid_numbers,
                &vote_plan,
            ) {
                Ok(id) => id,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };
            if let Err(err) =
                engingcore::Pallet::<T>::bind_vote_plan(proposal_id, vote_plan.clone())
            {
                return TransactionOutcome::Rollback(Err(err));
            }
            match Self::register_institution_data(
                who,
                proposal_id,
                vote_plan.proposal_owner.as_slice(),
                data,
            ) {
                Ok(id) => TransactionOutcome::Commit(Ok(id)),
                Err(err) => TransactionOutcome::Rollback(Err(err)),
            }
        })
    }
    fn register_institution_data(
        who: T::AccountId,
        proposal_id: u128,
        module_tag: &[u8],
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError> {
        let now = frame_system::Pallet::<T>::block_number();
        engingcore::Pallet::<T>::register_proposal_data(proposal_id, module_tag, data, now)?;
        let vote_plan = engingcore::ProposalVotePlans::<T>::get(proposal_id)
            .ok_or(engingcore::Error::<T>::InvalidVotePlan)?;
        let proposer_role = match vote_plan.proposer_subject {
            AuthorizationSubject::Institution(role) => role,
            AuthorizationSubject::PersonalMultisig(_) => {
                return Err(engingcore::Error::<T>::InvalidVotePlan.into())
            }
        };
        let subject = AuthorizationSubject::Institution(proposer_role.clone());
        // 机构Propose权限与Vote权限分别校验；只有冻结选民才自动投首票。
        if engingcore::Pallet::<T>::is_subject_voter_in_snapshot(proposal_id, subject, &who) {
            Self::vote_institution(who, proposal_id, proposer_role.role_code, true)?;
        }
        Ok(proposal_id)
    }
}
