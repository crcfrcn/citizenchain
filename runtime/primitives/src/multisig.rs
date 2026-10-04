//! 多签治理跨 pallet 共用 trait 与轻量类型。

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::DecodeWithMemTracking;
use scale_info::TypeInfo;
use sp_runtime::Debug;
use sp_std::vec::Vec;

// 地址校验与资金保护抽象。

/// 账户地址合法性校验。
pub trait AccountValidator<AccountId> {
    fn is_valid(address: &AccountId) -> bool;
}

impl<AccountId> AccountValidator<AccountId> for () {
    fn is_valid(_address: &AccountId) -> bool {
        true
    }
}

/// 制度保留账户校验。
pub trait ReservedAccountGuard<AccountId> {
    fn is_reserved(address: &AccountId) -> bool;
}

impl<AccountId> ReservedAccountGuard<AccountId> for () {
    fn is_reserved(_address: &AccountId) -> bool {
        false
    }
}

/// 转出源地址保护。
pub trait ProtectedSourceChecker<AccountId> {
    fn is_protected(address: &AccountId) -> bool;
}

impl<AccountId> ProtectedSourceChecker<AccountId> for () {
    fn is_protected(_address: &AccountId) -> bool {
        false
    }
}

// 多签账户管理员配置类型。

/// 多签账户的管理员配置快照。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
#[scale_info(skip_type_params(MaxAdmins))]
pub struct MultisigConfig<AccountId, MaxAdmins>
where
    MaxAdmins: frame_support::traits::Get<u32>,
{
    pub admins: frame_support::BoundedVec<AccountId, MaxAdmins>,
    pub admins_len: u32,
    pub threshold: u32,
}

/// 不带 BoundedVec 约束的 trait 返回快照。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultisigConfigSnapshot<AccountId> {
    pub admins: Vec<AccountId>,
    pub admins_len: u32,
    pub threshold: u32,
}
