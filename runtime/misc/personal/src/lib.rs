//! 管理个人多签账户的生命周期、成员、成员变更和投票终态执行。

#![cfg_attr(not(feature = "std"), no_std)]

//! 个人多签不归属机构CID，地址仍由创建钱包与账户名派生。
//! 账户和成员分别保存在本模块的原结构表中，普通动态阈值仍归投票引擎。
//! 创建、注销使用 `per-mgmt`，成员变更使用 `per-admin`；两种既有提案编码不变。

/// 模块标识前缀(8 字节,与机构生命周期模块 tag 长度对齐)。
/// personal / citizenwallet / citizenapp 三方解码必须保持一致。
pub const MODULE_TAG: &[u8] = b"per-mgmt";

/// 成员变更的既有提案owner；载荷仍是裸AdminSetChangeAction。
pub const ADMIN_MODULE_TAG: &[u8] = b"per-admin";

/// 提案动作类型常量,独立命名空间(从 0 起编号),与 publicentity/privateentity 的 ACTION 互不干扰。
pub const ACTION_CREATE: u8 = 0;
pub const ACTION_CLOSE: u8 = 1;

pub use pallet::*;

pub mod close;
pub mod create;
pub mod execute;
mod members;
pub mod traits;
pub mod types;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
#[path = "../tests/benchmarks.rs"]
mod benchmarks;

#[cfg(test)]
#[path = "../tests/mod.rs"]
mod tests;

pub use traits::PersonalMultisigQuery;
pub use types::*;

use codec::{Decode, Encode};
use engingcore::{
    types::{InstitutionCode, PMUL},
    InternalVoteEngine, InternalVoteResultCallback, ProposalExecutionOutcome,
    PROPOSAL_KIND_INTERNAL, STAGE_INTERNAL, STATUS_EXECUTION_FAILED, STATUS_PASSED,
    STATUS_REJECTED, STATUS_VOTING,
};
use frame_support::{
    ensure,
    pallet_prelude::*,
    storage::{with_transaction, TransactionOutcome},
    traits::{Currency, ReservableCurrency},
    BoundedVec,
};
use frame_system::pallet_prelude::*;
use sp_std::prelude::*;

pub(crate) type BalanceOf<T> =
    <<T as pallet::Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    #[pallet::config]
    pub trait Config: frame_system::Config + engingcore::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

        /// 内部投票引擎
        type InternalVoteEngine: engingcore::InternalVoteEngine<Self::AccountId>;

        type AccountValidator: primitives::multisig::AccountValidator<Self::AccountId>;
        type ReservedAccountChecker: primitives::multisig::ReservedAccountGuard<Self::AccountId>;
        type ProtectedSourceChecker: primitives::multisig::ProtectedSourceChecker<Self::AccountId>;
        type InstitutionAsset: primitives::institution_asset::InstitutionAsset<Self::AccountId>;

        /// 个人多签创建入金和注销转出的链上费统一执行器。
        type OnchainFeeCharger: primitives::fee_policy::OnchainFeeCharger<
            Self::AccountId,
            BalanceOf<Self>,
        >;

        /// 个人多签账户名称最大字节数
        #[pallet::constant]
        type MaxAccountNameLength: Get<u32>;

        /// 单个个人多签账户管理员最大数量上限。
        #[pallet::constant]
        type MaxPersonalAccountAdmins: Get<u32>;

        /// 创建时最低入金(默认 111 分 = 1.11 元)
        #[pallet::constant]
        type MinCreateAmount: Get<BalanceOf<Self>>;

        type WeightInfo: crate::weights::WeightInfo;
    }

    pub type AdminsOf<T> = BoundedVec<
        Admin<<T as frame_system::Config>::AccountId>,
        <T as Config>::MaxPersonalAccountAdmins,
    >;

    pub type AdminAccountOf<T> =
        AdminAccount<AdminsOf<T>, <T as frame_system::Config>::AccountId, BlockNumberFor<T>>;

    pub type PersonalAccountOf<T> = PersonalAccount<
        <T as frame_system::Config>::AccountId,
        AccountNameOf<T>,
        BlockNumberFor<T>,
    >;

    pub type AccountNameOf<T> = BoundedVec<u8, <T as Config>::MaxAccountNameLength>;

    pub type PersonalCreateActionOf<T> =
        PersonalCreateAction<<T as frame_system::Config>::AccountId, BalanceOf<T>>;

    pub type PersonalCloseActionOf<T> = PersonalCloseAction<<T as frame_system::Config>::AccountId>;

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    /// 个人多签账户配置。key 为 personal_account_id。
    ///
    /// 本表统一保存 creator_account_id/account_name/created_at/status。
    /// 成员集合、成员数量只从本模块 AdminAccounts 读取；
    /// 普通动态阈值只允许从投票引擎 internalvote 读取。
    #[pallet::storage]
    #[pallet::getter(fn personal_accounts)]
    pub type PersonalAccounts<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, PersonalAccountOf<T>, OptionQuery>;

    /// 正在投票中的个人多签创建提案,用于通过/拒绝时处理 reserve 资金。
    ///
    /// 资金模型: 发起时 reserve(amount + fee), 通过后 unreserve + transfer + withdraw fee,
    /// 否决/终态失败 unreserve。
    #[pallet::storage]
    #[pallet::getter(fn pending_personal_create)]
    pub type PendingPersonalCreate<T: Config> =
        StorageMap<_, Blake2_128Concat, u128, PersonalCreateActionOf<T>, OptionQuery>;

    /// 个人多签账户当前进行中的关闭提案 ID(防止并发注销提案)。
    /// 发起 propose_close 时写入,execute_close 成功或执行失败后清除。
    #[pallet::storage]
    #[pallet::getter(fn pending_close_proposal)]
    pub type PendingCloseProposal<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u128, OptionQuery>;

    /// 个人多签成员集合，沿用原记录字段、状态编码及按钱包授权规则。
    #[pallet::storage]
    #[pallet::getter(fn admin_account_of)]
    pub type AdminAccounts<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, AdminAccountOf<T>, OptionQuery>;

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

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {}
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 个人多签账户创建提案已发起(pending 状态预写入)。
        /// citizenapp 扫描此事件后引导其他管理员到投票引擎统一入口 `internal_vote` 投票。
        PersonalCreateProposed {
            proposal_id: u128,
            account_id: T::AccountId,
            proposer_account_id: T::AccountId,
            account_name: AccountNameOf<T>,
            admins: AdminsOf<T>,
            admins_len: u32,
            threshold: u32,
            amount: BalanceOf<T>,
            fee: BalanceOf<T>,
            expires_at: BlockNumberFor<T>,
        },
        /// 个人多签账户创建成功(投票通过,入金完成,状态变为 Active)。
        PersonalCreated {
            proposal_id: u128,
            account_id: T::AccountId,
            creator_account_id: T::AccountId,
            admins_len: u32,
            threshold: u32,
            amount: BalanceOf<T>,
            fee: BalanceOf<T>,
        },
        /// 创建提案投票通过但执行失败。
        CreateExecutionFailed {
            proposal_id: u128,
            account_id: T::AccountId,
        },
        /// 创建提案最终被拒绝(投票引擎返回 STATUS_REJECTED 后清理 Pending)。
        PersonalCreateRejected {
            proposal_id: u128,
            account_id: T::AccountId,
        },
        /// 关闭个人多签账户提案已发起。
        PersonalCloseProposed {
            proposal_id: u128,
            account_id: T::AccountId,
            proposer_account_id: T::AccountId,
            beneficiary_account_id: T::AccountId,
        },
        /// 个人多签账户注销成功(投票通过,余额转出,PersonalAccounts 删除)。
        PersonalClosed {
            proposal_id: u128,
            account_id: T::AccountId,
            beneficiary_account_id: T::AccountId,
            admins_len: u32,
            threshold: u32,
            amount: BalanceOf<T>,
            fee: BalanceOf<T>,
        },
        /// 关闭提案投票通过但执行失败。
        CloseExecutionFailed {
            proposal_id: u128,
            account_id: T::AccountId,
        },

        /// 已发起个人多签管理员集合变更提案。
        AdminSetChangeProposed {
            proposal_id: u128,
            account: T::AccountId,
            proposer: T::AccountId,
            old_admins_len: u32,
            new_admins_len: u32,
            new_threshold: u32,
        },
        /// 个人多签管理员集合已完成执行。
        AdminSetChanged {
            proposal_id: u128,
            account: T::AccountId,
            admins_len: u32,
            threshold: u32,
        },
        /// 个人多签管理员集合提案通过后执行失败。
        AdminSetChangeExecutionFailed { proposal_id: u128 },
        /// 个人多签管理员账户已写入 Pending。
        AdminAccountPendingCreated {
            account: T::AccountId,
            creator_account_id: T::AccountId,
            admins_len: u32,
        },
        /// 个人多签管理员账户已激活。
        AdminAccountActivated { account: T::AccountId },
        /// Pending 个人多签管理员账户已清理。
        AdminAccountPendingRemoved { account: T::AccountId },
        /// 个人多签管理员账户已关闭。
        AdminAccountClosed { account: T::AccountId },
    }

    #[pallet::error]
    pub enum Error<T> {
        IncompleteParameters,
        InvalidAccount,
        AccountReserved,
        DuplicateAdmin,
        InvalidFamilyName,
        InvalidGivenName,
        InvalidThreshold,
        InsufficientAmount,
        CreateAmountBelowMinimum,
        CloseBalanceBelowMinimum,
        PermissionDenied,
        InvalidAdminsLen,
        AdminsLenMismatch,
        PersonalNotFound,
        PersonalNotActive,
        InvalidBeneficiary,
        ProtectedSource,
        DerivedAccountDecodeFailed,
        ReservedBalanceRemaining,
        VoteEngineError,
        ProposalActionNotFound,
        TransferFailed,
        EmptyPersonalName,
        PersonalAlreadyExists,
        CloseAlreadyPending,
        ReserveFailed,
        ReserveReleaseFailed,
        FeeWithdrawFailed,
        CloseTransferBelowED,
        /// propose_close 校验:仅个人多签账户可走本入口(非个人地址转 publicentity/privateentity)。
        NotPersonalAccount,
        // 并入成员错误只追加，原生命周期错误编号不变。
        AdminSetUnchanged,
        InvalidLifecycleScope,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 发起"创建个人多签账户"提案(无需 CID 注册)。
        ///
        /// 地址由 `creator_account_id + account_name` 派生，统一调用
        /// `primitives::account_derive::AccountKind::Personal { creator_account_id, account_name }.derive(ss58)`。
        ///
        /// 投票通过后由 `InternalVoteExecutor` 自动执行入金 + 激活。
        #[pallet::call_index(0)]
        #[pallet::weight(<T as pallet::Config>::WeightInfo::propose_create(admins.len() as u32))]
        pub fn propose_create(
            origin: OriginFor<T>,
            account_name: AccountNameOf<T>,
            admins: AdminsOf<T>,
            regular_threshold: u32,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            crate::create::do_propose_create::<T>(
                who,
                account_name,
                admins,
                regular_threshold,
                amount,
            )
        }

        /// 发起"关闭个人多签账户"提案。
        ///
        /// 仅接受个人多签账户(`PersonalAccounts.contains_key` 命中);机构多签走 publicentity/privateentity。
        #[pallet::call_index(1)]
        #[pallet::weight(<T as pallet::Config>::WeightInfo::propose_close())]
        pub fn propose_close(
            origin: OriginFor<T>,
            account_id: T::AccountId,
            beneficiary_account_id: T::AccountId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            crate::close::do_propose_close::<T>(who, account_id, beneficiary_account_id)
        }

        // call_index(2) 已永久废弃：拒绝和执行失败清理由 engingcore 终态回调完成。

        /// 发起成员集合变更，参数与原个人成员入口一致。
        #[pallet::call_index(3)]
        #[pallet::weight(<T as Config>::WeightInfo::propose_admin_set_change(<T as Config>::MaxPersonalAccountAdmins::get()))]
        pub fn propose_admin_set_change(
            origin: OriginFor<T>,
            institution_code: InstitutionCode,
            account: T::AccountId,
            admins: AdminsOf<T>,
            new_threshold: u32,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(institution_code == PMUL, Error::<T>::NotPersonalAccount);
            let admins = Self::normalize_admins(admins);
            let current =
                AdminAccounts::<T>::get(account.clone()).ok_or(Error::<T>::PersonalNotFound)?;
            ensure!(
                current.status == AdminAccountStatus::Active
                    && current.kind == AdminAccountKind::PersonalMultisig
                    && current.institution_code == PMUL,
                Error::<T>::NotPersonalAccount
            );
            let current_admins = current.admins.clone().into_inner();
            ensure!(
                current_admins.iter().any(|admin| admin.account_id == who),
                Error::<T>::PermissionDenied
            );
            Self::validate_admin_set_for_change(&admins, new_threshold)?;
            let current_threshold =
                T::InternalVoteEngine::active_personal_threshold(account.clone())
                    .ok_or(Error::<T>::InvalidThreshold)?;
            ensure!(
                !Self::same_admin_set(current_admins.as_slice(), admins.as_slice())
                    || current_threshold != new_threshold,
                Error::<T>::AdminSetUnchanged
            );

            with_transaction(|| {
                let action = AdminSetChangeAction {
                    personal_account_id: account.clone(),
                    admins: admins.clone(),
                    new_threshold,
                };
                let proposal_id =
                    match T::InternalVoteEngine::create_personal_admin_change_proposal_with_data(
                        who.clone(),
                        account.clone(),
                        admins.len() as u32,
                        new_threshold,
                        crate::ADMIN_MODULE_TAG,
                        action.encode(),
                    ) {
                        Ok(proposal_id) => proposal_id,
                        Err(err) => return TransactionOutcome::Rollback(Err(err)),
                    };
                Self::deposit_event(Event::<T>::AdminSetChangeProposed {
                    proposal_id,
                    account,
                    proposer: who,
                    old_admins_len: current_admins.len() as u32,
                    new_admins_len: admins.len() as u32,
                    new_threshold,
                });
                TransactionOutcome::Commit(Ok(()))
            })
        }
    }

    impl<T: Config> Pallet<T> {
        /// 派生个人多签账户。
        ///
        /// 地址只依赖 creator_account_id 与 account_name,与管理员列表无关,
        /// 所以未来换管理员地址不变。
        pub fn derive_personal_account(
            creator_account_id: &T::AccountId,
            account_name: &[u8],
        ) -> Result<T::AccountId, DispatchError> {
            // creator_account_id (AccountId32) 编码即 32 字节原始 pubkey;account_derive 的
            // Personal payload = creator_account_id(32B) || account_name,与历史拼装逐字节一致。
            let encoded = creator_account_id.encode();
            ensure!(encoded.len() >= 32, Error::<T>::DerivedAccountDecodeFailed);
            let mut creator_32 = [0u8; 32];
            creator_32.copy_from_slice(&encoded[..32]);
            let digest = primitives::account_derive::AccountKind::Personal {
                creator_account_id: &creator_32,
                account_name,
            }
            .derive(<T as frame_system::Config>::SS58Prefix::get());
            T::AccountId::decode(&mut &digest[..])
                .map_err(|_| Error::<T>::DerivedAccountDecodeFailed.into())
        }

        /// 校验管理员集合和用户输入的普通业务动态阈值。
        pub(crate) fn ensure_admin_config(
            who: &T::AccountId,
            admins: &AdminsOf<T>,
            regular_threshold: u32,
        ) -> Result<u32, DispatchError> {
            let admins_len = admins.len() as u32;
            ensure!(admins_len >= 2, Error::<T>::InvalidAdminsLen);
            ensure!(
                admins_len <= <T as Config>::MaxPersonalAccountAdmins::get(),
                Error::<T>::InvalidAdminsLen
            );
            T::InternalVoteEngine::validate_personal_threshold(admins_len, regular_threshold)
                .map_err(|_| Error::<T>::InvalidThreshold)?;
            Self::ensure_unique_admins(admins)?;
            ensure!(
                admins.iter().any(|admin| &admin.account_id == who),
                Error::<T>::PermissionDenied
            );
            Ok(regular_threshold)
        }

        /// 将缺失的管理员姓、名规范化为“管理”“员”，授权字段保持账户不变。
        pub(crate) fn normalize_admins(admins: AdminsOf<T>) -> AdminsOf<T> {
            AdminsOf::<T>::truncate_from(
                admins
                    .into_inner()
                    .into_iter()
                    .map(Admin::normalize_names)
                    .collect(),
            )
        }

        pub(crate) fn ensure_unique_admins(admins: &AdminsOf<T>) -> Result<(), DispatchError> {
            use sp_std::collections::btree_set::BTreeSet;
            let mut seen = BTreeSet::new();
            for admin in admins.iter() {
                ensure!(!admin.family_name.is_empty(), Error::<T>::InvalidFamilyName);
                ensure!(!admin.given_name.is_empty(), Error::<T>::InvalidGivenName);
                ensure!(
                    seen.insert(admin.account_id.clone()),
                    Error::<T>::DuplicateAdmin
                );
            }
            Ok(())
        }

        /// 校验发起人 free 余额覆盖 amount + fee + ED,返回 (reserve_total = amount + fee, fee)。
        pub(crate) fn ensure_proposer_can_afford(
            who: &T::AccountId,
            amount: BalanceOf<T>,
        ) -> Result<(BalanceOf<T>, BalanceOf<T>), DispatchError> {
            use sp_runtime::{traits::CheckedAdd, SaturatedConversion};
            let amount_u128: u128 = amount.saturated_into();
            let fee_u128 = primitives::fee_policy::calculate_onchain_fee(amount_u128);
            let fee: BalanceOf<T> = fee_u128.saturated_into();
            let reserve_total = amount
                .checked_add(&fee)
                .ok_or(Error::<T>::InsufficientAmount)?;
            let ed = T::Currency::minimum_balance();
            let required = reserve_total
                .checked_add(&ed)
                .ok_or(Error::<T>::InsufficientAmount)?;
            ensure!(
                T::Currency::free_balance(who) >= required,
                Error::<T>::InsufficientAmount
            );
            Ok((reserve_total, fee))
        }

        pub(crate) fn ensure_lifecycle_proposal(
            proposal_id: u128,
            module_tag: &[u8],
            account_id: T::AccountId,
            expected_status: u8,
            require_callback_scope: bool,
        ) -> DispatchResult {
            ensure!(
                engingcore::Pallet::<T>::is_proposal_owner(proposal_id, module_tag),
                Error::<T>::ProposalActionNotFound
            );
            let proposal = engingcore::Pallet::<T>::proposals(proposal_id)
                .ok_or(Error::<T>::ProposalActionNotFound)?;
            ensure!(
                proposal.kind == PROPOSAL_KIND_INTERNAL,
                Error::<T>::ProposalActionNotFound
            );
            ensure!(
                proposal.stage == STAGE_INTERNAL,
                Error::<T>::ProposalActionNotFound
            );
            ensure!(
                proposal.execution_account_id == Some(account_id)
                    && proposal.actor_cid_number.is_none(),
                Error::<T>::ProposalActionNotFound
            );
            ensure!(
                proposal.internal_code == Some(engingcore::types::PMUL),
                Error::<T>::ProposalActionNotFound
            );
            ensure!(
                proposal.status == expected_status,
                Error::<T>::ProposalActionNotFound
            );
            if require_callback_scope {
                ensure!(
                    engingcore::Pallet::<T>::is_callback_execution_scope(proposal_id),
                    Error::<T>::ProposalActionNotFound
                );
            }
            Ok(())
        }

        pub(crate) fn create_pending_admin_account_for_proposal(
            proposal_id: u128,
            account_id: T::AccountId,
            kind: AdminAccountKind,
            admins: &AdminsOf<T>,
            creator_account_id: &T::AccountId,
        ) -> DispatchResult {
            Self::ensure_lifecycle_proposal(
                proposal_id,
                crate::MODULE_TAG,
                account_id.clone(),
                STATUS_VOTING,
                false,
            )?;
            Self::do_create_pending_admin_account(
                account_id,
                kind,
                admins.iter().cloned().collect(),
                creator_account_id.clone(),
            )
        }

        pub(crate) fn activate_admin_account(
            proposal_id: u128,
            account_id: T::AccountId,
        ) -> DispatchResult {
            Self::ensure_lifecycle_proposal(
                proposal_id,
                crate::MODULE_TAG,
                account_id.clone(),
                STATUS_PASSED,
                true,
            )?;
            Self::do_activate_admin_account(account_id)
        }

        pub(crate) fn remove_pending_admin_account(
            proposal_id: u128,
            account_id: T::AccountId,
        ) -> DispatchResult {
            let proposal = engingcore::Pallet::<T>::proposals(proposal_id)
                .ok_or(Error::<T>::ProposalActionNotFound)?;
            ensure!(
                matches!(proposal.status, STATUS_REJECTED | STATUS_EXECUTION_FAILED),
                Error::<T>::ProposalActionNotFound
            );
            Self::ensure_lifecycle_proposal(
                proposal_id,
                crate::MODULE_TAG,
                account_id.clone(),
                proposal.status,
                false,
            )?;
            Self::do_remove_pending_admin_account(account_id)
        }

        pub(crate) fn close_admin_account(
            proposal_id: u128,
            account_id: T::AccountId,
        ) -> DispatchResult {
            Self::ensure_lifecycle_proposal(
                proposal_id,
                crate::MODULE_TAG,
                account_id.clone(),
                STATUS_PASSED,
                true,
            )?;
            Self::do_close_admin_account(account_id)
        }

        /// 统一解码本 pallet 的 ProposalData 前缀,避免回调和手动 cleanup 各写一套边界判断。
        pub(crate) fn decode_module_action(raw: &[u8]) -> Result<(u8, &[u8]), DispatchError> {
            let tag = crate::MODULE_TAG;
            ensure!(
                raw.len() > tag.len() && &raw[..tag.len()] == tag,
                Error::<T>::ProposalActionNotFound
            );
            Ok((raw[tag.len()], &raw[tag.len() + 1..]))
        }
    }
}

// ──── 投票终态回调:个人多签创建/关闭提案的执行落地 ────
//
// 投票统一由投票引擎承担,提案通过(或否决)经
// `engingcore::InternalVoteResultCallback` tuple 广播回来。
// 本 Executor 按 `MODULE_TAG + ACTION_*` 前缀认领本模块提案,
// approved=true 分派 execute_create / execute_close;approved=false 清理 Pending 存储。
// ──── PersonalMultisigQuery 实现:对 multisig-transfer / runtime config 暴露查询 ────

impl<T: pallet::Config> traits::PersonalMultisigQuery<T::AccountId> for pallet::Pallet<T> {
    fn lookup_admin_config(
        addr: &T::AccountId,
    ) -> Option<primitives::multisig::MultisigConfigSnapshot<T::AccountId>> {
        let account_id = pallet::PersonalAccounts::<T>::get(addr)?;
        if account_id.status != types::PersonalStatus::Active {
            return None;
        }
        let account_id = addr.clone();
        let institution_code = engingcore::types::PMUL;
        let admins =
            pallet::Pallet::<T>::active_account_admins(institution_code, account_id.clone())?;
        let admins_len =
            pallet::Pallet::<T>::active_account_admins_len(institution_code, account_id.clone())?;
        let threshold =
            <T as Config>::InternalVoteEngine::active_personal_threshold(account_id.clone())?;
        Some(primitives::multisig::MultisigConfigSnapshot {
            admins,
            admins_len,
            threshold,
        })
    }

    fn is_active(addr: &T::AccountId) -> bool {
        matches!(
            pallet::PersonalAccounts::<T>::get(addr).map(|a| a.status),
            Some(types::PersonalStatus::Active)
        )
    }
}

pub struct InternalVoteExecutor<T>(core::marker::PhantomData<T>);

impl<T: pallet::Config> InternalVoteResultCallback for InternalVoteExecutor<T> {
    fn on_internal_vote_finalized(
        proposal_id: u128,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, sp_runtime::DispatchError> {
        use frame_support::storage::{with_transaction, TransactionOutcome};
        // 同一pallet分别执行两类原始业务载荷，不增加新提案格式。
        if engingcore::Pallet::<T>::is_proposal_owner(proposal_id, crate::ADMIN_MODULE_TAG) {
            return crate::members::execute_admin_change::<T>(proposal_id, approved);
        }
        if !engingcore::Pallet::<T>::is_proposal_owner(proposal_id, crate::MODULE_TAG) {
            return Ok(ProposalExecutionOutcome::Ignored);
        }
        let raw = engingcore::Pallet::<T>::get_proposal_data(proposal_id)
            .ok_or(pallet::Error::<T>::ProposalActionNotFound)?;

        ensure!(
            raw.starts_with(crate::MODULE_TAG),
            pallet::Error::<T>::ProposalActionNotFound
        );
        let (action_byte, payload) = pallet::Pallet::<T>::decode_module_action(&raw)?;

        if approved {
            match action_byte {
                ACTION_CREATE => {
                    let action = pallet::PersonalCreateActionOf::<T>::decode(&mut &payload[..])
                        .map_err(|_| pallet::Error::<T>::ProposalActionNotFound)?;
                    let outcome = with_transaction(
                        || -> TransactionOutcome<Result<ProposalExecutionOutcome, sp_runtime::DispatchError>> {
                            match crate::execute::execute_create_with_finalizer::<T>(
                                proposal_id,
                                &action,
                            ) {
                                Ok(()) => TransactionOutcome::Commit(Ok(ProposalExecutionOutcome::Executed)),
                                Err(_) => TransactionOutcome::Rollback(Ok(
                                    ProposalExecutionOutcome::FatalFailed,
                                )),
                            }
                        },
                    )?;
                    Ok(outcome)
                }
                ACTION_CLOSE => {
                    let action = pallet::PersonalCloseActionOf::<T>::decode(&mut &payload[..])
                        .map_err(|_| pallet::Error::<T>::ProposalActionNotFound)?;
                    let outcome = with_transaction(
                        || -> TransactionOutcome<Result<ProposalExecutionOutcome, sp_runtime::DispatchError>> {
                            match crate::execute::execute_close_with_finalizer::<T>(
                                proposal_id,
                                &action,
                            ) {
                                Ok(()) => TransactionOutcome::Commit(Ok(ProposalExecutionOutcome::Executed)),
                                Err(_) => TransactionOutcome::Rollback(Ok(
                                    ProposalExecutionOutcome::FatalFailed,
                                )),
                            }
                        },
                    )?;
                    Ok(outcome)
                }
                _ => Ok(ProposalExecutionOutcome::Ignored),
            }
        } else {
            // 否决路径:清理 Pending 存储 + unreserve 资金。
            match action_byte {
                ACTION_CREATE => {
                    let action = pallet::PersonalCreateActionOf::<T>::decode(&mut &payload[..])
                        .map_err(|_| pallet::Error::<T>::ProposalActionNotFound)?;
                    crate::execute::cleanup_pending_create::<T>(proposal_id, &action, true)?;
                    Ok(ProposalExecutionOutcome::Executed)
                }
                ACTION_CLOSE => {
                    let action = pallet::PersonalCloseActionOf::<T>::decode(&mut &payload[..])
                        .map_err(|_| pallet::Error::<T>::ProposalActionNotFound)?;
                    pallet::PendingCloseProposal::<T>::remove(&action.account_id);
                    Ok(ProposalExecutionOutcome::Executed)
                }
                _ => Ok(ProposalExecutionOutcome::Ignored),
            }
        }
    }

    fn execution_weight() -> frame_support::weights::Weight {
        use crate::weights::WeightInfo;
        use frame_support::traits::Get;
        <T as crate::pallet::Config>::WeightInfo::propose_create(
            <T as crate::pallet::Config>::MaxPersonalAccountAdmins::get(),
        )
        .max(<T as crate::pallet::Config>::WeightInfo::propose_close())
        .max(
            <T as crate::pallet::Config>::WeightInfo::propose_admin_set_change(
                <T as crate::pallet::Config>::MaxPersonalAccountAdmins::get(),
            ),
        )
    }

    fn terminal_weight() -> frame_support::weights::Weight {
        use crate::weights::WeightInfo;
        <T as crate::pallet::Config>::WeightInfo::terminal_callback()
            .max(<T as crate::pallet::Config>::WeightInfo::terminal_callback_ignored())
    }

    fn on_execution_failed_terminal(proposal_id: u128) -> DispatchResult {
        let raw = match engingcore::Pallet::<T>::get_proposal_data(proposal_id) {
            Some(raw) if raw.starts_with(crate::MODULE_TAG) => raw,
            _ => return Ok(()),
        };
        let (action_byte, payload) = pallet::Pallet::<T>::decode_module_action(&raw)?;
        match action_byte {
            ACTION_CREATE => {
                let action = pallet::PersonalCreateActionOf::<T>::decode(&mut &payload[..])
                    .map_err(|_| pallet::Error::<T>::ProposalActionNotFound)?;
                if crate::execute::cleanup_pending_create::<T>(proposal_id, &action, false)? {
                    pallet::Pallet::<T>::deposit_event(pallet::Event::<T>::CreateExecutionFailed {
                        proposal_id,
                        account_id: action.account_id,
                    });
                }
            }
            ACTION_CLOSE => {
                let action = pallet::PersonalCloseActionOf::<T>::decode(&mut &payload[..])
                    .map_err(|_| pallet::Error::<T>::ProposalActionNotFound)?;
                if pallet::PendingCloseProposal::<T>::take(&action.account_id).is_some() {
                    pallet::Pallet::<T>::deposit_event(pallet::Event::<T>::CloseExecutionFailed {
                        proposal_id,
                        account_id: action.account_id,
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }
}
