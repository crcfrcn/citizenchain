#![cfg_attr(not(feature = "std"), no_std)]

//! 个人多签账户生命周期 pallet（MODULE_TAG = `b"per-mgmt"`）。
//!
//! 业务边界:用户自定义的多签账户(无 CID 归属),由 `creator_account_id + account_name`
//! 派生地址 `derive_personal_account`。本模块只承载创建、关闭及投票引擎终态回调。
//!
//! 与机构多签 (`public-manage/private-manage`) 完全独立的 storage / event / error / extrinsic 命名空间;
//! 共用基础设施仅限于 `primitives::core_const` 派生函数、
//! `primitives::multisig` 校验抽象、`votingengine::InternalVoteEngine` 和
//! `admin-primitives` 管理员生命周期 trait。个人多签管理员真源属于 `personal-admins`。

/// 模块标识前缀(8 字节,与机构生命周期模块 tag 长度对齐)。
/// personal-manage / citizenwallet / citizenapp 三方解码必须保持一致。
pub const MODULE_TAG: &[u8] = b"per-mgmt";

/// 提案动作类型常量,独立命名空间(从 0 起编号),与 public-manage/private-manage 的 ACTION 互不干扰。
pub const ACTION_CREATE: u8 = 0;
pub const ACTION_CLOSE: u8 = 1;

pub use pallet::*;

pub mod close;
pub mod create;
pub mod execute;
pub mod traits;
pub mod types;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;

#[cfg(test)]
mod tests;

pub use traits::PersonalMultisigQuery;
pub use types::{PersonalAccount, PersonalCloseAction, PersonalCreateAction, PersonalStatus};

use admin_primitives::{Admin, AdminAccountKind, AdminAccountLifecycle, AdminAccountQuery};
use codec::{Decode, Encode};
use frame_support::{
    ensure,
    pallet_prelude::*,
    traits::{Currency, ReservableCurrency},
    BoundedVec,
};
use frame_system::pallet_prelude::*;
use sp_std::prelude::*;
use votingengine::{
    InternalVoteEngine, InternalVoteResultCallback, ProposalExecutionOutcome,
    PROPOSAL_KIND_INTERNAL, STAGE_INTERNAL, STATUS_EXECUTION_FAILED, STATUS_PASSED,
    STATUS_REJECTED, STATUS_VOTING,
};

pub(crate) type BalanceOf<T> =
    <<T as pallet::Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    #[pallet::config]
    pub trait Config: frame_system::Config + votingengine::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

        /// 内部投票引擎
        type InternalVoteEngine: votingengine::InternalVoteEngine<Self::AccountId>;

        type AccountValidator: primitives::multisig::AccountValidator<Self::AccountId>;
        type ReservedAccountChecker: primitives::multisig::ReservedAccountGuard<Self::AccountId>;
        type ProtectedSourceChecker: primitives::multisig::ProtectedSourceChecker<Self::AccountId>;
        type InstitutionAsset: primitives::institution_asset::InstitutionAsset<Self::AccountId>;

        /// 个人多签管理员生命周期写入口。
        ///
        /// 本模块只请求 personal-admins 写 Pending/Active/Closed 管理员账户，
        /// 不直接保存或修改个人多签管理员集合。
        type PersonalAdminLifecycle: AdminAccountLifecycle<Self::AccountId, Admin<Self::AccountId>>;

        /// 个人多签管理员查询入口。
        ///
        /// 个人多签账户状态由本模块保存；管理员集合与人数从 personal-admins 读取。
        type PersonalAdminQuery: AdminAccountQuery<Self::AccountId>;

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
    /// 管理员集合、管理员数量只允许从 personal-admins 读取；
    /// 普通动态阈值只允许从投票引擎 internal-vote 读取。
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
        StorageMap<_, Blake2_128Concat, u64, PersonalCreateActionOf<T>, OptionQuery>;

    /// 个人多签账户当前进行中的关闭提案 ID(防止并发注销提案)。
    /// 发起 propose_close 时写入,execute_close 成功或执行失败后清除。
    #[pallet::storage]
    #[pallet::getter(fn pending_close_proposal)]
    pub type PendingCloseProposal<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u64, OptionQuery>;

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
            proposal_id: u64,
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
            proposal_id: u64,
            account_id: T::AccountId,
            creator_account_id: T::AccountId,
            admins_len: u32,
            threshold: u32,
            amount: BalanceOf<T>,
            fee: BalanceOf<T>,
        },
        /// 创建提案投票通过但执行失败。
        CreateExecutionFailed {
            proposal_id: u64,
            account_id: T::AccountId,
        },
        /// 创建提案最终被拒绝(投票引擎返回 STATUS_REJECTED 后清理 Pending)。
        PersonalCreateRejected {
            proposal_id: u64,
            account_id: T::AccountId,
        },
        /// 关闭个人多签账户提案已发起。
        PersonalCloseProposed {
            proposal_id: u64,
            account_id: T::AccountId,
            proposer_account_id: T::AccountId,
            beneficiary_account_id: T::AccountId,
        },
        /// 个人多签账户注销成功(投票通过,余额转出,PersonalAccounts 删除)。
        PersonalClosed {
            proposal_id: u64,
            account_id: T::AccountId,
            beneficiary_account_id: T::AccountId,
            admins_len: u32,
            threshold: u32,
            amount: BalanceOf<T>,
            fee: BalanceOf<T>,
        },
        /// 关闭提案投票通过但执行失败。
        CloseExecutionFailed {
            proposal_id: u64,
            account_id: T::AccountId,
        },
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
        /// propose_close 校验:仅个人多签账户可走本入口(非个人地址转 public-manage/private-manage)。
        NotPersonalAccount,
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
        #[pallet::weight(<T as pallet::Config>::WeightInfo::propose_create())]
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
        /// 仅接受个人多签账户(`PersonalAccounts.contains_key` 命中);机构多签走 public-manage/private-manage。
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

        // call_index(2) 已永久废弃：拒绝和执行失败清理由 votingengine 终态回调完成。
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
            ensure!(
                regular_threshold > 0
                    && regular_threshold <= admins_len
                    && u64::from(regular_threshold).saturating_mul(2) > u64::from(admins_len),
                Error::<T>::InvalidThreshold
            );
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
            proposal_id: u64,
            module_tag: &[u8],
            account_id: T::AccountId,
            expected_status: u8,
            require_callback_scope: bool,
        ) -> DispatchResult {
            ensure!(
                votingengine::Pallet::<T>::is_proposal_owner(proposal_id, module_tag),
                Error::<T>::ProposalActionNotFound
            );
            let proposal = votingengine::Pallet::<T>::proposals(proposal_id)
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
                proposal.internal_code == Some(votingengine::types::PMUL),
                Error::<T>::ProposalActionNotFound
            );
            ensure!(
                proposal.status == expected_status,
                Error::<T>::ProposalActionNotFound
            );
            if require_callback_scope {
                ensure!(
                    votingengine::Pallet::<T>::is_callback_execution_scope(proposal_id),
                    Error::<T>::ProposalActionNotFound
                );
            }
            Ok(())
        }

        pub fn active_admin_account_exists(
            institution_code: votingengine::types::InstitutionCode,
            account_id: T::AccountId,
        ) -> bool {
            T::PersonalAdminQuery::active_admin_account_exists(institution_code, account_id)
        }

        pub fn is_active_account_admin(
            institution_code: votingengine::types::InstitutionCode,
            account_id: T::AccountId,
            who: &T::AccountId,
        ) -> bool {
            T::PersonalAdminQuery::is_active_account_admin(institution_code, account_id, who)
        }

        pub fn active_account_admins(
            institution_code: votingengine::types::InstitutionCode,
            account_id: T::AccountId,
        ) -> Option<Vec<T::AccountId>> {
            T::PersonalAdminQuery::active_account_admins(institution_code, account_id)
        }

        pub fn active_account_admins_len(
            institution_code: votingengine::types::InstitutionCode,
            account_id: T::AccountId,
        ) -> Option<u32> {
            T::PersonalAdminQuery::active_account_admins_len(institution_code, account_id)
        }

        pub fn pending_account_exists_for_snapshot(
            institution_code: votingengine::types::InstitutionCode,
            account_id: T::AccountId,
        ) -> bool {
            T::PersonalAdminQuery::pending_account_exists_for_snapshot(institution_code, account_id)
        }

        pub fn is_pending_account_admin_for_snapshot(
            institution_code: votingengine::types::InstitutionCode,
            account_id: T::AccountId,
            who: &T::AccountId,
        ) -> bool {
            T::PersonalAdminQuery::is_pending_account_admin_for_snapshot(
                institution_code,
                account_id,
                who,
            )
        }

        pub fn pending_account_admins_for_snapshot(
            institution_code: votingengine::types::InstitutionCode,
            account_id: T::AccountId,
        ) -> Option<Vec<T::AccountId>> {
            T::PersonalAdminQuery::pending_account_admins_for_snapshot(institution_code, account_id)
        }

        pub fn pending_account_admins_len_for_snapshot(
            institution_code: votingengine::types::InstitutionCode,
            account_id: T::AccountId,
        ) -> Option<u32> {
            T::PersonalAdminQuery::pending_account_admins_len_for_snapshot(
                institution_code,
                account_id,
            )
        }

        pub(crate) fn create_pending_admin_account_for_proposal(
            proposal_id: u64,
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
            T::PersonalAdminLifecycle::create_pending_admin_account_for_proposal(
                proposal_id,
                crate::MODULE_TAG,
                account_id,
                Vec::new(),
                votingengine::types::PMUL,
                kind,
                admins.iter().cloned().collect(),
                creator_account_id.clone(),
            )
        }

        pub(crate) fn activate_admin_account(
            proposal_id: u64,
            account_id: T::AccountId,
        ) -> DispatchResult {
            Self::ensure_lifecycle_proposal(
                proposal_id,
                crate::MODULE_TAG,
                account_id.clone(),
                STATUS_PASSED,
                true,
            )?;
            T::PersonalAdminLifecycle::activate_admin_account_for_proposal(
                proposal_id,
                crate::MODULE_TAG,
                account_id,
            )
        }

        pub(crate) fn remove_pending_admin_account(
            proposal_id: u64,
            account_id: T::AccountId,
        ) -> DispatchResult {
            let proposal = votingengine::Pallet::<T>::proposals(proposal_id)
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
            T::PersonalAdminLifecycle::remove_pending_admin_account_for_proposal(
                proposal_id,
                crate::MODULE_TAG,
                account_id,
            )
        }

        pub(crate) fn close_admin_account(
            proposal_id: u64,
            account_id: T::AccountId,
        ) -> DispatchResult {
            Self::ensure_lifecycle_proposal(
                proposal_id,
                crate::MODULE_TAG,
                account_id.clone(),
                STATUS_PASSED,
                true,
            )?;
            T::PersonalAdminLifecycle::close_admin_account_for_proposal(
                proposal_id,
                crate::MODULE_TAG,
                account_id,
            )
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
// `votingengine::InternalVoteResultCallback` tuple 广播回来。
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
        let institution_code = votingengine::types::PMUL;
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
        proposal_id: u64,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, sp_runtime::DispatchError> {
        use frame_support::storage::{with_transaction, TransactionOutcome};
        if !votingengine::Pallet::<T>::is_proposal_owner(proposal_id, crate::MODULE_TAG) {
            return Ok(ProposalExecutionOutcome::Ignored);
        }
        let raw = votingengine::Pallet::<T>::get_proposal_data(proposal_id)
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

    fn on_execution_failed_terminal(proposal_id: u64) -> DispatchResult {
        let raw = match votingengine::Pallet::<T>::get_proposal_data(proposal_id) {
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
