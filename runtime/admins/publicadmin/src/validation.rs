//! 管理员集合和机构身份的原有校验。
use crate::*;

impl<T: Config> Pallet<T> {
    pub(crate) fn ensure_unique_admins(admins: &[Admin<T::AccountId>]) -> DispatchResult {
        let mut seen = BTreeSet::new();
        let mut seen_cids = BTreeSet::new();
        for admin in admins {
            ensure!(
                seen.insert(admin.account_id.clone()),
                Error::<T>::DuplicateAdmin
            );
            if !admin.cid_number.is_empty() {
                let cid_text = core::str::from_utf8(admin.cid_number.as_slice())
                    .map_err(|_| Error::<T>::InvalidCitizenCid)?;
                let parts = primitives::cid::number::parse_cid_number_parts(cid_text)
                    .map_err(|_| Error::<T>::InvalidCitizenCid)?;
                ensure!(parts.institution == *b"CTZN", Error::<T>::InvalidCitizenCid);
                ensure!(
                    T::CitizenIdentityBinding::matches_citizen_account(
                        admin.cid_number.as_slice(),
                        &admin.account_id,
                    ),
                    Error::<T>::CitizenIdentityMismatch
                );
                ensure!(
                    seen_cids.insert(admin.cid_number.clone()),
                    Error::<T>::CitizenIdentityMismatch
                );
            }
        }
        Ok(())
    }

    pub(crate) fn validate_admin_set(
        kind: AdminAccountKind,
        institution_code: InstitutionCode,
        cid_number: &[u8],
        admins: &[Admin<T::AccountId>],
    ) -> DispatchResult {
        ensure!(
            kind == AdminAccountKind::PublicInstitution
                && can_store_public_admin_code(&institution_code),
            Error::<T>::InvalidAdminAccountKind
        );
        match adminbase::expected_fixed_governance_admins_len(institution_code, cid_number) {
            Some(expected) => {
                ensure!(
                    admins.len() == expected as usize,
                    Error::<T>::InvalidAdminsLen
                )
            }
            None => match primitives::institution_constraints::member_composition_by_identity(
                institution_code,
                cid_number,
            ) {
                Some(spec) => ensure!(
                    admins.len() >= spec.min_members as usize
                        && admins.len() <= spec.max_members as usize,
                    Error::<T>::InvalidAdminsLen
                ),
                None => {
                    ensure!(!admins.is_empty(), Error::<T>::InvalidAdminsLen);
                    ensure!(
                        admins.len() <= <T as Config>::MaxAdminsPerInstitution::get() as usize,
                        Error::<T>::InvalidAdminsLen
                    );
                }
            },
        }
        // 运行期(Operation):公权机构所有管理员四要素完整;Genesis 放行=允许空。
        // 校验原始字段(本 pallet 不 normalize,无默认值掩盖问题)。
        if T::ChainPhase::is_operation() {
            let req =
                adminbase::required_admin_elements(AdminAccountKind::PublicInstitution, false);
            for admin in admins {
                ensure!(admin.satisfies(req), Error::<T>::IncompleteAdminFields);
            }
        }
        Self::ensure_unique_admins(admins)
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
