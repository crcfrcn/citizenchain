#![cfg_attr(not(feature = "std"), no_std)]
//! 私权机构管理员账户集合模块。
//!
//! 机构唯一身份是 CID。本模块只保存 `AdminAccounts[cid_number] -> admins`；
//! 岗位和任职归 entity，投票阈值归 votebase，机构账户不参与管理员寻址。

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
    can_store_private_admin_code, Admin, AdminAccountKind, AdminCidNumber, ChainPhaseCheck,
    CitizenIdentityBindingQuery, InstitutionAdminLifecycle, InstitutionAdminQuery,
    InstitutionAdmins,
};
use entitybase::InstitutionLegalRepresentativeQuery as _;
use votebase::types::InstitutionCode;

pub use pallet::*;

/// 正式创世只接受统一管理员记录，不保留旧纯账户或单姓名存储迁移。
const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    #[pallet::config]
    pub trait Config: frame_system::Config + votebase::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        #[pallet::constant]
        type MaxAdminsPerInstitution: Get<u32>;

        /// 运行期强制门控(由 genesis 相位注入)。
        type ChainPhase: ChainPhaseCheck;

        /// 私权管理员非空公民 CID 与账户绑定查询（换绑钱包不掉权靠它解析）。
        type CitizenIdentityBinding: CitizenIdentityBindingQuery<Self::AccountId>;

        /// 法定代表人身份记录查询。分层强制下私权只有 LR 岗四要素完整，且强制落点是
        /// `InstitutionInfo.legal_representative` 而非本名册；名册 cid 为空时由此回落，
        /// 使 LR 同样享受「换绑不掉权」。
        type LegalRepresentativeQuery: entitybase::InstitutionLegalRepresentativeQuery<
            Self::AccountId,
        >;
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
    }
}

// 校验、写入与查询独立组织，不改变FRAME存储、事件或外部接口。
mod lifecycle;
mod query;
mod validation;

#[cfg(test)]
#[path = "../tests/mod.rs"]
mod tests;
