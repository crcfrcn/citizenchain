//! 按机构CID维护私权管理员名册，提供生命周期和授权查询。

#![cfg_attr(not(feature = "std"), no_std)]
//! 私权机构管理员账户集合模块。
//!
//! 机构唯一身份是 CID。本模块保存原有 `AdminAccounts[cid_number] -> admins` 与独立的机构钱包授权状态；
//! 岗位、任职和机构投票阈值归 entity，机构账户不参与管理员寻址。

extern crate alloc;

use alloc::vec::Vec;
use frame_support::{
    ensure,
    pallet_prelude::*,
    storage::{with_transaction, TransactionOutcome},
    traits::StorageVersion,
    Blake2_128Concat,
};
use frame_system::pallet_prelude::*;
use sp_std::collections::btree_set::BTreeSet;

use adminbase::{
    can_store_private_admin_code, Admin, AdminAccountKind, AdminCidNumber,
    InstitutionAdminLifecycle, InstitutionAdminQuery,
    InstitutionAdminWalletChange, InstitutionAdminWalletLifecycle, InstitutionAdminWalletQuery,
    InstitutionAdminWalletState, InstitutionAdmins,
};
use engingcore::types::InstitutionCode;

pub use pallet::*;

/// 正式创世只接受统一管理员记录，不保留旧纯账户或单姓名存储迁移。
const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    #[pallet::config]
    pub trait Config: frame_system::Config + engingcore::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        #[pallet::constant]
        type MaxAdminsPerInstitution: Get<u32>;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    pub type AdminsOf<T> = BoundedVec<
        Admin<<T as frame_system::Config>::AccountId>,
        <T as Config>::MaxAdminsPerInstitution,
    >;
    pub type InstitutionAdminsOf<T> = InstitutionAdmins<AdminsOf<T>>;

    /// 私权机构管理员集合。CID 是唯一 key；value 不重复保存 CID 或生命周期状态。
    #[pallet::storage]
    #[pallet::getter(fn institution_admins_of)]
    pub type AdminAccounts<T: Config> =
        StorageMap<_, Blake2_128Concat, AdminCidNumber, InstitutionAdminsOf<T>, OptionQuery>;

    /// 完整机构CID + 实际钱包账户 → 授权状态。None钱包保留退役版本。
    /// 机构签名校验实际名单与本状态，名册生命周期同步维护。
    #[pallet::storage]
    #[pallet::getter(fn stored_admin_wallet)]
    pub type AdminWallets<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        AdminCidNumber,
        Blake2_128Concat,
        T::AccountId,
        InstitutionAdminWalletState<T::AccountId>,
        OptionQuery,
    >;

    /// 当前钱包使用的已批准岗位签名授权引用。仅供岗位校验，不作为管理员或票据身份。
    #[pallet::storage]
    pub type WalletRoleBindings<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        AdminCidNumber,
        Blake2_128Concat,
        T::AccountId,
        T::AccountId,
        OptionQuery,
    >;

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub _phantom: core::marker::PhantomData<T>,
    }

    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self {
                _phantom: Default::default(),
            }
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn integrity_test() {
            assert!(
                <T as Config>::MaxAdminsPerInstitution::get() >= 1,
                "MaxAdminsPerInstitution must be >= 1"
            );
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {}
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        InstitutionAdminsSet {
            cid_number: AdminCidNumber,
            institution_code: InstitutionCode,
            admins_len: u32,
            created: bool,
        },
        /// 名册登记同步建立钱包版本。事件编码位置保留，登录不再执行初始化。
        InstitutionAdminWalletInitialized {
            cid_number: AdminCidNumber,
            admin_account: T::AccountId,
            wallet: T::AccountId,
            revision: u64,
        },
        InstitutionAdminWalletChanged {
            cid_number: AdminCidNumber,
            admin_account: T::AccountId,
            previous_wallet: T::AccountId,
            new_wallet: T::AccountId,
            revision: u64,
        },
        InstitutionAdminWalletRetired {
            cid_number: AdminCidNumber,
            admin_account: T::AccountId,
            previous_wallet: T::AccountId,
            revision: u64,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        InvalidInstitution,
        InstitutionCodeMismatch,
        InvalidAdminsLen,
        InvalidAdminAccountKind,
        DuplicateAdmin,
        InvalidFamilyName,
        InvalidGivenName,
        WalletAdminNotFound,
        WalletAdminStillPresent,
        WalletUninitialized,
        WalletAlreadyInitialized,
        WalletInactive,
        WalletOldMismatch,
        WalletStaleRevision,
        WalletUnchanged,
        WalletDuplicate,
        WalletRevisionOverflow,
    }
}

// 原名册校验、写入与查询保持原行为；钱包基础状态独立追加，尚无对外调用。
mod lifecycle;
mod query;
mod validation;

#[cfg(test)]
#[path = "../tests/mod.rs"]
mod tests;
