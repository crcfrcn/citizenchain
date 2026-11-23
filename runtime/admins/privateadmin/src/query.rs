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
            || !can_store_private_admin_code(&institution_code)
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
        // 管理员仅按实际在用钱包判断；不解析个人CID。
        Self::resolve_admin_account(institution_code, cid_number, who).is_some()
    }

    fn resolve_admin_account(
        code: InstitutionCode,
        cid: &[u8],
        caller: &T::AccountId,
    ) -> Option<T::AccountId> {
        let roster = Self::get_institution_admins(code, cid)?;
        if !roster.admins.iter().any(|a| &a.account_id == caller) {
            return None;
        }
        let state = AdminWallets::<T>::get(Self::bounded_cid(cid)?, caller)?;
        (state.wallet.as_ref() == Some(caller)).then(|| caller.clone())
    }
    fn resolve_role_authorization(
        code: InstitutionCode,
        cid: &[u8],
        caller: &T::AccountId,
    ) -> Option<T::AccountId> {
        Self::resolve_admin_account(code, cid, caller)?;
        WalletRoleBindings::<T>::get(Self::bounded_cid(cid)?, caller)
    }
    fn has_role_authorization(code: InstitutionCode, cid: &[u8], reference: &T::AccountId) -> bool {
        Self::get_institution_admins(code, cid).is_some_and(|roster| {
            roster.admins.iter().any(|a| {
                Self::resolve_role_authorization(code, cid, &a.account_id).as_ref()
                    == Some(reference)
            })
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

impl<T: Config> InstitutionAdminWalletQuery<T::AccountId> for Pallet<T> {
    fn institution_wallet_state(
        code: InstitutionCode,
        cid: &[u8],
        wallet: &T::AccountId,
    ) -> Option<InstitutionAdminWalletState<T::AccountId>> {
        Self::get_institution_admins(code, cid)?;
        AdminWallets::<T>::get(Self::bounded_cid(cid)?, wallet)
    }
    fn current_institution_wallet(
        code: InstitutionCode,
        cid: &[u8],
        wallet: &T::AccountId,
    ) -> Option<T::AccountId> {
        Self::resolve_admin_account(code, cid, wallet)
    }
}
