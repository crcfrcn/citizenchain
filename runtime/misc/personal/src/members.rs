//! 保存和查询个人多签成员，按原内部投票结果执行成员变更。

use crate::*;
use engingcore::ProposalSubject;

impl<T: Config> Pallet<T> {
    pub(crate) fn validate_admin_set_for_change(
        admins: &AdminsOf<T>,
        new_threshold: u32,
    ) -> DispatchResult {
        let admins_len = admins.len() as u32;
        ensure!(admins_len >= 2, Error::<T>::InvalidAdminsLen);
        ensure!(
            admins_len <= <T as Config>::MaxPersonalAccountAdmins::get(),
            Error::<T>::InvalidAdminsLen
        );
        T::InternalVoteEngine::validate_personal_threshold(admins_len, new_threshold)
            .map_err(|_| Error::<T>::InvalidThreshold)?;
        Self::ensure_unique_admins(admins)?;
        Ok(())
    }

    pub(crate) fn same_admin_set(
        left: &[Admin<T::AccountId>],
        right: &[Admin<T::AccountId>],
    ) -> bool {
        if left.len() != right.len() {
            return false;
        }
        let mut left = left.to_vec();
        let mut right = right.to_vec();
        left.sort_by(|a, b| a.account_id.cmp(&b.account_id));
        right.sort_by(|a, b| a.account_id.cmp(&b.account_id));
        left == right
    }

    pub(crate) fn do_create_pending_admin_account(
        account: T::AccountId,
        kind: AdminAccountKind,
        admins: Vec<Admin<T::AccountId>>,
        creator_account_id: T::AccountId,
    ) -> DispatchResult {
        ensure!(
            kind == AdminAccountKind::PersonalMultisig,
            Error::<T>::NotPersonalAccount
        );
        ensure!(
            !AdminAccounts::<T>::contains_key(account.clone()),
            Error::<T>::PersonalAlreadyExists
        );
        let bounded: AdminsOf<T> = admins
            .into_iter()
            .map(Admin::normalize_names)
            .collect::<Vec<_>>()
            .try_into()
            .map_err(|_| Error::<T>::InvalidAdminsLen)?;
        Self::ensure_unique_admins(&bounded)?;
        let now = frame_system::Pallet::<T>::block_number();
        let admins_len = bounded.len() as u32;
        AdminAccounts::<T>::insert(
            account.clone(),
            AdminAccount {
                institution_code: PMUL,
                kind,
                admins: bounded,
                creator_account_id: creator_account_id.clone(),
                created_at: now,
                updated_at: now,
                status: AdminAccountStatus::Pending,
            },
        );
        Self::deposit_event(Event::<T>::AdminAccountPendingCreated {
            account,
            creator_account_id,
            admins_len,
        });
        Ok(())
    }

    pub(crate) fn do_activate_admin_account(account: T::AccountId) -> DispatchResult {
        AdminAccounts::<T>::try_mutate(account.clone(), |maybe| -> DispatchResult {
            let account_id = maybe.as_mut().ok_or(Error::<T>::PersonalNotFound)?;
            ensure!(
                account_id.status == AdminAccountStatus::Pending,
                Error::<T>::PersonalNotActive
            );
            account_id.status = AdminAccountStatus::Active;
            account_id.updated_at = frame_system::Pallet::<T>::block_number();
            Ok(())
        })?;
        Self::deposit_event(Event::<T>::AdminAccountActivated { account });
        Ok(())
    }

    pub(crate) fn do_remove_pending_admin_account(account: T::AccountId) -> DispatchResult {
        let account_id =
            AdminAccounts::<T>::get(account.clone()).ok_or(Error::<T>::PersonalNotFound)?;
        ensure!(
            account_id.status == AdminAccountStatus::Pending,
            Error::<T>::PersonalNotActive
        );
        AdminAccounts::<T>::remove(account.clone());
        Self::deposit_event(Event::<T>::AdminAccountPendingRemoved { account });
        Ok(())
    }

    pub(crate) fn do_close_admin_account(account: T::AccountId) -> DispatchResult {
        let account_id =
            AdminAccounts::<T>::get(account.clone()).ok_or(Error::<T>::PersonalNotFound)?;
        ensure!(
            account_id.status == AdminAccountStatus::Active,
            Error::<T>::PersonalNotActive
        );
        AdminAccounts::<T>::remove(account.clone());
        Self::deposit_event(Event::<T>::AdminAccountClosed { account });
        Ok(())
    }

    pub(crate) fn admin_account_with_status(
        institution_code: InstitutionCode,
        account: T::AccountId,
        status: AdminAccountStatus,
    ) -> Option<AdminAccountOf<T>> {
        if institution_code != PMUL {
            return None;
        }
        let account_id = AdminAccounts::<T>::get(account)?;
        if account_id.status == status
            && account_id.kind == AdminAccountKind::PersonalMultisig
            && account_id.institution_code == PMUL
        {
            Some(account_id)
        } else {
            None
        }
    }

    pub fn active_admin_account_exists(
        institution_code: InstitutionCode,
        account: T::AccountId,
    ) -> bool {
        Self::admin_account_with_status(institution_code, account, AdminAccountStatus::Active)
            .is_some()
    }

    pub fn is_active_account_admin(
        institution_code: InstitutionCode,
        account: T::AccountId,
        who: &T::AccountId,
    ) -> bool {
        let Some(account_id) =
            Self::admin_account_with_status(institution_code, account, AdminAccountStatus::Active)
        else {
            return false;
        };
        account_id
            .admins
            .iter()
            .any(|admin| &admin.account_id == who)
    }

    pub fn active_account_admins(
        institution_code: InstitutionCode,
        account: T::AccountId,
    ) -> Option<Vec<T::AccountId>> {
        Some(
            Self::admin_account_with_status(institution_code, account, AdminAccountStatus::Active)?
                .admins
                .into_inner()
                .into_iter()
                .map(|admin| admin.account_id)
                .collect(),
        )
    }

    pub fn active_account_admin_records(
        institution_code: InstitutionCode,
        account: T::AccountId,
    ) -> Option<Vec<Admin<T::AccountId>>> {
        Some(
            Self::admin_account_with_status(institution_code, account, AdminAccountStatus::Active)?
                .admins
                .into_inner(),
        )
    }

    pub fn active_account_admins_len(
        institution_code: InstitutionCode,
        account: T::AccountId,
    ) -> Option<u32> {
        Some(
            Self::admin_account_with_status(institution_code, account, AdminAccountStatus::Active)?
                .admins
                .len() as u32,
        )
    }

    pub fn pending_account_exists_for_snapshot(
        institution_code: InstitutionCode,
        account: T::AccountId,
    ) -> bool {
        Self::admin_account_with_status(institution_code, account, AdminAccountStatus::Pending)
            .is_some()
    }

    pub fn is_pending_account_admin_for_snapshot(
        institution_code: InstitutionCode,
        account: T::AccountId,
        who: &T::AccountId,
    ) -> bool {
        let Some(account_id) =
            Self::admin_account_with_status(institution_code, account, AdminAccountStatus::Pending)
        else {
            return false;
        };
        account_id
            .admins
            .iter()
            .any(|admin| &admin.account_id == who)
    }

    pub fn pending_account_admins_for_snapshot(
        institution_code: InstitutionCode,
        account: T::AccountId,
    ) -> Option<Vec<T::AccountId>> {
        Some(
            Self::admin_account_with_status(
                institution_code,
                account,
                AdminAccountStatus::Pending,
            )?
            .admins
            .into_inner()
            .into_iter()
            .map(|admin| admin.account_id)
            .collect(),
        )
    }

    pub fn pending_account_admin_records_for_snapshot(
        institution_code: InstitutionCode,
        account: T::AccountId,
    ) -> Option<Vec<Admin<T::AccountId>>> {
        Some(
            Self::admin_account_with_status(
                institution_code,
                account,
                AdminAccountStatus::Pending,
            )?
            .admins
            .into_inner(),
        )
    }

    pub fn pending_account_admins_len_for_snapshot(
        institution_code: InstitutionCode,
        account: T::AccountId,
    ) -> Option<u32> {
        Some(
            Self::admin_account_with_status(institution_code, account, AdminAccountStatus::Pending)?
                .admins
                .len() as u32,
        )
    }

    pub(crate) fn try_execute_set_change_from_action(
        proposal_id: u128,
        action: AdminSetChangeAction<T::AccountId, AdminsOf<T>>,
    ) -> DispatchResult {
        let proposal = engingcore::Pallet::<T>::proposals(proposal_id)
            .ok_or(Error::<T>::ProposalActionNotFound)?;
        ensure!(
            proposal.kind == PROPOSAL_KIND_INTERNAL && proposal.stage == STAGE_INTERNAL,
            Error::<T>::ProposalActionNotFound
        );
        ensure!(
            proposal.status == STATUS_PASSED,
            Error::<T>::ProposalActionNotFound
        );
        ensure!(
            proposal.execution_account_id == Some(action.personal_account_id.clone())
                && proposal.actor_cid_number.is_none(),
            Error::<T>::ProposalActionNotFound
        );
        ensure!(
            proposal.internal_code == Some(PMUL),
            Error::<T>::ProposalActionNotFound
        );
        engingcore::Pallet::<T>::ensure_admin_set_mutation_lock_owner(
            ProposalSubject::PersonalAccount(action.personal_account_id.clone()),
            proposal_id,
        )?;

        let current = AdminAccounts::<T>::get(action.personal_account_id.clone())
            .ok_or(Error::<T>::PersonalNotFound)?;
        ensure!(
            current.status == AdminAccountStatus::Active
                && current.kind == AdminAccountKind::PersonalMultisig
                && current.institution_code == PMUL,
            Error::<T>::NotPersonalAccount
        );
        let current_admins = current.admins.clone().into_inner();
        Self::validate_admin_set_for_change(&action.admins, action.new_threshold)?;
        let current_threshold =
            T::InternalVoteEngine::active_personal_threshold(action.personal_account_id.clone())
                .ok_or(Error::<T>::InvalidThreshold)?;
        ensure!(
            !Self::same_admin_set(current_admins.as_slice(), action.admins.as_slice())
                || current_threshold != action.new_threshold,
            Error::<T>::AdminSetUnchanged
        );

        AdminAccounts::<T>::mutate(action.personal_account_id.clone(), |maybe| {
            if let Some(account) = maybe {
                account.admins = action.admins.clone();
                account.updated_at = frame_system::Pallet::<T>::block_number();
            }
        });
        Self::deposit_event(Event::<T>::AdminSetChanged {
            proposal_id,
            account: action.personal_account_id,
            admins_len: action.admins.len() as u32,
            threshold: action.new_threshold,
        });
        Ok(())
    }
}

/// 成员变更仍按原owner和裸动作解码，否决无成员写入。
pub(crate) fn execute_admin_change<T: Config>(
    proposal_id: u128,
    approved: bool,
) -> Result<ProposalExecutionOutcome, sp_runtime::DispatchError> {
    if !engingcore::Pallet::<T>::is_proposal_owner(proposal_id, crate::ADMIN_MODULE_TAG) {
        return Ok(ProposalExecutionOutcome::Ignored);
    }
    let raw = engingcore::Pallet::<T>::get_proposal_data(proposal_id)
        .ok_or(pallet::Error::<T>::ProposalActionNotFound)?;

    if !approved {
        return Ok(ProposalExecutionOutcome::Executed);
    }

    let action = AdminSetChangeAction::<T::AccountId, pallet::AdminsOf<T>>::decode(&mut &raw[..])
        .map_err(|_| pallet::Error::<T>::ProposalActionNotFound)?;
    match pallet::Pallet::<T>::try_execute_set_change_from_action(proposal_id, action) {
        Ok(()) => Ok(ProposalExecutionOutcome::Executed),
        Err(_) => {
            pallet::Pallet::<T>::deposit_event(pallet::Event::<T>::AdminSetChangeExecutionFailed {
                proposal_id,
            });
            Ok(ProposalExecutionOutcome::FatalFailed)
        }
    }
}
