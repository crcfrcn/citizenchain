//! 个人多签的建案、钱包投票、阈值和阈值生命周期；不解析机构岗位或人员CID。
use super::*;

fn is_personal_admin<T: Config>(personal_account_id: T::AccountId, who: &T::AccountId) -> bool {
    <T as engingcore::Config>::InternalVoteProvider::is_personal_admin(personal_account_id, who)
}

impl<T: Config> Pallet<T> {
    /// 普通个人提案采用1..=成员数；创建和注销另行强制全员。
    pub(crate) fn ensure_personal_threshold(admins_len: u32, threshold: u32) -> DispatchResult {
        ensure!(
            admins_len >= 2
                && admins_len <= primitives::multisig::MAX_PERSONAL_ADMINS
                && threshold > 0
                && threshold <= admins_len,
            Error::<T>::InvalidDynamicThreshold
        );
        Ok(())
    }

    pub(crate) fn vote_personal(
        who: T::AccountId,
        proposal_id: u128,
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
        ensure!(
            proposal.actor_cid_number.is_none()
                && proposal.internal_code == Some(engingcore::types::PMUL),
            engingcore::Error::<T>::NoPermission
        );
        let account = proposal
            .execution_account_id
            .ok_or(engingcore::Error::<T>::InvalidInstitution)?;
        let subject = ProposalSubject::PersonalAccount(account);
        ensure!(
            engingcore::Pallet::<T>::is_admin_in_snapshot(proposal_id, subject.clone(), &who),
            engingcore::Error::<T>::NoPermission
        );
        let total = engingcore::Pallet::<T>::snapshot_admins_len(proposal_id, subject)
            .ok_or(engingcore::Error::<T>::MissingAdminSnapshot)?;
        Self::record_internal_vote(
            who.clone(),
            proposal_id,
            InternalVoteTicket::Personal(who),
            None,
            approve,
            total,
        )
    }

    fn register_personal_data(
        who: T::AccountId,
        proposal_id: u128,
        module_tag: &[u8],
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError> {
        let now = frame_system::Pallet::<T>::block_number();
        engingcore::Pallet::<T>::register_proposal_data(proposal_id, module_tag, data, now)?;
        // 先登记完整owner/data再投首票，1/N也只能排队执行已绑定的业务。
        Self::vote_personal(who, proposal_id, true)?;
        Ok(proposal_id)
    }

    pub(crate) fn clear_personal_pending(proposal_id: u128) {
        PendingPersonalThresholds::<T>::remove(proposal_id);
        PendingPersonalAdminChangeThresholds::<T>::remove(proposal_id);
    }
    pub fn do_create_personal_proposal(
        who: T::AccountId,
        personal_account_id: T::AccountId,
    ) -> Result<u128, DispatchError> {
        ensure!(
            is_personal_admin::<T>(personal_account_id.clone(), &who),
            engingcore::Error::<T>::NoPermission
        );
        Self::do_create_active_personal_proposal(
            who,
            personal_account_id,
            InternalProposalMutexKind::Regular,
            InternalProposalRole::General,
            false,
        )
    }

    pub fn do_create_personal_lifecycle_proposal(
        who: T::AccountId,
        personal_account_id: T::AccountId,
    ) -> Result<u128, DispatchError> {
        ensure!(
            is_personal_admin::<T>(personal_account_id.clone(), &who),
            engingcore::Error::<T>::NoPermission
        );
        Self::do_create_active_personal_proposal(
            who,
            personal_account_id,
            InternalProposalMutexKind::Regular,
            InternalProposalRole::PersonalClose,
            true,
        )
    }

    pub fn do_create_personal_admin_change_proposal(
        who: T::AccountId,
        personal_account_id: T::AccountId,
        new_admins_len: u32,
        new_threshold: u32,
    ) -> Result<u128, DispatchError> {
        Self::ensure_personal_threshold(new_admins_len, new_threshold)?;
        ensure!(
            is_personal_admin::<T>(personal_account_id.clone(), &who),
            engingcore::Error::<T>::NoPermission
        );
        let proposal_id = Self::do_create_active_personal_proposal(
            who,
            personal_account_id.clone(),
            InternalProposalMutexKind::AdminSetMutationExclusive,
            InternalProposalRole::PersonalAdminChange,
            false,
        )?;
        PendingPersonalAdminChangeThresholds::<T>::insert(
            proposal_id,
            PendingPersonalAdminChangeThreshold {
                personal_account_id,
                new_admins_len,
                new_threshold,
            },
        );
        Ok(proposal_id)
    }

    pub fn do_create_personal_account_create_proposal(
        who: T::AccountId,
        personal_account_id: T::AccountId,
        admins: sp_std::vec::Vec<T::AccountId>,
        dynamic_threshold: u32,
    ) -> Result<u128, DispatchError> {
        ensure!(
            !admins.is_empty(),
            engingcore::Error::<T>::MissingAdminSnapshot
        );
        ensure!(
            admins.iter().any(|admin| admin == &who),
            engingcore::Error::<T>::NoPermission
        );
        for i in 0..admins.len() {
            for j in i.saturating_add(1)..admins.len() {
                ensure!(
                    admins[i] != admins[j],
                    engingcore::Error::<T>::InvalidInstitution
                );
            }
        }
        let admins_len = admins.len() as u32;
        Self::ensure_personal_threshold(admins_len, dynamic_threshold)?;
        let bounded_admins: BoundedVec<
            T::AccountId,
            frame_support::traits::ConstU32<{ primitives::multisig::MAX_PERSONAL_ADMINS }>,
        > = admins
            .try_into()
            .map_err(|_| engingcore::Error::<T>::InvalidInstitution)?;

        let now = <frame_system::Pallet<T>>::block_number();
        let end = now.saturating_add(Self::internal_stage_duration());
        let proposal = Proposal {
            kind: PROPOSAL_KIND_INTERNAL,
            stage: STAGE_INTERNAL,
            status: engingcore::STATUS_VOTING,
            internal_code: Some(engingcore::types::PMUL),
            actor_cid_number: None,
            execution_account_id: Some(personal_account_id.clone()),
            subject_cid_numbers: ProposalSubjectCidNumbers::default(),
            start: now,
            end,
        };

        with_transaction(|| {
            let id = match Self::allocate_and_lock(&proposal, InternalProposalMutexKind::Regular) {
                Ok(id) => id,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };
            AdminSnapshot::<T>::insert(
                id,
                ProposalSubject::PersonalAccount(personal_account_id),
                bounded_admins,
            );
            PendingPersonalThresholds::<T>::insert(id, dynamic_threshold);
            Self::finish_proposal_create(
                id,
                proposal,
                end,
                admins_len,
                InternalProposalRole::PersonalCreate,
            )
        })
    }

    fn do_create_active_personal_proposal(
        who: T::AccountId,
        personal_account_id: T::AccountId,
        mutex_kind: InternalProposalMutexKind,
        role: InternalProposalRole,
        force_all_admin_threshold: bool,
    ) -> Result<u128, DispatchError> {
        let now = <frame_system::Pallet<T>>::block_number();
        let end = now.saturating_add(Self::internal_stage_duration());
        let proposal = Proposal {
            kind: PROPOSAL_KIND_INTERNAL,
            stage: STAGE_INTERNAL,
            status: engingcore::STATUS_VOTING,
            internal_code: Some(engingcore::types::PMUL),
            actor_cid_number: None,
            execution_account_id: Some(personal_account_id.clone()),
            subject_cid_numbers: ProposalSubjectCidNumbers::default(),
            start: now,
            end,
        };

        with_transaction(|| {
            let id = match Self::allocate_and_lock(&proposal, mutex_kind) {
                Ok(id) => id,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };
            if let Err(err) = <engingcore::Pallet<T>>::snapshot_personal_admins(
                id,
                personal_account_id.clone(),
                false,
            ) {
                return TransactionOutcome::Rollback(Err(err));
            }
            let subject = ProposalSubject::PersonalAccount(personal_account_id.clone());
            if !<engingcore::Pallet<T>>::is_admin_in_snapshot(id, subject.clone(), &who) {
                return TransactionOutcome::Rollback(Err(
                    engingcore::Error::<T>::NoPermission.into()
                ));
            }
            let snapshot_size = match Self::snapshot_admins_len_or_missing(id, subject) {
                Ok(size) => size,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };
            let threshold = if force_all_admin_threshold {
                snapshot_size
            } else {
                match ActivePersonalThresholds::<T>::get(personal_account_id) {
                    Some(threshold) => threshold,
                    None => {
                        return TransactionOutcome::Rollback(Err(
                            Error::<T>::MissingDynamicThreshold.into(),
                        ))
                    }
                }
            };
            let threshold_check = if force_all_admin_threshold {
                Self::ensure_all_admin_threshold(snapshot_size, threshold)
            } else {
                Self::ensure_personal_threshold(snapshot_size, threshold)
            };
            if let Err(err) = threshold_check {
                return TransactionOutcome::Rollback(Err(err));
            }
            Self::finish_proposal_create(id, proposal, end, threshold, role)
        })
    }

    pub(crate) fn ensure_all_admin_threshold(admins_len: u32, threshold: u32) -> DispatchResult {
        // 账户链上注册与注销会改变账户生命周期，必须由该账户快照内全体管理员通过。
        ensure!(
            admins_len > 0 && threshold == admins_len,
            Error::<T>::InvalidThresholdSnapshot
        );
        Ok(())
    }

    pub(crate) fn snapshot_admins_len_or_missing(
        proposal_id: u128,
        subject: ProposalSubject<T::AccountId>,
    ) -> Result<u32, DispatchError> {
        <engingcore::Pallet<T>>::snapshot_admins_len(proposal_id, subject)
            .ok_or(engingcore::Error::<T>::MissingAdminSnapshot.into())
    }

    pub(crate) fn proposal_personal_account(
        proposal_id: u128,
    ) -> Result<T::AccountId, DispatchError> {
        let proposal =
            Proposals::<T>::get(proposal_id).ok_or(engingcore::Error::<T>::ProposalNotFound)?;
        ensure!(
            proposal.internal_code == Some(engingcore::types::PMUL)
                && proposal.actor_cid_number.is_none(),
            engingcore::Error::<T>::InvalidInstitution
        );
        proposal
            .execution_account_id
            .ok_or(engingcore::Error::<T>::InvalidInstitution.into())
    }

    pub(crate) fn apply_executed_threshold_side_effect(proposal_id: u128) -> DispatchResult {
        match InternalProposalRoles::<T>::get(proposal_id) {
            Some(InternalProposalRole::PersonalCreate) => {
                let personal_account_id = Self::proposal_personal_account(proposal_id)?;
                let threshold = PendingPersonalThresholds::<T>::take(proposal_id)
                    .ok_or(Error::<T>::MissingDynamicThreshold)?;
                ActivePersonalThresholds::<T>::insert(personal_account_id, threshold);
            }
            Some(InternalProposalRole::PersonalClose) => {
                let personal_account_id = Self::proposal_personal_account(proposal_id)?;
                ActivePersonalThresholds::<T>::remove(personal_account_id);
            }
            Some(InternalProposalRole::PersonalAdminChange) => {
                if let Some(pending) = PendingPersonalAdminChangeThresholds::<T>::take(proposal_id)
                {
                    Self::ensure_personal_threshold(pending.new_admins_len, pending.new_threshold)?;
                    ActivePersonalThresholds::<T>::insert(
                        pending.personal_account_id,
                        pending.new_threshold,
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn apply_terminal_threshold_cleanup(
        proposal_id: u128,
        status: u8,
    ) -> DispatchResult {
        match (InternalProposalRoles::<T>::get(proposal_id), status) {
            (
                Some(InternalProposalRole::PersonalCreate),
                STATUS_REJECTED | STATUS_EXECUTION_FAILED,
            ) => {
                PendingPersonalThresholds::<T>::remove(proposal_id);
            }
            (
                Some(InternalProposalRole::PersonalAdminChange),
                STATUS_REJECTED | STATUS_EXECUTION_FAILED,
            ) => {
                PendingPersonalAdminChangeThresholds::<T>::remove(proposal_id);
            }
            (Some(_), STATUS_EXECUTED) | (None, _) => {}
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn create_personal(
        who: T::AccountId,
        personal_account_id: T::AccountId,
        module_tag: &[u8],
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError> {
        with_transaction(|| {
            let proposal_id =
                match Self::do_create_personal_proposal(who.clone(), personal_account_id) {
                    Ok(id) => id,
                    Err(err) => return TransactionOutcome::Rollback(Err(err)),
                };
            match Self::register_personal_data(who, proposal_id, module_tag, data) {
                Ok(id) => TransactionOutcome::Commit(Ok(id)),
                Err(err) => TransactionOutcome::Rollback(Err(err)),
            }
        })
    }

    pub(crate) fn create_personal_close(
        who: T::AccountId,
        personal_account_id: T::AccountId,
        module_tag: &[u8],
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError> {
        with_transaction(|| {
            let proposal_id =
                match Self::do_create_personal_lifecycle_proposal(who.clone(), personal_account_id)
                {
                    Ok(id) => id,
                    Err(err) => return TransactionOutcome::Rollback(Err(err)),
                };
            match Self::register_personal_data(who, proposal_id, module_tag, data) {
                Ok(id) => TransactionOutcome::Commit(Ok(id)),
                Err(err) => TransactionOutcome::Rollback(Err(err)),
            }
        })
    }

    pub(crate) fn create_personal_account(
        who: T::AccountId,
        personal_account_id: T::AccountId,
        admins: sp_std::vec::Vec<T::AccountId>,
        dynamic_threshold: u32,
        module_tag: &[u8],
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError> {
        with_transaction(|| {
            let proposal_id = match Self::do_create_personal_account_create_proposal(
                who.clone(),
                personal_account_id,
                admins,
                dynamic_threshold,
            ) {
                Ok(id) => id,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };
            match Self::register_personal_data(who, proposal_id, module_tag, data) {
                Ok(id) => TransactionOutcome::Commit(Ok(id)),
                Err(err) => TransactionOutcome::Rollback(Err(err)),
            }
        })
    }

    pub(crate) fn change_personal_admins(
        who: T::AccountId,
        personal_account_id: T::AccountId,
        new_admins_len: u32,
        new_threshold: u32,
        module_tag: &[u8],
        data: sp_std::vec::Vec<u8>,
    ) -> Result<u128, DispatchError> {
        with_transaction(|| {
            let proposal_id = match Self::do_create_personal_admin_change_proposal(
                who.clone(),
                personal_account_id,
                new_admins_len,
                new_threshold,
            ) {
                Ok(id) => id,
                Err(err) => return TransactionOutcome::Rollback(Err(err)),
            };
            match Self::register_personal_data(who, proposal_id, module_tag, data) {
                Ok(id) => TransactionOutcome::Commit(Ok(id)),
                Err(err) => TransactionOutcome::Rollback(Err(err)),
            }
        })
    }

    pub(crate) fn personal_threshold(personal_account_id: T::AccountId) -> Option<u32> {
        ActivePersonalThresholds::<T>::get(personal_account_id)
    }

    pub(crate) fn configured_threshold(
        proposal_id: u128,
        personal_account_id: T::AccountId,
    ) -> Option<u32> {
        PendingPersonalThresholds::<T>::get(proposal_id)
            .or_else(|| ActivePersonalThresholds::<T>::get(personal_account_id))
    }
}
