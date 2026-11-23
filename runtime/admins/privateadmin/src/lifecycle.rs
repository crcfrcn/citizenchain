//! 管理员集合事务写入和生命周期接口。
use crate::*;

impl<T: Config> Pallet<T> {
    pub(crate) fn do_set_institution_admins(
        cid_number: Vec<u8>,
        institution_code: InstitutionCode,
        kind: AdminAccountKind,
        admins: Vec<Admin<T::AccountId>>,
    ) -> DispatchResult {
        let cid_number = Self::bound_cid(cid_number)?;
        let admins = admins
            .into_iter()
            .map(Admin::normalize_names)
            .collect::<Vec<_>>();
        Self::validate_admin_set(kind, institution_code, &admins)?;
        let bounded: AdminsOf<T> = admins
            .try_into()
            .map_err(|_| Error::<T>::InvalidAdminsLen)?;
        let admins_len = bounded.len() as u32;

        with_transaction(|| {
            let previous = AdminAccounts::<T>::get(&cid_number);
            if previous
                .as_ref()
                .is_some_and(|old| old.institution_code != institution_code)
            {
                return TransactionOutcome::Rollback(Err(
                    Error::<T>::InstitutionCodeMismatch.into()
                ));
            }
            let next = InstitutionAdmins {
                institution_code,
                admins: bounded.clone(),
            };
            if let Err(error) =
                Self::maintain_institution_wallets(&cid_number, previous.as_ref(), &next)
            {
                return TransactionOutcome::Rollback(Err(error));
            }
            let created = match previous {
                Some(existing) => {
                    if existing.institution_code != institution_code {
                        return TransactionOutcome::Rollback(Err(
                            Error::<T>::InstitutionCodeMismatch.into(),
                        ));
                    }
                    false
                }
                None => true,
            };
            AdminAccounts::<T>::insert(
                &cid_number,
                InstitutionAdmins {
                    institution_code,
                    admins: bounded,
                },
            );
            Self::deposit_event(Event::<T>::InstitutionAdminsSet {
                cid_number,
                institution_code,
                admins_len,
                created,
            });
            TransactionOutcome::Commit(Ok(()))
        })
    }

    /// 创世专用写入口：复用运行期完整管理员校验，机构阈值由 entity 独立写入。
    pub fn store_genesis_institution_admins(
        cid_number: Vec<u8>,
        institution_code: InstitutionCode,
        admins: Vec<Admin<T::AccountId>>,
    ) -> DispatchResult {
        let key = Self::bound_cid(cid_number.clone())?;
        ensure!(
            !AdminAccounts::<T>::contains_key(&key),
            Error::<T>::InvalidInstitution
        );
        Self::do_set_institution_admins(
            cid_number,
            institution_code,
            AdminAccountKind::PrivateInstitution,
            admins,
        )
    }
}

impl<T: pallet::Config> InstitutionAdminLifecycle<T::AccountId> for pallet::Pallet<T> {
    #[frame_support::transactional]
    fn update_institution_wallets(
        raw: Vec<u8>,
        code: InstitutionCode,
        added: Vec<Admin<T::AccountId>>,
        removed: Vec<(T::AccountId, u64)>,
        replaced: Vec<InstitutionAdminWalletChange<T::AccountId>>,
    ) -> DispatchResult {
        let added = added
            .into_iter()
            .map(Admin::normalize_names)
            .collect::<Vec<_>>();
        let cid = Self::bound_cid(raw.clone())?;
        let old = AdminAccounts::<T>::get(&cid);
        ensure!(
            old.as_ref().is_none_or(|r| r.institution_code == code),
            Error::<T>::InstitutionCodeMismatch
        );
        ensure!(
            added
                .len()
                .saturating_add(removed.len())
                .saturating_add(replaced.len())
                > 0
                && added
                    .len()
                    .saturating_add(removed.len())
                    .saturating_add(replaced.len())
                    <= <T as Config>::MaxAdminsPerInstitution::get() as usize,
            Error::<T>::InvalidAdminsLen
        );
        if adminbase::expected_fixed_governance_admins_len(code, &cid).is_some() {
            ensure!(
                added.is_empty() && removed.is_empty(),
                Error::<T>::InvalidAdminsLen
            );
        }
        let mut sources = BTreeSet::new();
        let mut targets = BTreeSet::new();
        for (wallet, revision) in &removed {
            ensure!(sources.insert(wallet.clone()), Error::<T>::DuplicateAdmin);
            ensure!(
                Self::resolve_admin_account(code, &cid, wallet).is_some(),
                Error::<T>::WalletAdminNotFound
            );
            ensure!(
                AdminWallets::<T>::get(&cid, wallet).is_some_and(|s| s.revision == *revision),
                Error::<T>::WalletStaleRevision
            );
        }
        for change in &replaced {
            ensure!(
                sources.insert(change.admin_account.clone())
                    && targets.insert(change.new_wallet.clone()),
                Error::<T>::DuplicateAdmin
            );
        }
        for admin in &added {
            ensure!(
                targets.insert(admin.account_id.clone()),
                Error::<T>::DuplicateAdmin
            );
        }
        ensure!(
            targets.iter().all(|w| !sources.contains(w)
                && !old
                    .as_ref()
                    .is_some_and(|r| r.admins.iter().any(|a| &a.account_id == w))),
            Error::<T>::WalletDuplicate
        );
        if !added.is_empty() {
            Self::validate_admin_set(AdminAccountKind::PrivateInstitution, code, &added)?;
        }
        for change in replaced {
            Self::do_change_institution_wallet(code, raw.clone(), change)?;
        }
        if !added.is_empty() || !removed.is_empty() {
            let current = AdminAccounts::<T>::get(&cid);
            let mut records = current
                .as_ref()
                .map(|r| r.admins.to_vec())
                .unwrap_or_default();
            records.retain(|a| !removed.iter().any(|(w, _)| w == &a.account_id));
            records.extend(added);
            let next = InstitutionAdmins {
                institution_code: code,
                admins: records
                    .try_into()
                    .map_err(|_| Error::<T>::InvalidAdminsLen)?,
            };
            Self::maintain_institution_wallets(&cid, current.as_ref(), &next)?;
            let len = next.admins.len() as u32;
            AdminAccounts::<T>::insert(&cid, next);
            Self::deposit_event(Event::<T>::InstitutionAdminsSet {
                cid_number: cid,
                institution_code: code,
                admins_len: len,
                created: old.is_none(),
            });
        }
        Ok(())
    }
    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_set_institution_admins(
        _module_tag: &[u8],
        cid_number: Vec<u8>,
        institution_code: InstitutionCode,
        kind: AdminAccountKind,
        admins: Vec<Admin<T::AccountId>>,
    ) -> DispatchResult {
        Self::do_set_institution_admins(cid_number, institution_code, kind, admins)
    }
}

// 只维护实际钱包名单与版本。岗位人员、任期、余额和提案快照不在本模块写入。
impl<T: Config> Pallet<T> {
    fn wallet_roster(
        code: InstitutionCode,
        cid: &AdminCidNumber,
    ) -> Result<InstitutionAdminsOf<T>, DispatchError> {
        ensure!(!cid.is_empty(), Error::<T>::InvalidInstitution);
        let roster =
            Self::get_institution_admins(code, cid).ok_or(Error::<T>::InvalidInstitution)?;
        Ok(roster)
    }

    fn wallet_error(error: adminbase::InstitutionAdminWalletError) -> DispatchError {
        use adminbase::InstitutionAdminWalletError as E;
        match error {
            E::Uninitialized => Error::<T>::WalletUninitialized,
            E::AlreadyInitialized => Error::<T>::WalletAlreadyInitialized,
            E::Inactive => Error::<T>::WalletInactive,
            E::OldWalletMismatch => Error::<T>::WalletOldMismatch,
            E::StaleRevision => Error::<T>::WalletStaleRevision,
            E::Unchanged => Error::<T>::WalletUnchanged,
            E::DuplicateWallet => Error::<T>::WalletDuplicate,
            E::RevisionOverflow => Error::<T>::WalletRevisionOverflow,
        }
        .into()
    }
    #[frame_support::transactional]
    pub(crate) fn do_change_institution_wallet(
        code: InstitutionCode,
        raw_cid: Vec<u8>,
        change: InstitutionAdminWalletChange<T::AccountId>,
    ) -> DispatchResult {
        let cid = Self::bound_cid(raw_cid)?;
        let mut roster = Self::wallet_roster(code, &cid)?;
        let index = roster
            .admins
            .iter()
            .position(|a| a.account_id == change.admin_account)
            .ok_or(Error::<T>::WalletAdminNotFound)?;
        ensure!(
            change.admin_account == change.expected_wallet,
            Error::<T>::WalletOldMismatch
        );
        ensure!(
            change.expected_wallet != change.new_wallet,
            Error::<T>::WalletUnchanged
        );
        ensure!(
            !roster
                .admins
                .iter()
                .any(|a| a.account_id == change.new_wallet),
            Error::<T>::WalletDuplicate
        );
        let previous = AdminWallets::<T>::get(&cid, &change.expected_wallet);
        let new_previous = AdminWallets::<T>::get(&cid, &change.new_wallet);
        let reference = WalletRoleBindings::<T>::get(&cid, &change.expected_wallet)
            .ok_or(Error::<T>::WalletUninitialized)?;
        let (retired, next) = adminbase::changed_institution_wallet_state(
            previous.as_ref(),
            new_previous.as_ref(),
            &change,
            &[],
        )
        .map_err(Self::wallet_error)?;
        // 每个(CID,钱包)履历首次建立时持有一次System引用；退役保留，避免nonce归零。
        if new_previous.is_none() {
            let refs = frame_system::Account::<T>::get(&change.new_wallet);
            ensure!(
                refs.providers
                    .checked_add(refs.sufficients)
                    .and_then(|n| n.checked_add(1))
                    .is_some(),
                sp_runtime::ArithmeticError::Overflow
            );
            frame_system::Pallet::<T>::inc_sufficients(&change.new_wallet);
        }
        roster.admins[index].account_id = change.new_wallet.clone();
        AdminAccounts::<T>::insert(&cid, roster);
        AdminWallets::<T>::insert(&cid, &change.expected_wallet, retired);
        AdminWallets::<T>::insert(&cid, &change.new_wallet, &next);
        WalletRoleBindings::<T>::remove(&cid, &change.expected_wallet);
        WalletRoleBindings::<T>::insert(&cid, &change.new_wallet, reference);
        Self::deposit_event(Event::<T>::InstitutionAdminWalletChanged {
            cid_number: cid,
            admin_account: change.admin_account,
            previous_wallet: change.expected_wallet,
            new_wallet: change.new_wallet,
            revision: next.revision,
        });
        Ok(())
    }

    #[frame_support::transactional]
    pub(crate) fn do_retire_institution_wallet(
        code: InstitutionCode,
        raw_cid: Vec<u8>,
        wallet: T::AccountId,
        revision: u64,
    ) -> DispatchResult {
        let cid = Self::bound_cid(raw_cid)?;
        ensure!(
            primitives::cid::code::institution_code_from_cid_number(
                core::str::from_utf8(&cid).map_err(|_| Error::<T>::InvalidInstitution)?
            ) == Some(code),
            Error::<T>::InstitutionCodeMismatch
        );
        if let Some(roster) = AdminAccounts::<T>::get(&cid) {
            ensure!(
                roster.institution_code == code,
                Error::<T>::InstitutionCodeMismatch
            );
            ensure!(
                !roster.admins.iter().any(|a| a.account_id == wallet),
                Error::<T>::WalletAdminStillPresent
            );
        }
        let previous = AdminWallets::<T>::get(&cid, &wallet);
        let retired = adminbase::retired_institution_wallet_state(previous.as_ref(), revision)
            .map_err(Self::wallet_error)?;
        AdminWallets::<T>::insert(&cid, &wallet, &retired);
        WalletRoleBindings::<T>::remove(&cid, &wallet);
        Self::deposit_event(Event::<T>::InstitutionAdminWalletRetired {
            cid_number: cid,
            admin_account: wallet.clone(),
            previous_wallet: wallet,
            revision: retired.revision,
        });
        Ok(())
    }
    /// 新登记写入初值，保留钱包延续版本；缺失的在用记录是错误，不能现场补建。
    fn maintain_institution_wallets(
        cid: &AdminCidNumber,
        previous: Option<&InstitutionAdminsOf<T>>,
        next: &InstitutionAdminsOf<T>,
    ) -> DispatchResult {
        let mut retained_references = BTreeSet::new();
        if let Some(old) = previous {
            for admin in &old.admins {
                let state = AdminWallets::<T>::get(cid, &admin.account_id)
                    .ok_or(Error::<T>::WalletUninitialized)?;
                ensure!(
                    state.wallet.as_ref() == Some(&admin.account_id),
                    Error::<T>::WalletInactive
                );
                let reference = WalletRoleBindings::<T>::get(cid, &admin.account_id)
                    .ok_or(Error::<T>::WalletUninitialized)?;
                if next.admins.iter().any(|a| a.account_id == admin.account_id) {
                    ensure!(
                        retained_references.insert(reference),
                        Error::<T>::WalletDuplicate
                    );
                }
            }
        }
        for admin in &next.admins {
            if previous
                .is_some_and(|old| old.admins.iter().any(|a| a.account_id == admin.account_id))
            {
                continue;
            }
            ensure!(
                retained_references.insert(admin.account_id.clone()),
                Error::<T>::WalletDuplicate
            );
            let prior = AdminWallets::<T>::get(cid, &admin.account_id);
            let state = adminbase::initial_institution_wallet_state(
                prior.as_ref(),
                &admin.account_id,
                prior.as_ref().map(|s| s.revision),
                &[],
            )
            .map_err(Self::wallet_error)?;
            if prior.is_none() {
                let refs = frame_system::Account::<T>::get(&admin.account_id);
                ensure!(
                    refs.providers
                        .checked_add(refs.sufficients)
                        .and_then(|n| n.checked_add(1))
                        .is_some(),
                    sp_runtime::ArithmeticError::Overflow
                );
                frame_system::Pallet::<T>::inc_sufficients(&admin.account_id);
            }
            AdminWallets::<T>::insert(cid, &admin.account_id, state);
            WalletRoleBindings::<T>::insert(cid, &admin.account_id, &admin.account_id);
        }
        if let Some(old) = previous {
            for admin in &old.admins {
                if next.admins.iter().any(|a| a.account_id == admin.account_id) {
                    continue;
                }
                let prior = AdminWallets::<T>::get(cid, &admin.account_id)
                    .ok_or(Error::<T>::WalletUninitialized)?;
                let state =
                    adminbase::retired_institution_wallet_state(Some(&prior), prior.revision)
                        .map_err(Self::wallet_error)?;
                AdminWallets::<T>::insert(cid, &admin.account_id, state);
                WalletRoleBindings::<T>::remove(cid, &admin.account_id);
            }
        }
        Ok(())
    }
}
impl<T: Config> InstitutionAdminWalletLifecycle<T::AccountId> for Pallet<T> {
    fn change_institution_wallet(
        code: InstitutionCode,
        cid: Vec<u8>,
        change: InstitutionAdminWalletChange<T::AccountId>,
    ) -> DispatchResult {
        Self::do_change_institution_wallet(code, cid, change)
    }
    fn retire_institution_wallet(
        code: InstitutionCode,
        cid: Vec<u8>,
        wallet: T::AccountId,
        revision: u64,
    ) -> DispatchResult {
        Self::do_retire_institution_wallet(code, cid, wallet, revision)
    }
}
