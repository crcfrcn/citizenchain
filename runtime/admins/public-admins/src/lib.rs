#![cfg_attr(not(feature = "std"), no_std)]
//! 公权机构管理员账户集合模块。
//!
//! 机构唯一身份是 CID。本模块只保存 `AdminAccounts[cid_number] -> admins`；
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

use admin_primitives::{
    can_store_public_admin_code, Admin, AdminAccountKind, AdminCidNumber, ChainPhaseCheck,
    CitizenIdentityBindingQuery, InstitutionAdminLifecycle, InstitutionAdminQuery,
    InstitutionAdmins,
};
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

        /// 公权管理员非空公民 CID 必须匹配 citizen-identity 的唯一账户绑定。
        type CitizenIdentityBinding: CitizenIdentityBindingQuery<Self::AccountId>;

        /// 运行期强制门控(由 genesis-pallet 相位注入);仅 Operation 期强制四要素完整。
        type ChainPhase: admin_primitives::ChainPhaseCheck;
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
    }

    impl<T: Config> Pallet<T> {
        fn ensure_unique_admins(admins: &[Admin<T::AccountId>]) -> DispatchResult {
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

        fn validate_admin_set(
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
            match admin_primitives::expected_fixed_governance_admins_len(
                institution_code,
                cid_number,
            ) {
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
                let req = admin_primitives::required_admin_elements(
                    AdminAccountKind::PublicInstitution,
                    false,
                );
                for admin in admins {
                    ensure!(admin.satisfies(req), Error::<T>::IncompleteAdminFields);
                }
            }
            Self::ensure_unique_admins(admins)
        }

        fn bound_cid(cid_number: Vec<u8>) -> Result<AdminCidNumber, DispatchError> {
            cid_number
                .try_into()
                .map_err(|_| Error::<T>::InvalidInstitution.into())
        }

        fn bounded_cid(cid_number: &[u8]) -> Option<AdminCidNumber> {
            cid_number.to_vec().try_into().ok()
        }

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

        pub(crate) fn get_institution_admins(
            institution_code: InstitutionCode,
            cid_number: &[u8],
        ) -> Option<InstitutionAdminsOf<T>> {
            let cid_number = Self::bounded_cid(cid_number)?;
            let value = AdminAccounts::<T>::get(cid_number)?;
            if value.institution_code != institution_code
                || !can_store_public_admin_code(&institution_code)
            {
                return None;
            }
            Some(value)
        }
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
        value.admins.iter().find_map(|admin| {
            let matched = if operation && !admin.cid_number.is_empty() {
                // 运行期 + 有 CID：身份锚定，只认该 CID 当前绑定的钱包（无 account_id 回退，旧钱包掉权）。
                T::CitizenIdentityBinding::matches_citizen_account(
                    admin.cid_number.as_slice(),
                    caller,
                )
            } else {
                // 创世期，或无 CID 管理员：钱包锚定（现状）。
                &admin.account_id == caller
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
