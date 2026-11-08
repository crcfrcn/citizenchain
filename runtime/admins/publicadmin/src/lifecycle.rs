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
        let admins = if institution_code == adminbase::FRG {
            if let Some(existing) = AdminAccounts::<T>::get(&cid_number) {
                Self::merge_frg_admins(&cid_number, kind, existing, admins)?
            } else {
                Self::validate_admin_set(kind, institution_code, cid_number.as_slice(), &admins)?;
                admins
            }
        } else {
            Self::validate_admin_set(kind, institution_code, cid_number.as_slice(), &admins)?;
            admins
        };
        let bounded: AdminsOf<T> = admins
            .try_into()
            .map_err(|_| Error::<T>::InvalidAdminsLen)?;
        let admins_len = bounded.len() as u32;

        with_transaction(|| {
            let created = match AdminAccounts::<T>::get(&cid_number) {
                Some(existing) => {
                    if existing.institution_code != institution_code {
                        return TransactionOutcome::Rollback(Err(
                            Error::<T>::InstitutionCodeMismatch.into(),
                        ));
                    }
                    AdminAccounts::<T>::insert(
                        &cid_number,
                        InstitutionAdmins {
                            institution_code,
                            admins: bounded.clone(),
                        },
                    );
                    false
                }
                None => {
                    AdminAccounts::<T>::insert(
                        &cid_number,
                        InstitutionAdmins {
                            institution_code,
                            admins: bounded.clone(),
                        },
                    );
                    true
                }
            };
            Self::deposit_event(Event::<T>::InstitutionAdminsSet {
                cid_number,
                institution_code,
                admins_len,
                created,
            });
            TransactionOutcome::Commit(Ok(()))
        })
    }

    fn merge_frg_admins(
        cid: &AdminCidNumber,
        kind: AdminAccountKind,
        existing: InstitutionAdminsOf<T>,
        proposed: Vec<Admin<T::AccountId>>,
    ) -> Result<Vec<Admin<T::AccountId>>, DispatchError> {
        ensure!(
            kind == AdminAccountKind::PublicInstitution,
            Error::<T>::InvalidAdminAccountKind
        );
        ensure!(
            existing.institution_code == adminbase::FRG,
            Error::<T>::InstitutionCodeMismatch
        );
        let expected = adminbase::expected_fixed_governance_admins_len(adminbase::FRG, cid)
            .ok_or(Error::<T>::InvalidInstitution)? as usize;
        ensure!(
            existing.admins.len() == expected && proposed.len() == expected,
            Error::<T>::InvalidAdminsLen
        );

        let proposed_ids: BTreeSet<_> = proposed
            .iter()
            .map(|admin| admin.account_id.clone())
            .collect();
        ensure!(proposed_ids.len() == expected, Error::<T>::DuplicateAdmin);
        let existing_ids: BTreeSet<_> = existing
            .admins
            .iter()
            .map(|admin| admin.account_id.clone())
            .collect();
        ensure!(existing_ids.len() == expected, Error::<T>::DuplicateAdmin);
        let incoming: Vec<_> = proposed
            .into_iter()
            .filter(|admin| !existing_ids.contains(&admin.account_id))
            .collect();
        ensure!(
            incoming.len() <= primitives::count_const::FRG_PROVINCE_GROUP_ADMIN_COUNT as usize,
            Error::<T>::InvalidAdminsLen
        );
        if T::ChainPhase::is_operation() {
            let required = adminbase::required_admin_elements(kind, false);
            ensure!(
                incoming.iter().all(|admin| admin.satisfies(required)),
                Error::<T>::IncompleteAdminFields
            );
        }
        Self::ensure_unique_admins(&incoming)?;

        // 保留在任记录及顺序，只在离任账户原位置写入接任记录。
        let mut incoming = incoming.into_iter();
        let merged: Vec<_> = existing
            .admins
            .into_inner()
            .into_iter()
            .map(|admin| {
                if proposed_ids.contains(&admin.account_id) {
                    Ok(admin)
                } else {
                    incoming.next().ok_or(Error::<T>::InvalidAdminsLen)
                }
            })
            .collect::<Result<_, _>>()?;
        let mut cids = BTreeSet::new();
        ensure!(
            merged
                .iter()
                .filter(|admin| !admin.cid_number.is_empty())
                .all(|admin| cids.insert(admin.cid_number.clone())),
            Error::<T>::CitizenIdentityMismatch
        );
        Ok(merged)
    }
}

impl<T: pallet::Config> InstitutionAdminLifecycle<T::AccountId, Admin<T::AccountId>>
    for pallet::Pallet<T>
{
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
