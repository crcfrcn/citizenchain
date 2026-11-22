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
        Self::do_set_institution_admins(
            cid_number,
            institution_code,
            AdminAccountKind::PrivateInstitution,
            admins,
        )
    }
}

impl<T: pallet::Config> InstitutionAdminLifecycle<T::AccountId> for pallet::Pallet<T> {
    fn set_institution_admins(
        _module_tag: &[u8],
        cid_number: Vec<u8>,
        institution_code: InstitutionCode,
        kind: AdminAccountKind,
        admins: Vec<Admin<T::AccountId>>,
    ) -> DispatchResult {
        Self::do_set_institution_admins(cid_number, institution_code, kind, admins)
    }
}

// 仅供受控实体入口调用：本人双签、投票执行与名册生命周期共用同一状态内核。
impl<T: Config> Pallet<T> {
    fn wallet_roster(
        institution_code: InstitutionCode,
        cid: &AdminCidNumber,
    ) -> Result<InstitutionAdminsOf<T>, sp_runtime::DispatchError> {
        ensure!(
            !cid.is_empty() && can_store_private_admin_code(&institution_code),
            Error::<T>::InvalidInstitution
        );
        let roster = AdminAccounts::<T>::get(cid).ok_or(Error::<T>::InvalidInstitution)?;
        ensure!(
            roster.institution_code == institution_code,
            Error::<T>::InstitutionCodeMismatch
        );
        Ok(roster)
    }

    fn wallet_error(error: adminbase::InstitutionAdminWalletError) -> sp_runtime::DispatchError {
        use adminbase::InstitutionAdminWalletError as WalletError;
        match error {
            WalletError::Uninitialized => Error::<T>::WalletUninitialized,
            WalletError::AlreadyInitialized => Error::<T>::WalletAlreadyInitialized,
            WalletError::Inactive => Error::<T>::WalletInactive,
            WalletError::OldWalletMismatch => Error::<T>::WalletOldMismatch,
            WalletError::StaleRevision => Error::<T>::WalletStaleRevision,
            WalletError::Unchanged => Error::<T>::WalletUnchanged,
            WalletError::DuplicateWallet => Error::<T>::WalletDuplicate,
            WalletError::RevisionOverflow => Error::<T>::WalletRevisionOverflow,
        }
        .into()
    }

    fn other_institution_wallets(
        cid: &AdminCidNumber,
        roster: &InstitutionAdminsOf<T>,
        admin_account: &T::AccountId,
        require_initialized: bool,
    ) -> Result<Vec<T::AccountId>, sp_runtime::DispatchError> {
        let mut wallets = Vec::new();
        for admin in &roster.admins {
            if &admin.account_id == admin_account {
                continue;
            }
            match AdminWallets::<T>::get(cid, &admin.account_id).and_then(|state| state.wallet) {
                Some(wallet) => wallets.push(wallet),
                None => ensure!(!require_initialized, Error::<T>::WalletUninitialized),
            }
        }
        Ok(wallets)
    }

    #[frame_support::transactional]
    pub(crate) fn do_initialize_institution_wallet(
        institution_code: InstitutionCode,
        cid_number: Vec<u8>,
        admin_account: T::AccountId,
        wallet: T::AccountId,
        expected_revision: Option<u64>,
    ) -> DispatchResult {
        let cid = Self::bound_cid(cid_number)?;
        let roster = Self::wallet_roster(institution_code, &cid)?;
        ensure!(
            roster
                .admins
                .iter()
                .any(|admin| admin.account_id == admin_account),
            Error::<T>::WalletAdminNotFound
        );
        let previous = AdminWallets::<T>::get(&cid, &admin_account);
        // 初始化阶段缺失记录保持缺失，绝不回退到名册锚点或个人CID绑定。
        let others = Self::other_institution_wallets(&cid, &roster, &admin_account, false)?;
        let next = adminbase::initial_institution_wallet_state(
            previous.as_ref(),
            &wallet,
            expected_revision,
            &others,
        )
        .map_err(Self::wallet_error)?;
        AdminWallets::<T>::insert(&cid, &admin_account, &next);
        WalletOwners::<T>::insert(&cid, &wallet, &admin_account);
        Self::deposit_event(Event::<T>::InstitutionAdminWalletInitialized {
            cid_number: cid,
            admin_account,
            wallet,
            revision: next.revision,
        });
        Ok(())
    }

    #[frame_support::transactional]
    pub(crate) fn do_change_institution_wallet(
        institution_code: InstitutionCode,
        cid_number: Vec<u8>,
        change: InstitutionAdminWalletChange<T::AccountId>,
    ) -> DispatchResult {
        let cid = Self::bound_cid(cid_number)?;
        let roster = Self::wallet_roster(institution_code, &cid)?;
        ensure!(
            roster
                .admins
                .iter()
                .any(|admin| admin.account_id == change.admin_account),
            Error::<T>::WalletAdminNotFound
        );
        let previous = AdminWallets::<T>::get(&cid, &change.admin_account);
        // 更新前本机构所有在册成员须初始化，不能遗漏尚未知晓的有效钱包。
        let others = if WalletsInitialized::<T>::get(&cid) {
            ensure!(
                WalletOwners::<T>::get(&cid, &change.expected_wallet).as_ref()
                    == Some(&change.admin_account),
                Error::<T>::WalletOldMismatch
            );
            WalletOwners::<T>::get(&cid, &change.new_wallet)
                .filter(|owner| owner != &change.admin_account)
                .map(|_| Vec::from([change.new_wallet.clone()]))
                .unwrap_or_default()
        } else {
            Self::other_institution_wallets(&cid, &roster, &change.admin_account, true)?
        };
        let next = adminbase::changed_institution_wallet_state(previous.as_ref(), &change, &others)
            .map_err(Self::wallet_error)?;
        AdminWallets::<T>::insert(&cid, &change.admin_account, &next);
        WalletOwners::<T>::remove(&cid, &change.expected_wallet);
        WalletOwners::<T>::insert(&cid, &change.new_wallet, &change.admin_account);
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
        institution_code: InstitutionCode,
        cid_number: Vec<u8>,
        admin_account: T::AccountId,
        expected_revision: u64,
    ) -> DispatchResult {
        let cid = Self::bound_cid(cid_number)?;
        ensure!(
            !cid.is_empty() && can_store_private_admin_code(&institution_code),
            Error::<T>::InvalidInstitution
        );
        if let Some(roster) = AdminAccounts::<T>::get(&cid) {
            ensure!(
                roster.institution_code == institution_code,
                Error::<T>::InstitutionCodeMismatch
            );
            ensure!(
                !roster
                    .admins
                    .iter()
                    .any(|admin| admin.account_id == admin_account),
                Error::<T>::WalletAdminStillPresent
            );
        } else {
            // 名册已关闭时仍核对键中的机构码，不能借错误路由改变历史版本。
            let text =
                core::str::from_utf8(cid.as_slice()).map_err(|_| Error::<T>::InvalidInstitution)?;
            ensure!(
                primitives::cid::code::institution_code_from_cid_number(text)
                    == Some(institution_code),
                Error::<T>::InstitutionCodeMismatch
            );
        }
        let previous = AdminWallets::<T>::get(&cid, &admin_account);
        let next =
            adminbase::retired_institution_wallet_state(previous.as_ref(), expected_revision)
                .map_err(Self::wallet_error)?;
        let previous_wallet = previous
            .and_then(|state| state.wallet)
            .ok_or(Error::<T>::WalletInactive)?;
        AdminWallets::<T>::insert(&cid, &admin_account, &next);
        if WalletOwners::<T>::get(&cid, &previous_wallet).as_ref() == Some(&admin_account) {
            WalletOwners::<T>::remove(&cid, &previous_wallet);
        }
        Self::deposit_event(Event::<T>::InstitutionAdminWalletRetired {
            cid_number: cid,
            admin_account,
            previous_wallet,
            revision: next.revision,
        });
        Ok(())
    }
}

impl<T: pallet::Config> InstitutionAdminWalletLifecycle<T::AccountId> for pallet::Pallet<T> {
    fn ensure_institution_wallets(code: InstitutionCode, cid: Vec<u8>) -> DispatchResult {
        Self::do_ensure_institution_wallets(code, cid)
    }

    fn initialize_institution_wallet(
        institution_code: InstitutionCode,
        cid_number: Vec<u8>,
        admin_account: T::AccountId,
        wallet: T::AccountId,
        expected_revision: Option<u64>,
    ) -> DispatchResult {
        Self::do_initialize_institution_wallet(
            institution_code,
            cid_number,
            admin_account,
            wallet,
            expected_revision,
        )
    }

    fn change_institution_wallet(
        institution_code: InstitutionCode,
        cid_number: Vec<u8>,
        change: InstitutionAdminWalletChange<T::AccountId>,
    ) -> DispatchResult {
        Self::do_change_institution_wallet(institution_code, cid_number, change)
    }

    fn retire_institution_wallet(
        institution_code: InstitutionCode,
        cid_number: Vec<u8>,
        admin_account: T::AccountId,
        expected_revision: u64,
    ) -> DispatchResult {
        Self::do_retire_institution_wallet(
            institution_code,
            cid_number,
            admin_account,
            expected_revision,
        )
    }
}

impl<T: Config> Pallet<T> {
    #[frame_support::transactional]
    pub(crate) fn do_ensure_institution_wallets(
        code: InstitutionCode,
        cid: Vec<u8>,
    ) -> DispatchResult {
        let cid = Self::bound_cid(cid)?;
        let roster = Self::wallet_roster(code, &cid)?;
        if WalletsInitialized::<T>::get(&cid) {
            return Ok(());
        }
        let mut used = BTreeSet::new();
        let mut updates = Vec::new();
        for admin in &roster.admins {
            let wallet = Self::legacy_institution_wallet(&cid, admin)
                .ok_or(Error::<T>::WalletLegacyUnavailable)?;
            ensure!(used.insert(wallet.clone()), Error::<T>::WalletDuplicate);
            let previous = AdminWallets::<T>::get(&cid, &admin.account_id);
            if let Some(state) = &previous {
                if let Some(existing) = &state.wallet {
                    ensure!(existing == &wallet, Error::<T>::WalletLegacyMismatch);
                    WalletOwners::<T>::insert(&cid, &wallet, &admin.account_id);
                    continue;
                }
            }
            let expected = previous.as_ref().map(|state| state.revision);
            let next = adminbase::initial_institution_wallet_state(
                previous.as_ref(),
                &wallet,
                expected,
                &[],
            )
            .map_err(Self::wallet_error)?;
            updates.push((admin.account_id.clone(), wallet, next));
        }
        for (anchor, wallet, state) in updates {
            AdminWallets::<T>::insert(&cid, &anchor, &state);
            WalletOwners::<T>::insert(&cid, &wallet, &anchor);
            Self::deposit_event(Event::<T>::InstitutionAdminWalletInitialized {
                cid_number: cid.clone(),
                admin_account: anchor,
                wallet,
                revision: state.revision,
            });
        }
        WalletsInitialized::<T>::insert(&cid, true);
        Ok(())
    }

    /// 名册生命周期与钱包同事务，保留在任人员的钱包，离任退役，重录版本不回退。
    fn maintain_institution_wallets(
        cid: &AdminCidNumber,
        previous: Option<&InstitutionAdminsOf<T>>,
        next: &InstitutionAdminsOf<T>,
    ) -> DispatchResult {
        let mut used = BTreeSet::new();
        let initialized = WalletsInitialized::<T>::get(cid);
        if let Some(old) = previous {
            for admin in &old.admins {
                if let Some(wallet) =
                    AdminWallets::<T>::get(cid, &admin.account_id).and_then(|state| state.wallet)
                {
                    WalletOwners::<T>::remove(cid, wallet);
                }
            }
        }
        for admin in &next.admins {
            let retained = previous.is_some_and(|old| {
                old.admins
                    .iter()
                    .any(|item| item.account_id == admin.account_id)
            });
            let old_state = AdminWallets::<T>::get(cid, &admin.account_id);
            let state = if retained && initialized {
                let state = old_state.ok_or(Error::<T>::WalletUninitialized)?;
                ensure!(state.wallet.is_some(), Error::<T>::WalletInactive);
                state
            } else {
                let wallet = if retained {
                    Self::legacy_institution_wallet(cid, admin)
                        .ok_or(Error::<T>::WalletLegacyUnavailable)?
                } else {
                    admin.account_id.clone()
                };
                if retained
                    && old_state
                        .as_ref()
                        .is_some_and(|state| state.wallet.is_some())
                {
                    let state = old_state.ok_or(Error::<T>::WalletUninitialized)?;
                    ensure!(
                        state.wallet.as_ref() == Some(&wallet),
                        Error::<T>::WalletLegacyMismatch
                    );
                    state
                } else {
                    let expected = old_state.as_ref().map(|state| state.revision);
                    adminbase::initial_institution_wallet_state(
                        old_state.as_ref(),
                        &wallet,
                        expected,
                        &[],
                    )
                    .map_err(Self::wallet_error)?
                }
            };
            let wallet = state.wallet.as_ref().ok_or(Error::<T>::WalletInactive)?;
            ensure!(used.insert(wallet.clone()), Error::<T>::WalletDuplicate);
            WalletOwners::<T>::insert(cid, wallet, &admin.account_id);
            AdminWallets::<T>::insert(cid, &admin.account_id, state);
        }
        if let Some(old) = previous {
            for admin in &old.admins {
                if next
                    .admins
                    .iter()
                    .any(|item| item.account_id == admin.account_id)
                {
                    continue;
                }
                let before = AdminWallets::<T>::get(cid, &admin.account_id);
                let revision = before
                    .as_ref()
                    .map_or(0, |state| state.revision)
                    .checked_add(1)
                    .ok_or(Error::<T>::WalletRevisionOverflow)?;
                AdminWallets::<T>::insert(
                    cid,
                    &admin.account_id,
                    InstitutionAdminWalletState::<T::AccountId> {
                        wallet: None,
                        revision,
                    },
                );
            }
        }
        WalletsInitialized::<T>::insert(cid, true);
        Ok(())
    }
}
