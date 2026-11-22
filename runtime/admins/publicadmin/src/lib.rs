//! 按机构CID维护公权管理员名册，核对当前公民账户绑定。

#![cfg_attr(not(feature = "std"), no_std)]
//! 公权机构管理员账户集合模块。
//!
//! 机构唯一身份是 CID。本模块保存原有 `AdminAccounts[cid_number] -> admins` 与独立的机构钱包授权状态；
//! 岗位、任职和机构治理阈值归 entity；投票引擎只读取阈值，机构账户不参与管理员寻址。

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
    can_store_public_admin_code, Admin, AdminAccountKind, AdminCidNumber, ChainPhaseCheck,
    CitizenIdentityBindingQuery, InstitutionAdminLifecycle, InstitutionAdminQuery,
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

        /// 公权管理员非空公民 CID 必须匹配 identity 的唯一账户绑定。
        type CitizenIdentityBinding: CitizenIdentityBindingQuery<Self::AccountId>;

        /// 运行期强制门控(由 genesis 相位注入);仅 Operation 期强制四要素完整。
        type ChainPhase: adminbase::ChainPhaseCheck;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    pub type AdminsOf<T> = BoundedVec<
        Admin<<T as frame_system::Config>::AccountId>,
        <T as Config>::MaxAdminsPerInstitution,
    >;
    pub type InstitutionAdminsOf<T> = InstitutionAdmins<AdminsOf<T>>;

    /// 公权机构管理员集合。CID 是唯一 key；value 不重复保存 CID 或生命周期状态。
    #[pallet::storage]
    #[pallet::getter(fn institution_admins_of)]
    pub type AdminAccounts<T: Config> =
        StorageMap<_, Blake2_128Concat, AdminCidNumber, InstitutionAdminsOf<T>, OptionQuery>;

    /// 机构CID + 名册稳定账户锚点 → 钱包基础状态。None钱包保留退役版本。
    /// 激活后机构签名只读取本状态，名册生命周期同步维护。
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

    /// 每机构初始化完成标记。未完成保持旧读取；完成后缺失钱包绝不回退。
    #[pallet::storage]
    #[pallet::getter(fn wallets_initialized)]
    pub type WalletsInitialized<T: Config> =
        StorageMap<_, Blake2_128Concat, AdminCidNumber, bool, ValueQuery>;

    /// 机构内钱包反向索引。用于已激活机构的唯一性检查，避免每次执行读取整组钱包。
    #[pallet::storage]
    pub type WalletOwners<T: Config> = StorageDoubleMap<
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
        /// 注册局或机构治理写入公权机构管理员人员集合。
        InstitutionAdminsSet {
            cid_number: AdminCidNumber,
            institution_code: InstitutionCode,
            admins_len: u32,
            created: bool,
        },
        /// 钱包内部初始化完成，不表示已切换现有业务授权。
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
        InvalidCitizenCid,
        CitizenIdentityMismatch,
        /// 运行期(Operation)公权管理员缺少必填要素(cid/姓/名)。
        IncompleteAdminFields,
        WalletLegacyUnavailable,
        WalletLegacyMismatch,
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
