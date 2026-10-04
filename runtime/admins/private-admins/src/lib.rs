#![cfg_attr(not(feature = "std"), no_std)]
//! 私权机构管理员账户集合模块。
//!
//! 机构唯一身份是 CID。本模块只保存 `AdminAccounts[cid_number] -> admins`；
//! 岗位和任职归 entity，投票阈值归 votingengine，机构账户不参与管理员寻址。

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

use admin_primitives::{
    can_store_private_admin_code, Admin, AdminAccountKind, AdminCidNumber, ChainPhaseCheck,
    CitizenIdentityBindingQuery, InstitutionAdminLifecycle, InstitutionAdminQuery,
    InstitutionAdmins,
};
use entity_primitives::InstitutionLegalRepresentativeQuery as _;
use votingengine::types::InstitutionCode;

pub use pallet::*;

/// 正式创世只接受统一管理员记录，不保留旧纯账户或单姓名存储迁移。
const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    #[pallet::config]
    pub trait Config: frame_system::Config + votingengine::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        #[pallet::constant]
        type MaxAdminsPerInstitution: Get<u32>;

        /// 运行期强制门控(由 genesis-pallet 相位注入)。
        type ChainPhase: ChainPhaseCheck;

        /// 私权管理员非空公民 CID 与账户绑定查询（换绑钱包不掉权靠它解析）。
        type CitizenIdentityBinding: CitizenIdentityBindingQuery<Self::AccountId>;

        /// 法定代表人身份记录查询。分层强制下私权只有 LR 岗四要素完整，且强制落点是
        /// `InstitutionInfo.legal_representative` 而非本名册；名册 cid 为空时由此回落，
        /// 使 LR 同样享受「换绑不掉权」。
        type LegalRepresentativeQuery: entity_primitives::InstitutionLegalRepresentativeQuery<
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

    impl<T: Config> Pallet<T> {
        fn bound_cid(cid_number: Vec<u8>) -> Result<AdminCidNumber, DispatchError> {
            cid_number
                .try_into()
                .map_err(|_| Error::<T>::InvalidInstitution.into())
        }

        fn bounded_cid(cid_number: &[u8]) -> Option<AdminCidNumber> {
            cid_number.to_vec().try_into().ok()
        }

        fn validate_admin_set(
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

        pub(crate) fn do_set_institution_admins(
            cid_number: Vec<u8>,
            institution_code: InstitutionCode,
            kind: AdminAccountKind,
            admins: Vec<Admin<T::AccountId>>,
        ) -> DispatchResult {
            let cid_number = Self::bound_cid(cid_number)?;
            let admins = admins
                .into_iter()
                .map(Admin::normalize_names)
                .collect::<Vec<_>>();
            Self::validate_admin_set(kind, institution_code, &admins)?;
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
                        false
                    }
                    None => true,
                };
                AdminAccounts::<T>::insert(
                    &cid_number,
                    InstitutionAdmins {
                        institution_code,
                        admins: bounded,
                    },
                );
                Self::deposit_event(Event::<T>::InstitutionAdminsSet {
                    cid_number,
                    institution_code,
                    admins_len,
                    created,
                });
                TransactionOutcome::Commit(Ok(()))
            })
        }

        /// 创世专用写入口：复用运行期完整管理员校验，机构阈值由 entity 独立写入。
        pub fn store_genesis_institution_admins(
            cid_number: Vec<u8>,
            institution_code: InstitutionCode,
            admins: Vec<Admin<T::AccountId>>,
        ) -> DispatchResult {
            Self::do_set_institution_admins(
                cid_number,
                institution_code,
                AdminAccountKind::PrivateInstitution,
                admins,
            )
        }

        pub(crate) fn get_institution_admins(
            institution_code: InstitutionCode,
            cid_number: &[u8],
        ) -> Option<InstitutionAdminsOf<T>> {
            let cid_number = Self::bounded_cid(cid_number)?;
            let value = AdminAccounts::<T>::get(cid_number)?;
            if value.institution_code != institution_code
                || !can_store_private_admin_code(&institution_code)
            {
                return None;
            }
            Some(value)
        }
    }
}

impl<T: pallet::Config> InstitutionAdminLifecycle<T::AccountId> for pallet::Pallet<T> {
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

impl<T: pallet::Config> InstitutionAdminQuery<T::AccountId> for pallet::Pallet<T> {
    fn institution_admins_exist(institution_code: InstitutionCode, cid_number: &[u8]) -> bool {
        Self::get_institution_admins(institution_code, cid_number).is_some()
    }

    fn is_institution_admin(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        who: &T::AccountId,
    ) -> bool {
        // 名册成员判断按 account_id（供枚举/投票快照使用）。调用者门的 CID 解析走
        // `resolve_admin_account`，二者语义不同，不可互相替换。
        Self::get_institution_admins(institution_code, cid_number)
            .map(|value| value.admins.iter().any(|admin| &admin.account_id == who))
            .unwrap_or(false)
    }

    fn resolve_admin_account(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        caller: &T::AccountId,
    ) -> Option<T::AccountId> {
        let value = Self::get_institution_admins(institution_code, cid_number)?;
        let operation = T::ChainPhase::is_operation();
        // 运行期才需要 LR 回落：私权 LR 的四要素强制在 `InstitutionInfo.legal_representative`
        // 身份记录上，其名册条目的 cid 可能为空；回落后 LR 同样按 CID 身份锚定。
        let (lr_account, lr_cid) = if operation {
            (
                T::LegalRepresentativeQuery::legal_representative(cid_number),
                T::LegalRepresentativeQuery::legal_representative_cid(cid_number),
            )
        } else {
            (None, None)
        };
        value.admins.iter().find_map(|admin| {
            // 该管理员的有效 CID：名册优先；名册为空且他是本机构 LR → 用 LR 身份记录的 cid。
            let effective_cid: Option<&[u8]> = if !admin.cid_number.is_empty() {
                Some(admin.cid_number.as_slice())
            } else if lr_account.as_ref() == Some(&admin.account_id) {
                lr_cid.as_deref()
            } else {
                None
            };
            let matched = match effective_cid {
                // 运行期 + 有有效 CID：身份锚定，只认该 CID 当前绑定的钱包
                //（无 account_id 回退 → 换绑后旧钱包掉权）。
                Some(cid_bytes) if operation => {
                    T::CitizenIdentityBinding::matches_citizen_account(cid_bytes, caller)
                }
                // 创世期，或无 CID 管理员（私权非 LR / 个人多签）：钱包锚定（现状）。
                _ => &admin.account_id == caller,
            };
            matched.then(|| admin.account_id.clone())
        })
    }

    fn institution_admins(
        institution_code: InstitutionCode,
        cid_number: &[u8],
    ) -> Option<Vec<T::AccountId>> {
        Self::get_institution_admins(institution_code, cid_number).map(|value| {
            value
                .admins
                .into_inner()
                .into_iter()
                .map(|admin| admin.account_id)
                .collect()
        })
    }

    fn institution_admins_len(institution_code: InstitutionCode, cid_number: &[u8]) -> Option<u32> {
        Self::get_institution_admins(institution_code, cid_number)
            .map(|value| value.admins.len() as u32)
    }
}

#[cfg(test)]
mod tests;
