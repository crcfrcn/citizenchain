//! 机构信息维护:改名(`do_update_institution_info`)。
//!
//! 链是机构信息唯一真源(ADR-031);创世只铸定初始集,今后改名走注册局交易。
//! 新增账户已改为机构自身提案+内部投票流程,见 `crate::add`。
//! 授权唯一真源 = 注册局机构 CID + 岗位码 + 任职管理员账户;管理员身份本身不授权。
//! 省/市作用域由目标 CID 直接派生,不再嵌独立凭证/签名/nonce。
//! 机构码/CID/省市码物理编码在 CID 里,改不了也不给参数。防重放由 extrinsic 账户 nonce 承担。

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
