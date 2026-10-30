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
        Self::validate_admin_set(kind, institution_code, cid_number.as_slice(), &admins)?;
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
