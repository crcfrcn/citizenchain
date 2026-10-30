//! 管理员集合和机构身份的原有校验。
use crate::*;

impl<T: Config> Pallet<T> {
    pub(crate) fn validate_admin_set(
        kind: AdminAccountKind,
        institution_code: InstitutionCode,
        admins: &[Admin<T::AccountId>],
    ) -> DispatchResult {
        ensure!(
            kind == AdminAccountKind::PrivateInstitution
                && can_store_private_admin_code(&institution_code),
            Error::<T>::InvalidAdminAccountKind
        );
        ensure!(!admins.is_empty(), Error::<T>::InvalidAdminsLen);
        ensure!(
            admins.len() <= <T as Config>::MaxAdminsPerInstitution::get() as usize,
            Error::<T>::InvalidAdminsLen
        );
        let mut seen = BTreeSet::new();
        for admin in admins {
            ensure!(!admin.family_name.is_empty(), Error::<T>::InvalidFamilyName);
            ensure!(!admin.given_name.is_empty(), Error::<T>::InvalidGivenName);
            ensure!(
                seen.insert(admin.account_id.clone()),
                Error::<T>::DuplicateAdmin
            );
        }
        Ok(())
    }

    pub(crate) fn bound_cid(cid_number: Vec<u8>) -> Result<AdminCidNumber, DispatchError> {
        cid_number
            .try_into()
            .map_err(|_| Error::<T>::InvalidInstitution.into())
    }

    pub(crate) fn bounded_cid(cid_number: &[u8]) -> Option<AdminCidNumber> {
        cid_number.to_vec().try_into().ok()
    }
}
