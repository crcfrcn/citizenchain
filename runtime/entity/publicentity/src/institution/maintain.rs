//! 按登记权限更新公权机构资料和名称。

extern crate alloc;

use alloc::vec::Vec;
use frame_support::ensure;
use sp_runtime::DispatchResult;

use crate::pallet::{self, AccountNameOf, CidNumberOf, Error, Event, Institutions, Pallet};
use crate::traits::RegistryAuthority;

/// 注册局改机构全称/简称:链是唯一真源;机构码/CID/省市码不可改故不给参数。
pub(crate) fn do_update_institution_info<T: pallet::Config>(
    submitter: T::AccountId,
    cid_number: CidNumberOf<T>,
    cid_full_name: AccountNameOf<T>,
    cid_short_name: AccountNameOf<T>,
    actor_cid_number: Vec<u8>,
    actor_role_code: Vec<u8>,
) -> DispatchResult {
    ensure!(!cid_number.is_empty(), Error::<T>::EmptyCidNumber);
    ensure!(!cid_full_name.is_empty(), Error::<T>::EmptyAccountName);
    ensure!(!cid_short_name.is_empty(), Error::<T>::EmptyAccountName);

    let info = Institutions::<T>::get(&cid_number).ok_or(Error::<T>::InstitutionNotFound)?;
    // 授权唯一真源:extrinsic 签名者是注册局在册管理员,且对目标机构有登记权。
    ensure!(
        T::RegistryAuthority::can_register_institution_origin(
            &submitter,
            actor_cid_number.as_slice(),
            actor_role_code.as_slice(),
            cid_number.as_slice(),
            info.institution_code,
        ),
        Error::<T>::RegistryAuthorityDenied
    );

    Institutions::<T>::mutate(&cid_number, |maybe| {
        if let Some(info) = maybe {
            info.cid_full_name = cid_full_name.clone();
            info.cid_short_name = cid_short_name.clone();
        }
    });
    Pallet::<T>::deposit_event(Event::<T>::InstitutionInfoUpdated {
        cid_number,
        cid_full_name,
        cid_short_name,
        submitter,
    });
    Ok(())
}
