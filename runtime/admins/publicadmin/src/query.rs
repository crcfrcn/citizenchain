//! 管理员名册及当前身份授权查询。
use crate::*;

impl<T: Config> Pallet<T> {
    pub(crate) fn get_institution_admins(
        institution_code: InstitutionCode,
        cid_number: &[u8],
    ) -> Option<InstitutionAdminsOf<T>> {
        let cid_number = Self::bounded_cid(cid_number)?;
        let value = AdminAccounts::<T>::get(cid_number)?;
        if value.institution_code != institution_code
            || !can_store_public_admin_code(&institution_code)
        {
            return None;
        }
        Some(value)
    }
}

impl<T: pallet::Config> InstitutionAdminQuery<T::AccountId> for pallet::Pallet<T> {
    fn institution_admins_exist(institution_code: InstitutionCode, cid_number: &[u8]) -> bool {
        Self::get_institution_admins(institution_code, cid_number).is_some()
    }

    fn is_institution_admin(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        who: &T::AccountId,
    ) -> bool {
        // 名册成员判断按 account_id（供枚举/投票快照使用）。调用者门的 CID 解析走
        // `resolve_admin_account`，二者语义不同，不可互相替换。
        Self::get_institution_admins(institution_code, cid_number)
            .map(|value| value.admins.iter().any(|admin| &admin.account_id == who))
            .unwrap_or(false)
    }

    fn resolve_admin_account(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        caller: &T::AccountId,
    ) -> Option<T::AccountId> {
        let value = Self::get_institution_admins(institution_code, cid_number)?;
        if Self::wallets_initialized(Self::bounded_cid(cid_number)?) {
            let key = Self::bounded_cid(cid_number)?;
            let anchor = WalletOwners::<T>::get(&key, caller)?;
            return value
                .admins
                .iter()
                .any(|admin| admin.account_id == anchor)
                .then_some(anchor.clone())
                .filter(|_| {
                    AdminWallets::<T>::get(&key, &anchor)
                        .and_then(|state| state.wallet)
                        .as_ref()
                        == Some(caller)
                });
        }
        let operation = T::ChainPhase::is_operation();
        value.admins.iter().find_map(|admin| {
            let matched = if operation && !admin.cid_number.is_empty() {
                // 运行期 + 有 CID：身份锚定，只认该 CID 当前绑定的钱包（无 account_id 回退，旧钱包掉权）。
                T::CitizenIdentityBinding::matches_citizen_account(
                    admin.cid_number.as_slice(),
                    caller,
                )
            } else {
                // 创世期，或无 CID 管理员：钱包锚定（现状）。
                &admin.account_id == caller
            };
            matched.then(|| admin.account_id.clone())
        })
    }

    fn institution_admins(
        institution_code: InstitutionCode,
        cid_number: &[u8],
    ) -> Option<Vec<T::AccountId>> {
        Self::get_institution_admins(institution_code, cid_number).map(|value| {
            value
                .admins
                .into_inner()
                .into_iter()
                .map(|admin| admin.account_id)
                .collect()
        })
    }

    fn institution_admins_len(institution_code: InstitutionCode, cid_number: &[u8]) -> Option<u32> {
        Self::get_institution_admins(institution_code, cid_number)
            .map(|value| value.admins.len() as u32)
    }
}

// 新钱包接口只读取基础状态；原 InstitutionAdminQuery / resolve_admin_account 未切换。
impl<T: pallet::Config> InstitutionAdminWalletQuery<T::AccountId> for pallet::Pallet<T> {
    fn institution_wallets_initialized(
        institution_code: InstitutionCode,
        cid_number: &[u8],
    ) -> bool {
        Self::get_institution_admins(institution_code, cid_number).is_some()
            && Self::bounded_cid(cid_number).is_some_and(WalletsInitialized::<T>::get)
    }
    fn institution_wallet_state(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        admin_account: &T::AccountId,
    ) -> Option<InstitutionAdminWalletState<T::AccountId>> {
        Self::get_institution_admins(institution_code, cid_number)?;
        AdminWallets::<T>::get(Self::bounded_cid(cid_number)?, admin_account)
    }

    fn current_institution_wallet(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        admin_account: &T::AccountId,
    ) -> Option<T::AccountId> {
        let roster = Self::get_institution_admins(institution_code, cid_number)?;
        if !roster
            .admins
            .iter()
            .any(|admin| &admin.account_id == admin_account)
        {
            return None;
        }
        AdminWallets::<T>::get(Self::bounded_cid(cid_number)?, admin_account)?.wallet
    }
}

impl<T: Config> Pallet<T> {
    pub(crate) fn legacy_institution_wallet(
        _cid: &AdminCidNumber,
        admin: &Admin<T::AccountId>,
    ) -> Option<T::AccountId> {
        if T::ChainPhase::is_operation() {
            let effective_cid = (!admin.cid_number.is_empty()).then(|| admin.cid_number.to_vec());
            if let Some(cid) = effective_cid {
                let wallet = T::CitizenIdentityBinding::current_account(&cid)?;
                return T::CitizenIdentityBinding::matches_citizen_account(&cid, &wallet)
                    .then_some(wallet);
            }
        }
        Some(admin.account_id.clone())
    }
}
