//! # 决议发行模块 (resolution-issuance)
//!
//! 本模块把决议发行治理与执行合并为一个完整业务 pallet：
//! 在同一模块内完成决议发行提案、联合投票回调、发行执行、
//! 防重放、暂停和审计维护。

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;
pub mod execution;
pub mod proposal;
#[cfg(test)]
mod tests;
pub mod validation;
pub mod weights;

pub use pallet::*;
use votingengine::JointVoteResultCallback;

/// 模块标识前缀，用于在投票引擎 ProposalData 中识别决议发行提案。
///
/// 该值是跨端识别决议发行提案的稳定业务标签。
pub const MODULE_TAG: &[u8] = b"res-iss";

#[frame_support::pallet]
pub mod pallet {
    use crate::{proposal::RecipientAmount, weights::WeightInfo};
    use codec::Decode;
    use entity_primitives::InstitutionRoleAuthorizationQuery;
    use frame_support::{pallet_prelude::*, traits::Currency};
    use frame_system::pallet_prelude::*;
    use primitives::cid::china::china_cb::CHINA_CB;
    #[cfg(feature = "std")]
    use sp_runtime::traits::Zero;
    use sp_std::vec::Vec;
    use votingengine::JointVoteEngine;

    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
    pub type ReasonOf<T> = BoundedVec<u8, <T as Config>::MaxReasonLen>;
    pub type AllocationOf<T> = BoundedVec<
        RecipientAmount<<T as frame_system::Config>::AccountId, BalanceOf<T>>,
        <T as Config>::MaxAllocations,
    >;

    /// 联合投票终结后的业务执行结果，用于回调时告知投票引擎写入最终执行状态。
    pub(crate) enum FinalizeOutcome {
        ApprovedExecutionSucceeded,
        ApprovedExecutionFailed,
        Rejected,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config + votingengine::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Currency: Currency<Self::AccountId>;

        /// 允许国家储委会或省储委会管理员发起决议发行提案。
        type ProposeOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Self::AccountId>;

        /// 统一投票引擎：本模块只创建联合提案，投票动作由投票引擎公开入口承载。
        type JointVoteEngine: JointVoteEngine<Self::AccountId>;
        /// 决议发行按“机构 CID + 显式岗位码 + 签名账户”校验提案权限。
        type InstitutionRoleAuthorization: InstitutionRoleAuthorizationQuery<Self::AccountId>;

        #[pallet::constant]
        type MaxReasonLen: Get<u32>;
        #[pallet::constant]
        type MaxAllocations: Get<u32>;
        #[pallet::constant]
        type MaxTotalIssuance: Get<BalanceOf<Self>>;
        #[pallet::constant]
        type MaxSingleIssuance: Get<BalanceOf<Self>>;

        type WeightInfo: crate::weights::WeightInfo;
    }

    /// 全新创世口径:创世即终态布局,storage 版本恒为 v1,不承载历史迁移。
    const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    /// 合法收款账户集合。决议发行只允许向该集合精确分配。
    #[pallet::storage]
    #[pallet::getter(fn allowed_recipients)]
    pub type AllowedRecipients<T: Config> =
        StorageValue<_, BoundedVec<T::AccountId, T::MaxAllocations>, ValueQuery>;

    /// 当前处于 Voting 状态的决议发行提案数量，用于阻止治理中途切换收款集合。
    #[pallet::storage]
    #[pallet::getter(fn voting_proposal_count)]
    pub type VotingProposalCount<T> = StorageValue<_, u32, ValueQuery>;

    /// proposal_id 是否已有短期执行记录，用于审计展示和维护排障。
    #[pallet::storage]
    pub type Executed<T: Config> = StorageMap<_, Twox64Concat, u64, BlockNumberFor<T>, OptionQuery>;

    /// proposal_id 是否历史上执行过。该标记永久防重放，维护清理不得删除。
    #[pallet::storage]
    pub type EverExecuted<T: Config> = StorageMap<_, Twox64Concat, u64, (), OptionQuery>;

    /// 决议发行累计执行量。
    #[pallet::storage]
    pub type TotalIssued<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub allowed_recipients: Vec<T::AccountId>,
    }

    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            let allowed_recipients = CHINA_CB
                .iter()
                .skip(1)
                .map(|node| {
                    T::AccountId::decode(&mut &node.main_account[..])
                        .expect("CHINA_CB main_account must decode to AccountId")
                })
                .collect();
            Self { allowed_recipients }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            let bounded: BoundedVec<T::AccountId, T::MaxAllocations> = self
                .allowed_recipients
                .clone()
                .try_into()
                .expect("allowed_recipients must fit MaxAllocations");
            Pallet::<T>::ensure_unique_recipients(bounded.as_slice())
                .expect("allowed_recipients must not contain duplicates");
            Pallet::<T>::ensure_recipients_in_china_cb(&bounded)
                .expect("allowed_recipients must be CHINA_CB PRC addresses");
            AllowedRecipients::<T>::put(bounded);
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        #[cfg(feature = "std")]
        fn integrity_test() {
            assert!(
                (CHINA_CB.len() as u32).saturating_sub(1) <= T::MaxAllocations::get(),
                "MaxAllocations must cover CHINA_CB recipients"
            );
            assert!(
                !T::MaxTotalIssuance::get().is_zero(),
                "MaxTotalIssuance must be greater than 0"
            );
            assert!(
                !T::MaxSingleIssuance::get().is_zero(),
                "MaxSingleIssuance must be greater than 0"
            );
            assert!(
                T::MaxSingleIssuance::get() <= T::MaxTotalIssuance::get(),
                "MaxSingleIssuance must not exceed MaxTotalIssuance"
            );
            assert!(T::MaxReasonLen::get() > 0, "MaxReasonLen must be > 0");
        }
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 决议发行提案已创建，联合投票已发起。
        ResolutionIssuanceProposed {
            proposal_id: u64,
            actor_cid_number: votingengine::types::CidNumber,
            proposer_role_code: votingengine::types::RoleCode,
            proposer_account_id: T::AccountId,
            total_amount: BalanceOf<T>,
            allocation_count: u32,
        },
        /// 联合投票已终结，approved 表示是否通过。
        JointVoteFinalized { proposal_id: u64, approved: bool },
        /// 投票通过且发行执行成功，铸币已落账。
        IssuanceExecutionTriggered {
            proposal_id: u64,
            total_amount: BalanceOf<T>,
        },
        /// 投票通过但发行执行失败，投票引擎状态会写为 STATUS_EXECUTION_FAILED。
        IssuanceExecutionFailed { proposal_id: u64 },
        /// 决议发行已经执行。
        ResolutionIssuanceExecuted {
            proposal_id: u64,
            total_amount: BalanceOf<T>,
            recipient_count: u32,
            reason_hash: T::Hash,
            allocations_hash: T::Hash,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        EmptyReason,
        EmptyAllocations,
        InvalidAllocationCount,
        DuplicateRecipient,
        InvalidRecipientSet,
        ZeroAmount,
        AllocationOverflow,
        TotalMismatch,
        ProposalNotFound,
        JointVoteCreateFailed,
        RecipientsNotConfigured,
        DuplicateAllowedRecipient,
        VotingProposalCountOverflow,
        VotingProposalCountUnderflow,
        ProposalDataStoreFailed,
        RecipientNotInChinaCb,
        AlreadyExecuted,
        TotalIssuedOverflow,
        ReasonTooLong,
        BelowExistentialDeposit,
        DepositFailed,
        ExceedsTotalIssuanceCap,
        ExceedsSingleIssuanceCap,
        ProposalNotFinalizable,
        InvalidActorCid,
        /// 发起人没有目标机构委员岗位的决议发行提案权限。
        UnauthorizedActorRole,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 创建“决议发行”联合投票提案。
        /// 本模块只提交决议发行业务内容；人口快照、联合签名、
        /// 投票资格和计票流程全部由 votingengine 负责。
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::propose_issuance())]
        pub fn propose_issuance(
            origin: OriginFor<T>,
            actor_cid_number: votingengine::types::CidNumber,
            proposer_role_code: votingengine::types::RoleCode,
            reason: ReasonOf<T>,
            total_amount: BalanceOf<T>,
            allocations: AllocationOf<T>,
        ) -> DispatchResult {
            let proposer_account_id = T::ProposeOrigin::ensure_origin(origin)?;
            Self::create_resolution_issuance_proposal(
                proposer_account_id,
                actor_cid_number,
                proposer_role_code,
                reason,
                total_amount,
                allocations,
            )
        }

        // call_index 2/3/4(set_allowed_recipients / clear_executed / set_paused)已删除:
        // 三者均以 EnsureRoot 门控,而本 runtime 无 Sudo/治理派发 Root,永久不可达=死入口。
        // 收款集合按创世固定,应急运维走开发期直升 runtime(dev-direct upgrade);
        // 不保留假的"暂停/清理/改收款"开关以免误导。留洞不复用。
    }
}

impl<T: pallet::Config> JointVoteResultCallback for pallet::Pallet<T> {
    fn on_joint_vote_finalized(
        vote_proposal_id: u64,
        approved: bool,
    ) -> Result<votingengine::ProposalExecutionOutcome, sp_runtime::DispatchError> {
        let outcome = pallet::Pallet::<T>::apply_joint_vote_result(vote_proposal_id, approved)?;
        Ok(match outcome {
            pallet::FinalizeOutcome::ApprovedExecutionSucceeded => {
                votingengine::ProposalExecutionOutcome::Executed
            }
            pallet::FinalizeOutcome::ApprovedExecutionFailed => {
                votingengine::ProposalExecutionOutcome::FatalFailed
            }
            pallet::FinalizeOutcome::Rejected => votingengine::ProposalExecutionOutcome::Executed,
        })
    }
}
