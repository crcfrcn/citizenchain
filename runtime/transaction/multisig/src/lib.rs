//! # 多签资金账户转账模块 (multisig)
//!
//! 本模块为所有机构多签账户和个人多签账户提供链上转账治理流程：
//! - 机构由具备目标业务权限的岗位任职人发起，个人多签由管理员发起；统一经内部投票执行。
//! - 自动执行失败时保留提案状态，可通过 `VotingEngine::retry_passed_proposal` 手动重试。
//! - 余额在提案创建和执行两个时点双重检查，含手续费和 ED 保留。
//! - 收款地址不能是转出资金账户自身，也不能是受保护地址(质押地址等)。
//! - 本模块只处理转账提案与执行；个人多签生命周期归 `personal-manage`，
//!   个人多签管理员真源归 `personal-admins`。

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{ensure, pallet_prelude::*, traits::Currency, BoundedVec};
use frame_system::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_runtime::traits::{CheckedAdd, SaturatedConversion, Zero};

extern crate alloc;

use alloc::{vec, vec::Vec};

use primitives::account_derive::{RESERVED_NAME_FEE, RESERVED_NAME_MAIN};
use primitives::cid::china::china_cb::{CHINA_CB, SAFETY_FUND_ACCOUNT};
use primitives::fee_policy::OnchainFeeCharger;
use votingengine::{
    types::{
        institution_code_from_cid_number, AuthorizationSubject, BusinessActionId, CidNumber,
        InstitutionCode, RoleCode, RoleSubject, VotePlanOf, VotingEngineKind, NRC, PMUL, PRB,
    },
    InternalVoteResultCallback, ProposalExecutionOutcome, PROPOSAL_KIND_INTERNAL, STAGE_INTERNAL,
    STATUS_PASSED,
};

pub use pallet::*;
/// 模块标识前缀，用于在 ProposalData 中区分不同业务模块，防止跨模块误解码。
pub const MODULE_TAG: &[u8] = b"multisig";
const SAFETY_FUND_OWNER_DATA: &[u8] = b"multisig:safety";
const SWEEP_OWNER_DATA: &[u8] = b"multisig:sweep";

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;
pub mod weights;

type BalanceOf<T> =
    <<T as pallet::Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

/// 转账动作：记录一次转账提案的完整业务参数。
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(MaxRemarkLen))]
pub struct TransferAction<AccountId, Balance, MaxRemarkLen: Get<u32>> {
    /// 机构转账必须存在 CID；个人多签没有 CID，严格使用 None。
    pub actor_cid_number: Option<CidNumber>,
    /// 实际转出资金的机构账户或个人多签账户。
    pub funding_account_id: AccountId,
    /// 收款地址
    pub beneficiary_account_id: AccountId,
    /// 转账金额
    pub amount: Balance,
    /// 备注
    pub remark: BoundedVec<u8, MaxRemarkLen>,
    /// 发起管理员
    pub proposer_account_id: AccountId,
}

/// 安全基金转账动作：从国家储委会安全基金账户向指定收款地址转账。
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(MaxRemarkLen))]
pub struct SafetyFundAction<AccountId, Balance, MaxRemarkLen: Get<u32>> {
    /// 国家储委会唯一 CID。
    pub actor_cid_number: CidNumber,
    /// 国家储委会安全基金账户。
    pub institution_account_id: AccountId,
    /// 收款地址
    pub beneficiary_account_id: AccountId,
    /// 转账金额
    pub amount: Balance,
    /// 备注
    pub remark: BoundedVec<u8, MaxRemarkLen>,
    /// 发起管理员
    pub proposer_account_id: AccountId,
}

/// 手续费划转动作：从机构手续费账户向机构主账户划转。
///
/// `proposer_account_id` 字段与 transfer / safety_fund 两类动作对齐,便于 Executor 在
/// 投票通过 / 否决回调时统一识别提案发起人。
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct SweepAction<AccountId, Balance> {
    /// 发起机构唯一 CID。
    pub actor_cid_number: CidNumber,
    /// 实际转出的机构费用账户。
    pub institution_account_id: AccountId,
    /// 划转金额
    pub amount: Balance,
    /// 发起管理员(Tx 1 中锁定)
    pub proposer_account_id: AccountId,
}

/// 单次划转上限：可用余额的 80%。
const FEE_SWEEP_MAX_PERCENT: u128 = 80;

fn decode_raw_account<T: frame_system::Config>(raw: &[u8; 32]) -> Option<T::AccountId> {
    T::AccountId::decode(&mut &raw[..]).ok()
}

fn nrc_cid() -> CidNumber {
    CHINA_CB[0]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("NRC CID must fit protocol bound")
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    use entity_primitives::ProtectedSourceChecker;
    use frame_support::traits::ExistenceRequirement;
    use primitives::institution_asset::{InstitutionAsset, InstitutionAssetAction};
    use votingengine::InternalAdminProvider;
    use votingengine::InternalVoteEngine;

    #[pallet::config]
    pub trait Config: frame_system::Config + votingengine::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// 链上基础货币。
        type Currency: Currency<Self::AccountId>;

        /// 内部投票引擎。
        type InternalVoteEngine: votingengine::InternalVoteEngine<Self::AccountId>;

        /// 机构岗位业务授权真源；个人多签不经过该接口。
        type InstitutionRoleAuthorization: entity_primitives::InstitutionRoleAuthorizationQuery<
            Self::AccountId,
        >;

        /// 资金白名单检查器。
        type InstitutionAsset: primitives::institution_asset::InstitutionAsset<Self::AccountId>;

        /// 资金源保护检查器。
        type ProtectedSourceChecker: ProtectedSourceChecker<Self::AccountId>;

        /// 备注最大长度
        #[pallet::constant]
        type MaxRemarkLen: Get<u32>;

        /// 投票通过后的链上交易费统一执行器。
        type OnchainFeeCharger: primitives::fee_policy::OnchainFeeCharger<
            Self::AccountId,
            BalanceOf<Self>,
        >;

        /// 个人多签账户状态与管理员配置查询,由 personal-manage 聚合 personal-admins 提供。
        type PersonalQuery: personal_manage::traits::PersonalMultisigQuery<Self::AccountId>;

        /// 注册机构账户状态与管理员配置查询,由 runtime 聚合 public/private 生命周期模块提供。
        type InstitutionQuery: entity_primitives::InstitutionMultisigQuery<Self::AccountId>;

        /// Weight 配置
        type WeightInfo: crate::weights::WeightInfo;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    // 活跃提案数限制已移至 votingengine::active_proposal_limit 全局管控。
    // 提案业务数据和元数据已统一存储到 votingengine（ProposalData / ProposalMeta）。

    /// 安全基金转账提案动作存储。
    #[pallet::storage]
    pub type SafetyFundProposalActions<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        SafetyFundAction<T::AccountId, BalanceOf<T>, T::MaxRemarkLen>,
        OptionQuery,
    >;

    /// 手续费划转提案动作存储（省储行 + 国家储委会共用）。
    #[pallet::storage]
    pub type SweepProposalActions<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, SweepAction<T::AccountId, BalanceOf<T>>, OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 转账提案已创建。citizenapp 可扫描此事件展示投票详情。
        TransferProposed {
            proposal_id: u64,
            institution_code: InstitutionCode,
            actor_cid_number: Option<CidNumber>,
            proposer_account_id: T::AccountId,
            funding_account_id: T::AccountId,
            beneficiary_account_id: T::AccountId,
            amount: BalanceOf<T>,
            /// 原文 remark,供管理员投票前核对。
            remark: BoundedVec<u8, T::MaxRemarkLen>,
            /// 投票引擎分配的超时区块,供 citizenapp 倒计时
            expires_at: BlockNumberFor<T>,
        },
        /// 投票通过但执行失败（投票已记录，提案数据保留，可通过 VotingEngine 统一入口手动重试）
        TransferExecutionFailed {
            proposal_id: u64,
            funding_account_id: T::AccountId,
        },
        /// 转账已执行（投票通过后自动触发，含手续费分账）
        TransferExecuted {
            proposal_id: u64,
            funding_account_id: T::AccountId,
            fee_payer: T::AccountId,
            beneficiary_account_id: T::AccountId,
            amount: BalanceOf<T>,
            fee: BalanceOf<T>,
        },
        /// 安全基金转账提案已创建。
        SafetyFundTransferProposed {
            proposal_id: u64,
            actor_cid_number: CidNumber,
            proposer_account_id: T::AccountId,
            institution_account_id: T::AccountId,
            beneficiary_account_id: T::AccountId,
            amount: BalanceOf<T>,
            /// 原文 remark,供管理员投票前核对。
            remark: BoundedVec<u8, T::MaxRemarkLen>,
            expires_at: BlockNumberFor<T>,
        },
        /// 安全基金转账已执行
        SafetyFundTransferExecuted {
            proposal_id: u64,
            fee_payer: T::AccountId,
            beneficiary_account_id: T::AccountId,
            amount: BalanceOf<T>,
            fee: BalanceOf<T>,
        },
        /// 安全基金投票通过但执行失败
        SafetyFundExecutionFailed { proposal_id: u64 },
        /// 手续费划转提案已创建。
        SweepToMainProposed {
            proposal_id: u64,
            actor_cid_number: CidNumber,
            proposer_account_id: T::AccountId,
            institution_account_id: T::AccountId,
            main_account: T::AccountId,
            amount: BalanceOf<T>,
            expires_at: BlockNumberFor<T>,
        },
        /// 手续费划转已执行
        SweepToMainExecuted {
            proposal_id: u64,
            actor_cid_number: CidNumber,
            institution_account_id: T::AccountId,
            amount: BalanceOf<T>,
            fee: BalanceOf<T>,
            reserve_left: BalanceOf<T>,
        },
        /// 手续费划转投票通过但执行失败
        SweepExecutionFailed { proposal_id: u64 },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// 资金账户不属于有效机构或个人多签账户。
        InvalidInstitution,
        /// 调用者声明的机构码与资金账户实际分类不一致。
        InstitutionCodeMismatch,
        /// 调用者不是该多签资金账户的管理员。
        UnauthorizedAdmin,
        /// 资金账户保护检查未通过（如冻结期间禁止支出）。
        InstitutionSpendNotAllowed,
        /// 转账金额不能为零。
        ZeroAmount,
        /// 转账金额低于 ED（存在性保证金），收款地址可能无法创建。
        AmountBelowExistentialDeposit,
        /// 不允许转账给转出资金账户自身。
        SelfTransferNotAllowed,
        /// 收款地址是受保护地址（如质押地址），不允许作为收款方。
        BeneficiaryIsProtectedAddress,
        /// 提案动作数据未找到或解码失败。
        ProposalActionNotFound,
        /// 机构账户地址解码失败。
        InstitutionAccountDecodeFailed,
        /// 资金账户余额不足（需 amount + fee + ED）。
        InsufficientBalance,
        /// 提案未达到通过状态，不可执行。
        ProposalNotPassed,
        /// 链上转账操作失败。
        TransferFailed,
        /// 安全基金提案未找到。
        SafetyFundProposalNotFound,
        /// 安全基金余额不足。
        SafetyFundInsufficientBalance,
        /// 安全基金提案未通过。
        SafetyFundProposalNotPassed,
        /// 手续费划转提案未找到。
        SweepProposalNotFound,
        /// 手续费划转金额无效。
        InvalidSweepAmount,
        /// 手续费账户余额不足（需保留最低余额）。
        InsufficientFeeReserve,
        /// 手续费划转金额超过上限（可用余额的 80%）。
        SweepAmountExceedsCap,
        /// 手续费划转提案未通过。
        SweepProposalNotPassed,
        /// 无法从 actor CID 唯一解析费用账户。
        FeeAccountMissing,
        /// 费用账户无法支付金额手续费并保留 ED。
        InsufficientFeeBalance,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 发起多签资金账户转账提案。
        #[pallet::call_index(0)]
        #[pallet::weight(<T as pallet::Config>::WeightInfo::propose_transfer())]
        pub fn propose_transfer(
            origin: OriginFor<T>,
            actor_cid_number: Option<CidNumber>,
            proposer_role_code: Option<RoleCode>,
            funding_account_id: T::AccountId,
            beneficiary_account_id: T::AccountId,
            amount: BalanceOf<T>,
            remark: BoundedVec<u8, T::MaxRemarkLen>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            ensure!(amount > Zero::zero(), Error::<T>::ZeroAmount);
            let (institution_code, subject_cid_numbers) =
                Self::resolve_funding_authority(actor_cid_number.as_ref(), &funding_account_id)?;
            if actor_cid_number.is_none() {
                ensure!(proposer_role_code.is_none(), Error::<T>::UnauthorizedAdmin);
                ensure!(
                    <T as votingengine::Config>::InternalAdminProvider::is_personal_admin(
                        funding_account_id.clone(),
                        &who,
                    ),
                    Error::<T>::UnauthorizedAdmin
                );
            }
            ensure!(
                <T as Config>::InstitutionAsset::can_spend(
                    &funding_account_id,
                    InstitutionAssetAction::MultisigTransferExecute,
                ),
                Error::<T>::InstitutionSpendNotAllowed
            );

            // 转账金额必须 >= ED，防止收款地址不存在时创建失败
            let ed = <T as Config>::Currency::minimum_balance();
            ensure!(amount >= ed, Error::<T>::AmountBelowExistentialDeposit);

            // 不允许自转账
            ensure!(
                beneficiary_account_id != funding_account_id,
                Error::<T>::SelfTransferNotAllowed
            );

            // 不允许转到受保护地址（质押地址）
            ensure!(
                !<T as Config>::ProtectedSourceChecker::is_protected(&beneficiary_account_id,),
                Error::<T>::BeneficiaryIsProtectedAddress
            );

            // 活跃提案数由 votingengine 在 create_internal_proposal 中统一检查

            // 预检与执行一致：机构本金账户只承担本金，金额手续费由同 CID 费用账户承担；
            // 个人多签没有机构费用账户，继续由个人资金账户承担本金和执行手续费。
            let amount_u128: u128 = amount.saturated_into();
            let fee_u128 = primitives::fee_policy::calculate_onchain_fee(amount_u128);
            let fee: BalanceOf<T> = fee_u128.saturated_into();
            let free = <T as Config>::Currency::free_balance(&funding_account_id);
            let principal_required = amount
                .checked_add(&ed)
                .ok_or(Error::<T>::InsufficientBalance)?;
            if let Some(cid_number) = actor_cid_number.as_ref() {
                ensure!(free >= principal_required, Error::<T>::InsufficientBalance);
                let fee_account = Self::resolve_fee_account(cid_number)?;
                let fee_required = fee
                    .checked_add(&ed)
                    .ok_or(Error::<T>::InsufficientFeeBalance)?;
                ensure!(
                    <T as Config>::Currency::free_balance(&fee_account) >= fee_required,
                    Error::<T>::InsufficientFeeBalance
                );
            } else {
                let required = principal_required
                    .checked_add(&fee)
                    .ok_or(Error::<T>::InsufficientBalance)?;
                ensure!(free >= required, Error::<T>::InsufficientBalance);
            }

            let action = TransferAction {
                actor_cid_number: actor_cid_number.clone(),
                funding_account_id: funding_account_id.clone(),
                beneficiary_account_id: beneficiary_account_id.clone(),
                amount,
                remark: remark.clone(),
                proposer_account_id: who.clone(),
            };
            let mut encoded = sp_runtime::Vec::from(crate::MODULE_TAG);
            encoded.extend_from_slice(&action.encode());
            // 创建提案时同步写入 owner/data/meta，禁止后续跨模块覆写业务数据。
            let proposal_id = if let Some(cid_number) = actor_cid_number.as_ref() {
                let role_code = proposer_role_code
                    .as_ref()
                    .ok_or(Error::<T>::UnauthorizedAdmin)?;
                let vote_plan = Self::build_institution_vote_plan(
                    &who,
                    cid_number.as_slice(),
                    role_code.as_slice(),
                    entity_primitives::business_action::ACTION_MULTISIG_TRANSFER,
                    &encoded,
                )?;
                <T as Config>::InternalVoteEngine::create_institution_proposal_with_data(
                    who.clone(),
                    institution_code,
                    cid_number.to_vec(),
                    Some(funding_account_id.clone()),
                    subject_cid_numbers,
                    vote_plan,
                    encoded,
                )?
            } else {
                <T as Config>::InternalVoteEngine::create_personal_proposal_with_data(
                    who.clone(),
                    funding_account_id.clone(),
                    crate::MODULE_TAG,
                    encoded,
                )?
            };

            // 从投票引擎回读 proposal.end 作为 expires_at,供 citizenapp 倒计时。
            let expires_at = votingengine::Pallet::<T>::proposals(proposal_id)
                .map(|p| p.end)
                .ok_or(Error::<T>::ProposalActionNotFound)?;

            Self::deposit_event(Event::<T>::TransferProposed {
                proposal_id,
                institution_code,
                actor_cid_number,
                proposer_account_id: who,
                funding_account_id,
                beneficiary_account_id,
                amount,
                remark,
                expires_at,
            });
            Ok(())
        }

        /// 发起国家储委会安全基金转账提案（内部投票）。
        ///
        /// 从安全基金账户（`SAFETY_FUND_ACCOUNT`）向指定收款地址转账。
        /// 仅国家储委会委员岗位有效任职人可发起。
        #[pallet::call_index(1)]
        #[pallet::weight(T::DbWeight::get().reads_writes(4, 2))]
        pub fn propose_safety_fund_transfer(
            origin: OriginFor<T>,
            actor_cid_number: CidNumber,
            proposer_role_code: RoleCode,
            institution_account_id: T::AccountId,
            beneficiary_account_id: T::AccountId,
            amount: BalanceOf<T>,
            remark: BoundedVec<u8, T::MaxRemarkLen>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(amount > Zero::zero(), Error::<T>::ZeroAmount);

            // 安全基金业务永久绑定国家储委会及其委员岗位权限。
            ensure!(
                actor_cid_number == nrc_cid(),
                Error::<T>::InvalidInstitution
            );
            let expected_safety_fund = Self::decode_institution_account(&SAFETY_FUND_ACCOUNT)?;
            ensure!(
                institution_account_id == expected_safety_fund,
                Error::<T>::InvalidInstitution
            );
            // 验证安全基金账户余额
            ensure!(
                <T as Config>::InstitutionAsset::can_spend(
                    &institution_account_id,
                    InstitutionAssetAction::NrcSafetyFundTransfer,
                ),
                Error::<T>::InstitutionSpendNotAllowed
            );

            // 安全基金只承担本金，NRC 费用账户单独承担金额手续费。
            let amount_u128: u128 = amount.saturated_into();
            let fee_u128 = primitives::fee_policy::calculate_onchain_fee(amount_u128);
            let fee: BalanceOf<T> = fee_u128.saturated_into();
            let ed: BalanceOf<T> = <T as Config>::Currency::minimum_balance();
            let free = <T as Config>::Currency::free_balance(&institution_account_id);
            let required = amount
                .checked_add(&ed)
                .ok_or(Error::<T>::SafetyFundInsufficientBalance)?;
            ensure!(free >= required, Error::<T>::SafetyFundInsufficientBalance);
            let fee_account = Self::resolve_fee_account(&actor_cid_number)?;
            let fee_required = fee
                .checked_add(&ed)
                .ok_or(Error::<T>::InsufficientFeeBalance)?;
            ensure!(
                <T as Config>::Currency::free_balance(&fee_account) >= fee_required,
                Error::<T>::InsufficientFeeBalance
            );

            let proposal_data = sp_runtime::Vec::from(SAFETY_FUND_OWNER_DATA);
            let vote_plan = Self::build_institution_vote_plan(
                &who,
                actor_cid_number.as_slice(),
                proposer_role_code.as_slice(),
                entity_primitives::business_action::ACTION_SAFETY_FUND_TRANSFER,
                &proposal_data,
            )?;
            let proposal_id =
                <T as Config>::InternalVoteEngine::create_institution_proposal_with_data(
                    who.clone(),
                    NRC,
                    actor_cid_number.to_vec(),
                    Some(institution_account_id.clone()),
                    vec![actor_cid_number.to_vec()],
                    vote_plan,
                    proposal_data,
                )?;

            SafetyFundProposalActions::<T>::insert(
                proposal_id,
                SafetyFundAction {
                    actor_cid_number: actor_cid_number.clone(),
                    institution_account_id: institution_account_id.clone(),
                    beneficiary_account_id: beneficiary_account_id.clone(),
                    amount,
                    remark: remark.clone(),
                    proposer_account_id: who.clone(),
                },
            );

            // 从投票引擎回读 proposal.end 作为 expires_at。
            let expires_at = votingengine::Pallet::<T>::proposals(proposal_id)
                .map(|p| p.end)
                .ok_or(Error::<T>::SafetyFundProposalNotFound)?;

            Self::deposit_event(Event::SafetyFundTransferProposed {
                proposal_id,
                actor_cid_number,
                proposer_account_id: who,
                institution_account_id,
                beneficiary_account_id,
                amount,
                remark,
                expires_at,
            });
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(T::DbWeight::get().reads_writes(4, 2))]
        pub fn propose_sweep_to_main(
            origin: OriginFor<T>,
            actor_cid_number: CidNumber,
            proposer_role_code: RoleCode,
            institution_account_id: T::AccountId,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let amount_u128: u128 = amount.saturated_into();
            ensure!(amount_u128 > 0, Error::<T>::InvalidSweepAmount);

            // 动态判断治理机构码类型。
            let institution_code = Self::resolve_sweep_org(&actor_cid_number)?;
            let fee_account = Self::resolve_fee_account(&actor_cid_number)?;
            ensure!(
                institution_account_id == fee_account,
                Error::<T>::InvalidInstitution
            );
            let proposal_data = sp_runtime::Vec::from(SWEEP_OWNER_DATA);
            let vote_plan = Self::build_institution_vote_plan(
                &who,
                actor_cid_number.as_slice(),
                proposer_role_code.as_slice(),
                entity_primitives::business_action::ACTION_FEE_SWEEP_TO_MAIN,
                &proposal_data,
            )?;
            let proposal_id =
                <T as Config>::InternalVoteEngine::create_institution_proposal_with_data(
                    who.clone(),
                    institution_code,
                    actor_cid_number.to_vec(),
                    Some(institution_account_id.clone()),
                    vec![actor_cid_number.to_vec()],
                    vote_plan,
                    proposal_data,
                )?;

            SweepProposalActions::<T>::insert(
                proposal_id,
                SweepAction {
                    actor_cid_number: actor_cid_number.clone(),
                    institution_account_id: institution_account_id.clone(),
                    amount,
                    proposer_account_id: who.clone(),
                },
            );

            let main_account = Self::resolve_main_account(&actor_cid_number)?;
            let expires_at = votingengine::Pallet::<T>::proposals(proposal_id)
                .map(|p| p.end)
                .ok_or(Error::<T>::SweepProposalNotFound)?;

            Self::deposit_event(Event::SweepToMainProposed {
                proposal_id,
                actor_cid_number,
                proposer_account_id: who,
                institution_account_id,
                main_account,
                amount,
                expires_at,
            });
            Ok(())
        }

        // call_index 3/4/5 永久保留空位,不复用。
    }

    impl<T: Config> Pallet<T> {
        fn decode_institution_account(raw: &[u8; 32]) -> Result<T::AccountId, Error<T>> {
            decode_raw_account::<T>(raw).ok_or(Error::<T>::InstitutionAccountDecodeFailed)
        }

        fn resolve_funding_authority(
            actor_cid_number: Option<&CidNumber>,
            funding_account_id: &T::AccountId,
        ) -> Result<(InstitutionCode, Vec<Vec<u8>>), Error<T>> {
            use entity_primitives::InstitutionMultisigQuery;
            use personal_manage::traits::PersonalMultisigQuery;

            let Some(cid_number) = actor_cid_number else {
                ensure!(
                    <T as Config>::PersonalQuery::is_active(funding_account_id),
                    Error::<T>::InvalidInstitution
                );
                return Ok((PMUL, Vec::new()));
            };

            ensure!(
                <T as Config>::InstitutionQuery::account_exists(funding_account_id),
                Error::<T>::InvalidInstitution
            );
            let stored_cid = <T as Config>::InstitutionQuery::lookup_cid(funding_account_id)
                .ok_or(Error::<T>::InvalidInstitution)?;
            ensure!(
                stored_cid.as_slice() == cid_number.as_slice(),
                Error::<T>::InvalidInstitution
            );
            let cid_text = core::str::from_utf8(cid_number.as_slice())
                .map_err(|_| Error::<T>::InvalidInstitution)?;
            let institution_code =
                institution_code_from_cid_number(cid_text).ok_or(Error::<T>::InvalidInstitution)?;
            let stored_code = <T as Config>::InstitutionQuery::lookup_org(funding_account_id)
                .ok_or(Error::<T>::InvalidInstitution)?;
            ensure!(
                stored_code == institution_code,
                Error::<T>::InstitutionCodeMismatch
            );
            Ok((institution_code, vec![cid_number.to_vec()]))
        }

        fn build_institution_vote_plan(
            who: &T::AccountId,
            cid_number: &[u8],
            proposer_role_code: &[u8],
            action_code: u32,
            proposal_data: &[u8],
        ) -> Result<VotePlanOf<T::AccountId>, sp_runtime::DispatchError> {
            use entity_primitives::{InstitutionRoleAuthorizationQuery, RolePermissionOperation};

            let action_id = BusinessActionId {
                module_tag: crate::MODULE_TAG.to_vec(),
                action_code,
            };
            let proposer_account_id = entity_primitives::RoleSubject {
                cid_number: cid_number.to_vec(),
                role_code: proposer_role_code.to_vec(),
            };
            ensure!(
                T::InstitutionRoleAuthorization::is_authorized(
                    who,
                    &proposer_account_id,
                    &action_id,
                    RolePermissionOperation::Propose,
                ),
                Error::<T>::UnauthorizedAdmin
            );
            let voter_subjects = T::InstitutionRoleAuthorization::role_subjects_with_permission(
                cid_number,
                &action_id,
                RolePermissionOperation::Vote,
            )
            .into_iter()
            .map(|role| {
                Ok(AuthorizationSubject::Institution(RoleSubject {
                    cid_number: CidNumber::try_from(role.cid_number)
                        .map_err(|_| votingengine::Error::<T>::InvalidVotePlan)?,
                    role_code: RoleCode::try_from(role.role_code)
                        .map_err(|_| votingengine::Error::<T>::InvalidVotePlan)?,
                }))
            })
            .collect::<Result<Vec<_>, sp_runtime::DispatchError>>()?;
            let owner: frame_support::BoundedVec<
                u8,
                frame_support::traits::ConstU32<
                    { entity_primitives::BUSINESS_MODULE_TAG_MAX_BYTES },
                >,
            > = crate::MODULE_TAG
                .to_vec()
                .try_into()
                .map_err(|_| votingengine::Error::<T>::InvalidVotePlan)?;
            VotePlanOf::<T::AccountId>::try_new(
                BusinessActionId {
                    module_tag: owner.clone(),
                    action_code,
                },
                owner,
                AuthorizationSubject::Institution(RoleSubject {
                    cid_number: CidNumber::try_from(cid_number.to_vec())
                        .map_err(|_| votingengine::Error::<T>::InvalidVotePlan)?,
                    role_code: RoleCode::try_from(proposer_role_code.to_vec())
                        .map_err(|_| votingengine::Error::<T>::InvalidVotePlan)?,
                }),
                voter_subjects,
                VotingEngineKind::Internal,
                sp_io::hashing::blake2_256(proposal_data),
            )
            .map_err(|_| votingengine::Error::<T>::InvalidVotePlan.into())
        }

        /// 复核内部投票提案与具体资金业务的完整绑定。
        ///
        /// 投票引擎只证明内部投票已经通过；资金模块仍须证明通过的是当前机构、
        /// 当前账户和当前 CID 的本模块提案。自动执行与统一重试共用本校验。
        fn ensure_internal_business_proposal(
            proposal_id: u64,
            institution_code: InstitutionCode,
            actor_cid_number: Option<&CidNumber>,
            funding_account_id: &T::AccountId,
            subject_cid_numbers: &[Vec<u8>],
        ) -> DispatchResult {
            let proposal = votingengine::Pallet::<T>::proposals(proposal_id)
                .ok_or(Error::<T>::ProposalActionNotFound)?;
            ensure!(
                votingengine::Pallet::<T>::is_callback_execution_scope(proposal_id)
                    && votingengine::Pallet::<T>::is_proposal_owner(proposal_id, crate::MODULE_TAG,)
                    && proposal.kind == PROPOSAL_KIND_INTERNAL
                    && proposal.stage == STAGE_INTERNAL
                    && proposal.status == STATUS_PASSED
                    && proposal.internal_code == Some(institution_code)
                    && proposal.actor_cid_number.as_ref() == actor_cid_number
                    && proposal.execution_account_id.as_ref() == Some(funding_account_id)
                    && proposal.subject_cid_numbers.len() == subject_cid_numbers.len()
                    && subject_cid_numbers.iter().all(|expected| {
                        proposal
                            .subject_cid_numbers
                            .iter()
                            .any(|actual| actual.as_slice() == expected.as_slice())
                    }),
                Error::<T>::ProposalNotPassed
            );
            Ok(())
        }

        /// 判断治理机构码类型用于 sweep 提案。
        ///
        /// sweep 只服务治理机构费用账户，不接入个人多签或注册机构账户。
        fn resolve_sweep_org(cid_number: &CidNumber) -> Result<InstitutionCode, Error<T>> {
            let text = core::str::from_utf8(cid_number.as_slice())
                .map_err(|_| Error::<T>::InvalidInstitution)?;
            let code =
                institution_code_from_cid_number(text).ok_or(Error::<T>::InvalidInstitution)?;
            ensure!(matches!(code, NRC | PRB), Error::<T>::InvalidInstitution);
            Ok(code)
        }

        /// 解析治理机构手续费账户。
        fn resolve_fee_account(cid_number: &CidNumber) -> Result<T::AccountId, DispatchError> {
            use entity_primitives::InstitutionMultisigQuery;
            T::InstitutionQuery::lookup_institution_account(
                cid_number.as_slice(),
                RESERVED_NAME_FEE,
            )
            .ok_or(Error::<T>::FeeAccountMissing.into())
        }

        /// 解析治理机构主账户。
        fn resolve_main_account(cid_number: &CidNumber) -> Result<T::AccountId, DispatchError> {
            use entity_primitives::InstitutionMultisigQuery;
            T::InstitutionQuery::lookup_institution_account(
                cid_number.as_slice(),
                RESERVED_NAME_MAIN,
            )
            .ok_or(Error::<T>::InvalidInstitution.into())
        }

        pub(crate) fn try_execute_sweep_from_callback(proposal_id: u64) -> DispatchResult {
            let action = SweepProposalActions::<T>::get(proposal_id)
                .ok_or(Error::<T>::SweepProposalNotFound)?;

            let institution_code = Self::resolve_sweep_org(&action.actor_cid_number)?;
            Self::ensure_internal_business_proposal(
                proposal_id,
                institution_code,
                Some(&action.actor_cid_number),
                &action.institution_account_id,
                &[action.actor_cid_number.to_vec()],
            )
            .map_err(|_| Error::<T>::SweepProposalNotPassed)?;

            let fee_account = Self::resolve_fee_account(&action.actor_cid_number)?;
            ensure!(
                fee_account == action.institution_account_id,
                Error::<T>::InvalidInstitution
            );
            let main_account = Self::resolve_main_account(&action.actor_cid_number)?;

            ensure!(
                <T as Config>::InstitutionAsset::can_spend(
                    &fee_account,
                    InstitutionAssetAction::OffchainFeeSweepExecute,
                ),
                Error::<T>::InstitutionSpendNotAllowed
            );

            // ── 计算手续费 ──
            let amount_u128: u128 = action.amount.saturated_into();
            let tx_fee_u128 = primitives::fee_policy::calculate_onchain_fee(amount_u128);
            let tx_fee: BalanceOf<T> = tx_fee_u128.saturated_into();

            let fee_balance_u128: u128 =
                <T as Config>::Currency::free_balance(&fee_account).saturated_into();
            // 费用账户只需在支出后保留链上 ED，不设置账户级预存金额。
            let reserve_u128: u128 = <T as Config>::Currency::minimum_balance().saturated_into();

            // ── 余额检查：amount + tx_fee + reserve ──
            let total_deduct_u128 = amount_u128.saturating_add(tx_fee_u128);
            ensure!(
                fee_balance_u128 >= total_deduct_u128
                    && fee_balance_u128.saturating_sub(total_deduct_u128) >= reserve_u128,
                Error::<T>::InsufficientFeeReserve
            );
            // ── cap 检查：划转金额不超过可用余额的 80%（可用 = 余额 - reserve） ──
            let available_u128 = fee_balance_u128.saturating_sub(reserve_u128);
            let cap_u128 = available_u128
                .saturating_mul(FEE_SWEEP_MAX_PERCENT)
                .saturating_div(100);
            ensure!(amount_u128 <= cap_u128, Error::<T>::SweepAmountExceedsCap);

            // 费用账户同时承担归集本金和金额手续费，两项必须在同一事务中完成。
            let execution: Result<(), DispatchError> =
                frame_support::storage::with_transaction(|| {
                    if <T as Config>::OnchainFeeCharger::charge(&fee_account, action.amount)
                        .is_err()
                    {
                        return frame_support::storage::TransactionOutcome::Rollback(Err(
                            Error::<T>::InsufficientFeeReserve.into(),
                        ));
                    }
                    match <T as Config>::Currency::transfer(
                        &fee_account,
                        &main_account,
                        action.amount,
                        frame_support::traits::ExistenceRequirement::KeepAlive,
                    ) {
                        Ok(()) => frame_support::storage::TransactionOutcome::Commit(Ok(())),
                        Err(error) => {
                            frame_support::storage::TransactionOutcome::Rollback(Err(error))
                        }
                    }
                });
            execution?;

            let reserve_left = <T as Config>::Currency::free_balance(&fee_account);

            Self::deposit_event(Event::SweepToMainExecuted {
                proposal_id,
                actor_cid_number: action.actor_cid_number,
                institution_account_id: action.institution_account_id,
                amount: action.amount,
                fee: tx_fee,
                reserve_left,
            });
            Ok(())
        }

        pub(crate) fn try_execute_safety_fund_from_callback(proposal_id: u64) -> DispatchResult {
            let action = SafetyFundProposalActions::<T>::get(proposal_id)
                .ok_or(Error::<T>::SafetyFundProposalNotFound)?;

            Self::ensure_internal_business_proposal(
                proposal_id,
                NRC,
                Some(&action.actor_cid_number),
                &action.institution_account_id,
                &[action.actor_cid_number.to_vec()],
            )
            .map_err(|_| Error::<T>::SafetyFundProposalNotPassed)?;

            let safety_fund_account = T::AccountId::decode(&mut &SAFETY_FUND_ACCOUNT[..])
                .map_err(|_| Error::<T>::InstitutionAccountDecodeFailed)?;
            ensure!(
                action.actor_cid_number == nrc_cid()
                    && action.institution_account_id == safety_fund_account,
                Error::<T>::InvalidInstitution
            );

            ensure!(
                <T as Config>::InstitutionAsset::can_spend(
                    &safety_fund_account,
                    InstitutionAssetAction::NrcSafetyFundTransfer,
                ),
                Error::<T>::InstitutionSpendNotAllowed
            );

            // ── 计算手续费 ──
            let amount_u128: u128 = action.amount.saturated_into();
            let fee_u128 = primitives::fee_policy::calculate_onchain_fee(amount_u128);
            let fee: BalanceOf<T> = fee_u128.saturated_into();
            // ── 分账户余额检查：安全基金 amount + ED，费用账户 fee + ED ──
            let free = <T as Config>::Currency::free_balance(&safety_fund_account);
            let ed = <T as Config>::Currency::minimum_balance();
            let required = action
                .amount
                .checked_add(&ed)
                .ok_or(Error::<T>::SafetyFundInsufficientBalance)?;
            ensure!(free >= required, Error::<T>::SafetyFundInsufficientBalance);
            let fee_account = Self::resolve_fee_account(&action.actor_cid_number)?;
            let fee_required = fee
                .checked_add(&ed)
                .ok_or(Error::<T>::InsufficientFeeBalance)?;
            ensure!(
                <T as Config>::Currency::free_balance(&fee_account) >= fee_required,
                Error::<T>::InsufficientFeeBalance
            );

            let execution: Result<(), DispatchError> =
                frame_support::storage::with_transaction(|| {
                    if <T as Config>::OnchainFeeCharger::charge(&fee_account, action.amount)
                        .is_err()
                    {
                        return frame_support::storage::TransactionOutcome::Rollback(Err(
                            Error::<T>::InsufficientFeeBalance.into(),
                        ));
                    }
                    match <T as Config>::Currency::transfer(
                        &safety_fund_account,
                        &action.beneficiary_account_id,
                        action.amount,
                        frame_support::traits::ExistenceRequirement::KeepAlive,
                    ) {
                        Ok(()) => frame_support::storage::TransactionOutcome::Commit(Ok(())),
                        Err(_) => frame_support::storage::TransactionOutcome::Rollback(Err(
                            Error::<T>::SafetyFundInsufficientBalance.into(),
                        )),
                    }
                });
            execution?;

            Self::deposit_event(Event::SafetyFundTransferExecuted {
                proposal_id,
                fee_payer: fee_account,
                beneficiary_account_id: action.beneficiary_account_id,
                amount: action.amount,
                fee,
            });

            Ok(())
        }

        pub(crate) fn try_execute_transfer_from_callback(proposal_id: u64) -> DispatchResult {
            let raw = votingengine::Pallet::<T>::get_proposal_data(proposal_id)
                .ok_or(Error::<T>::ProposalActionNotFound)?;
            let tag = crate::MODULE_TAG;
            ensure!(
                raw.len() >= tag.len() && &raw[..tag.len()] == tag,
                Error::<T>::ProposalActionNotFound
            );
            let action = TransferAction::<T::AccountId, BalanceOf<T>, T::MaxRemarkLen>::decode(
                &mut &raw[tag.len()..],
            )
            .map_err(|_| Error::<T>::ProposalActionNotFound)?;
            let (institution_code, subject_cid_numbers) = Self::resolve_funding_authority(
                action.actor_cid_number.as_ref(),
                &action.funding_account_id,
            )?;
            Self::ensure_internal_business_proposal(
                proposal_id,
                institution_code,
                action.actor_cid_number.as_ref(),
                &action.funding_account_id,
                &subject_cid_numbers,
            )?;
            ensure!(
                <T as Config>::InstitutionAsset::can_spend(
                    &action.funding_account_id,
                    InstitutionAssetAction::MultisigTransferExecute,
                ),
                Error::<T>::InstitutionSpendNotAllowed
            );
            ensure!(action.amount > Zero::zero(), Error::<T>::ZeroAmount);
            let ed = <T as Config>::Currency::minimum_balance();
            ensure!(
                action.amount >= ed,
                Error::<T>::AmountBelowExistentialDeposit
            );
            ensure!(
                action.beneficiary_account_id != action.funding_account_id,
                Error::<T>::SelfTransferNotAllowed
            );
            ensure!(
                !<T as Config>::ProtectedSourceChecker::is_protected(
                    &action.beneficiary_account_id
                ),
                Error::<T>::BeneficiaryIsProtectedAddress
            );

            // ── 计算手续费（复用 primitives::fee_policy 唯一公式） ──
            let amount_u128: u128 = action.amount.saturated_into();
            let fee_u128 = primitives::fee_policy::calculate_onchain_fee(amount_u128);
            let fee: BalanceOf<T> = fee_u128.saturated_into();
            let free = <T as Config>::Currency::free_balance(&action.funding_account_id);
            let principal_required = action
                .amount
                .checked_add(&ed)
                .ok_or(Error::<T>::InsufficientBalance)?;
            let fee_payer = if let Some(cid_number) = action.actor_cid_number.as_ref() {
                ensure!(free >= principal_required, Error::<T>::InsufficientBalance);
                let fee_account = Self::resolve_fee_account(cid_number)?;
                let fee_required = fee
                    .checked_add(&ed)
                    .ok_or(Error::<T>::InsufficientFeeBalance)?;
                ensure!(
                    <T as Config>::Currency::free_balance(&fee_account) >= fee_required,
                    Error::<T>::InsufficientFeeBalance
                );
                fee_account
            } else {
                let required = principal_required
                    .checked_add(&fee)
                    .ok_or(Error::<T>::InsufficientBalance)?;
                ensure!(free >= required, Error::<T>::InsufficientBalance);
                action.funding_account_id.clone()
            };

            // 机构路径从费用账户扣费、从 funding_account_id 转本金；个人路径两者相同。
            // 任一失败时手续费事件、分账和本金转账全部回滚。
            let exec_result = frame_support::storage::with_transaction(|| {
                if <T as Config>::OnchainFeeCharger::charge(&fee_payer, action.amount).is_err() {
                    return frame_support::storage::TransactionOutcome::Rollback(Err(
                        Error::<T>::InsufficientFeeBalance.into(),
                    ));
                }
                match <T as Config>::Currency::transfer(
                    &action.funding_account_id,
                    &action.beneficiary_account_id,
                    action.amount,
                    ExistenceRequirement::KeepAlive,
                ) {
                    Ok(()) => frame_support::storage::TransactionOutcome::Commit(Ok(())),
                    Err(e) => frame_support::storage::TransactionOutcome::Rollback(Err(e)),
                }
            });
            exec_result?;

            Self::deposit_event(Event::<T>::TransferExecuted {
                proposal_id,
                funding_account_id: action.funding_account_id,
                fee_payer,
                beneficiary_account_id: action.beneficiary_account_id,
                amount: action.amount,
                fee,
            });
            Ok(())
        }
    }
}

// ──── 投票终态回调:把已通过的 3 组业务提案(转账/安全基金/手续费划转)落地到链上 ────
//
// 统一状态机整改后业务模块不再持有独立 vote/finalize call,提案通过(或否决)
// 由投票引擎通过 [`votingengine::InternalVoteResultCallback`] 广播回来。
// 本 Executor 按 `MODULE_TAG` 前缀 + 独立存储键认领对应业务:
// - `MODULE_TAG` 前缀 `multisig` → transfer
// - `SafetyFundProposalActions[id]` 存在 → safety_fund
// - `SweepProposalActions[id]` 存在 → sweep
//
// 失败语义:执行失败发 ExecutionFailed 事件,提案保留 PASSED 状态,快照管理员
// 可通过 VotingEngine::retry_passed_proposal 手动重试,实际权限由 votingengine 统一校验。
pub struct InternalVoteExecutor<T>(core::marker::PhantomData<T>);

impl<T: pallet::Config> InternalVoteResultCallback for InternalVoteExecutor<T> {
    fn on_internal_vote_finalized(
        proposal_id: u64,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, sp_runtime::DispatchError> {
        let is_safety_fund = SafetyFundProposalActions::<T>::contains_key(proposal_id);
        let is_sweep = SweepProposalActions::<T>::contains_key(proposal_id);
        let is_transfer = !is_safety_fund
            && !is_sweep
            && votingengine::Pallet::<T>::get_proposal_data(proposal_id)
                .map(|raw| raw.starts_with(crate::MODULE_TAG))
                .unwrap_or(false);

        if !is_transfer && !is_safety_fund && !is_sweep {
            return Ok(ProposalExecutionOutcome::Ignored); // 非本模块提案
        }

        if approved {
            let exec_result = if is_transfer {
                pallet::Pallet::<T>::try_execute_transfer_from_callback(proposal_id)
            } else if is_safety_fund {
                pallet::Pallet::<T>::try_execute_safety_fund_from_callback(proposal_id)
            } else {
                pallet::Pallet::<T>::try_execute_sweep_from_callback(proposal_id)
            };
            if let Err(_e) = exec_result {
                // 执行失败:发事件,提案保留 PASSED,供 VotingEngine 统一重试入口处理。
                if is_transfer {
                    if let Some(raw) = votingengine::Pallet::<T>::get_proposal_data(proposal_id) {
                        if raw.len() >= crate::MODULE_TAG.len()
                            && raw.starts_with(crate::MODULE_TAG)
                        {
                            if let Ok(action) = TransferAction::<
                                T::AccountId,
                                BalanceOf<T>,
                                T::MaxRemarkLen,
                            >::decode(
                                &mut &raw[crate::MODULE_TAG.len()..]
                            ) {
                                pallet::Pallet::<T>::deposit_event(
                                    pallet::Event::<T>::TransferExecutionFailed {
                                        proposal_id,
                                        funding_account_id: action.funding_account_id,
                                    },
                                );
                            }
                        }
                    }
                } else if is_safety_fund {
                    pallet::Pallet::<T>::deposit_event(
                        pallet::Event::<T>::SafetyFundExecutionFailed { proposal_id },
                    );
                } else if is_sweep {
                    pallet::Pallet::<T>::deposit_event(pallet::Event::<T>::SweepExecutionFailed {
                        proposal_id,
                    });
                }
                return Ok(ProposalExecutionOutcome::RetryableFailed);
            }
            return Ok(ProposalExecutionOutcome::Executed);
        } else {
            // 否决:清理独立存储,避免僵尸数据。
            SafetyFundProposalActions::<T>::remove(proposal_id);
            SweepProposalActions::<T>::remove(proposal_id);
        }
        Ok(ProposalExecutionOutcome::Executed)
    }

    fn on_execution_failed_terminal(proposal_id: u64) -> DispatchResult {
        // 普通转账仅依赖 ProposalData；安全基金和 sweep 还有独立动作存储，需要终态清理。
        SafetyFundProposalActions::<T>::remove(proposal_id);
        SweepProposalActions::<T>::remove(proposal_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
