//! # 投票引擎 (votingengine)
//!
//! 投票基础设施模块，统一承载四类投票流程：
//! - **内部投票**（INTERNAL）：按业务模块绑定的机构岗位主体或个人多签主体投票；
//!   机构使用岗位有效选民快照，个人多签使用独立管理员快照；赞成 ≥ 阈值提前通过，
//!   剩余票不足达到阈值提前否决，30 天超时兜底否决。

//! - **联合投票**（JOINT）：NRC/PRC 委员与 PRB 董事岗位有效选民按机构票权加权投票，
//!   105 票全票通过直接执行，任一机构反对立即进入联合公投，30 天超时进入联合公投。

//! - **立法机关表决**（LEGISLATION）：由 legislation-vote sub-pallet 承载代表机构表决、
//!   特别案/核心修宪立法公投、行政签署、三人会签和护宪终审。
//!
//! - **选举投票**（ELECTION）：由 election-vote sub-pallet 承载普选/互选选人流程，
//!   核心只提供提案生命周期、超时结算分发、回调和清理状态机。
//!
//! 关键机制：
//! - **授权主体快照锁定**：目标模型按 `VotePlan` 锁定岗位有效任职或个人多签管理员名单。
//! - **联合提案发起权**：由业务模块绑定的完整机构岗位主体决定，不按机构全体管理员决定。
//!
//! 通过 trait 为上层治理模块提供标准化能力：
//! - `InternalVoteEngine` / `JointVoteEngine`：业务模块发起提案的内部入口;
//!   投票走对应 sub-pallet(`internal-vote::cast` / `joint-vote::cast_admin` /
//!   `joint-vote::cast_referendum`)的公开 extrinsic。
//! - `InternalVoteResultCallback` / `JointVoteResultCallback`:内部/联合提案
//!   完成投票判定时,投票引擎按统一状态机调用业务 executor。
//!   业务模块只返回统一执行结果，不再直接推进投票引擎状态；PASSED 表示执行授权/可重试态。
//! - 自动超时结算、投票判定与业务执行解耦、执行错误指数退避/dead-letter、
//!   90 天延迟分块清理。

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;
pub mod cleanup;
pub mod data;
mod execution;
mod expiry;
pub mod id;
pub mod index;
mod lifecycle;
pub mod limit;
mod maintenance;
pub mod mutex;
pub mod snapshot;
pub mod tracks;
pub mod traits;
pub mod types;
pub mod weights;

pub use citizen_identity::{CitizenSubject, PopulationData, PopulationScope};
pub use pallet::*;
pub use tracks::*;
pub use traits::*;
pub use types::*;

use core::marker::PhantomData;
use frame_support::{dispatch::DispatchResult, traits::Get, weights::Weight};

/// 从 Runtime 最大区块权重派生独立维护管线预算。
///
/// 该配置类型不属于 benchmark 生成物，避免重生 `weights.rs` 时被覆盖。
pub struct BlockWeightFraction<T, const DIVISOR: u64>(PhantomData<T>);

impl<T: frame_system::Config, const DIVISOR: u64> Get<Weight> for BlockWeightFraction<T, DIVISOR> {
    fn get() -> Weight {
        let divisor = DIVISOR.max(1);
        let max = <T as frame_system::Config>::BlockWeights::get().max_block;
        Weight::from_parts(max.ref_time() / divisor, max.proof_size() / divisor)
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    use frame_support::{pallet_prelude::*, Blake2_128Concat, Twox64Concat};
    use frame_system::pallet_prelude::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        #[pallet::constant]
        type MaxVoteNonceLength: Get<u32>;

        #[pallet::constant]
        type MaxVoteSignatureLength: Get<u32>;

        /// 每个区块自动处理的“到期提案”上限，避免 on_initialize 无界增长。
        #[pallet::constant]
        type MaxAutoFinalizePerBlock: Get<u32>;

        /// 自动超时终结管线的独立区块权重预算。
        type MaxAutoFinalizeWeightPerBlock: Get<Weight>;

        /// 已通过提案业务执行管线的独立区块权重预算。
        type MaxExecutionWeightPerBlock: Get<Weight>;

        /// 延迟激活和分块清理管线共享的独立区块权重预算。
        type MaxCleanupWeightPerBlock: Get<Weight>;

        /// 单个到期区块允许挂载的提案 ID 上限，避免 expiry 桶无界增长。
        #[pallet::constant]
        type MaxProposalsPerExpiry: Get<u32>;

        /// 单个提案最多持有的内部互斥锁 binding 数量。
        #[pallet::constant]
        type MaxInternalProposalMutexBindings: Get<u32>;

        /// 每个主体最多允许同时存在的活跃提案数量。
        ///
        /// 机构类主体以 CID 计数,个人多签以账户计数。
        #[pallet::constant]
        type MaxActiveProposals: Get<u32>;

        /// 每个区块最多执行多少个清理步骤，避免历史提案清理拖垮 on_initialize。
        #[pallet::constant]
        type MaxCleanupStepsPerBlock: Get<u32>;

        /// 每块最多把多少个已到期任务从延迟 FIFO 激活到就绪 FIFO。
        #[pallet::constant]
        type MaxCleanupActivationsPerBlock: Get<u32>;

        /// 每个清理步骤最多删除多少条前缀项。
        #[pallet::constant]
        type CleanupKeysPerStep: Get<u32>;

        /// 提案业务数据最大长度（字节），各业务模块序列化后的数据不超过此限制。
        #[pallet::constant]
        type MaxProposalDataLen: Get<u32>;

        /// 提案大对象数据最大长度（字节），用于 runtime wasm 等大载荷。
        #[pallet::constant]
        type MaxProposalObjectLen: Get<u32>;

        /// 业务模块标识最大长度，用于 ProposalOwner 绑定。
        #[pallet::constant]
        type MaxModuleTagLen: Get<u32>;

        /// 自动执行失败后允许的最大手动失败次数。
        #[pallet::constant]
        type MaxManualExecutionAttempts: Get<u32>;

        /// 自动执行失败后等待管理员手动执行的宽限区块数。
        #[pallet::constant]
        type ExecutionRetryGraceBlocks: Get<BlockNumberFor<Self>>;

        /// 单个区块最多处理多少个执行重试超时提案。
        #[pallet::constant]
        type MaxExecutionRetryDeadlinesPerBlock: Get<u32>;

        /// 每块最多处理多少个因 deadline 重排失败进入待处理队列的 retry 提案。
        #[pallet::constant]
        type MaxPendingRetryExpirationsPerBlock: Get<u32>;

        type CitizenIdentityReader: CitizenIdentityReader<Self::AccountId>;

        type JointVoteResultCallback: JointVoteResultCallback;
        /// 内部投票终态回调(对称于 `JointVoteResultCallback`)。
        /// Runtime 用 tuple 注册多个业务模块的 Executor,投票引擎在提案进入
        /// `STATUS_PASSED` / `STATUS_REJECTED` 时广播到每个成员。
        type InternalVoteResultCallback: InternalVoteResultCallback;
        type InternalAdminProvider: InternalAdminProvider<Self::AccountId>;
        /// 单个资格快照最大账户数；上限与机构 admins 最大人数一致。
        #[pallet::constant]
        type MaxAdminsPerInstitution: Get<u32>;

        /// 时间源，用于提案 ID 编码年份。
        type TimeProvider: frame_support::traits::UnixTime;

        type WeightInfo: crate::weights::WeightInfo;

        /// 四类投票 Track 的统一生命周期路由。
        ///
        /// Runtime 使用递归 tuple 注册 sub-pallet；核心不再维护 mode/stage 分支。
        type TrackHandlers: crate::tracks::ProposalTracks<BlockNumberFor<Self>, Self::AccountId>;

        /// 立法投票终态业务回调(ADR-027),由 legislation-yuan 业务壳实现。
        /// 核心在 PROPOSAL_KIND_LEGISLATION 提案达终态时按 kind 广播。第1步装 `()`。
        type LegislationVoteResultCallback: LegislationVoteResultCallback;

        /// 选举投票终态业务回调。当前可接 election-vote 自身生成结果快照,后续接 admins 写入器。
        type ElectionVoteResultCallback: ElectionVoteResultCallback;
    }

    use crate::weights::WeightInfo;

    pub type VoteNonceOf<T> = BoundedVec<u8, <T as Config>::MaxVoteNonceLength>;
    pub type VoteSignatureOf<T> = BoundedVec<u8, <T as Config>::MaxVoteSignatureLength>;

    /// VotingEngine 主 pallet on-chain storage 版本。
    ///
    /// 布局:提案主键纯单调 u64 + ProposalDisplayId 展示号 +
    /// ProposalsByCode/Institution/Owner/Year 4 张反向索引,创世直写,无历史回填。
    pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    /// 当前提案年份（用于年度计数器重置）。
    #[pallet::storage]
    pub type CurrentProposalYear<T> = StorageValue<_, u16, ValueQuery>;

    /// 当前年份内的提案计数器（每年从 0 开始）。
    #[pallet::storage]
    pub type YearProposalCounter<T> = StorageValue<_, u32, ValueQuery>;

    /// 主键计数器:下一次 `allocate_proposal_id` 要返回的 u64。
    ///
    /// 双层 ID 设计:主键 `proposal_id` 全局单调累加,跨业务/跨年/跨机构唯一;
    /// 展示号 `(year, seq_in_year)` 单独存于 `ProposalDisplayId[id]` 反查表,
    /// 渲染层基于其拼成 "2026000123" 风格,**与主键解耦**。
    #[pallet::storage]
    #[pallet::getter(fn next_proposal_id)]
    pub type NextProposalId<T> = StorageValue<_, u64, ValueQuery>;

    /// 全局提案表：proposal_id → 提案元数据（类型/阶段/状态/起止区块/机构等）。
    /// 由 `create_internal_proposal` 写入，`set_status_and_emit` 更新状态，超时清理自动删除。
    #[pallet::storage]
    #[pallet::getter(fn proposals)]
    pub type Proposals<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        Proposal<BlockNumberFor<T>, T::AccountId>,
        OptionQuery,
    >;

    /// 回调执行作用域：只在拒绝回调或异步执行队列调用业务回调期间临时存在。
    ///
    /// 生产业务模块通过回调返回 `ProposalExecutionOutcome`；该作用域保护
    /// 测试和回调执行入口，避免非回调路径绕过最终事件和互斥锁释放逻辑。
    #[pallet::storage]
    pub type CallbackExecutionScopes<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, (), OptionQuery>;

    /// 以“阶段截止区块”索引提案，用于 on_initialize 自动超时结算。
    #[pallet::storage]
    #[pallet::getter(fn proposals_by_expiry)]
    pub type ProposalsByExpiry<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        BlockNumberFor<T>,
        BoundedVec<u64, <T as Config>::MaxProposalsPerExpiry>,
        ValueQuery,
    >;

    /// 自动结算游标：记录上个区块未处理完的过期桶。
    #[pallet::storage]
    #[pallet::getter(fn pending_expiry_bucket)]
    pub type PendingExpiryBucket<T: Config> = StorageValue<_, BlockNumberFor<T>, OptionQuery>;

    /// 分块清理游标：按提案维度逐步清理历史投票状态，避免 finalize 路径单次无界删除。
    #[pallet::storage]
    #[pallet::getter(fn pending_cleanup_stage)]
    pub type PendingProposalCleanups<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, PendingCleanupStage, OptionQuery>;

    /// 个人多签管理员快照。
    /// 机构内部/联合提案不得写入或读取本 storage，必须使用岗位投票人快照；
    /// 个人多签仍以个人账户为 key。
    #[pallet::storage]
    pub type AdminSnapshot<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        ProposalSubject<T::AccountId>,
        BoundedVec<T::AccountId, T::MaxAdminsPerInstitution>,
        OptionQuery,
    >;

    /// 提案投票计划。联合提案必须在创建事务内绑定且只能绑定一次。
    #[pallet::storage]
    #[pallet::getter(fn proposal_vote_plan)]
    pub type ProposalVotePlans<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, crate::types::VotePlanOf<T::AccountId>, OptionQuery>;

    /// 完整授权主体的投票人快照。
    ///
    /// 机构主体的 key 同时包含 CID 与岗位码，禁止退化成裸 CID 管理员集合。
    #[pallet::storage]
    pub type VoterSnapshot<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        crate::types::AuthorizationSubject<
            crate::types::CidNumber,
            crate::types::RoleCode,
            T::AccountId,
        >,
        BoundedVec<T::AccountId, T::MaxAdminsPerInstitution>,
        OptionQuery,
    >;

    /// 提案创建时冻结的机构岗位票据总数。
    ///
    /// 数量等于该机构全部投票岗位快照中的有效任职席位之和，不按账户去重；
    /// 具体资格始终由 `VoterSnapshot` 中的完整 CID + 岗位码校验。
    #[pallet::storage]
    pub type InstitutionTicketCountSnapshot<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        crate::types::CidNumber,
        u32,
        OptionQuery,
    >;

    /// 提案业务数据（由各业务模块序列化后写入，投票引擎统一存储和清理）。
    #[pallet::storage]
    #[pallet::getter(fn proposal_data)]
    pub type ProposalData<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, BoundedVec<u8, T::MaxProposalDataLen>, OptionQuery>;

    /// 提案 owner：proposal_id → 业务模块 MODULE_TAG。
    ///
    /// ProposalOwner 是投票引擎分发自动执行、手动重试和取消的唯一归属来源。
    /// 业务模块不再只依赖 ProposalData 前缀自认领，避免跨模块覆写后静默跳过。
    #[pallet::storage]
    #[pallet::getter(fn proposal_owner)]
    pub type ProposalOwner<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, BoundedVec<u8, T::MaxModuleTagLen>, OptionQuery>;

    /// 投票引擎根据 citizen-identity 四级人口数据生成的提案人口快照。
    /// 身份模块只提供人口数据和历史资格判断，不保存本投票快照。
    #[pallet::storage]
    #[pallet::getter(fn proposal_population_snapshot)]
    pub type ProposalPopulationSnapshots<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        crate::types::ProposalPopulationSnapshot<BlockNumberFor<T>>,
        OptionQuery,
    >;

    /// 自动执行失败后的可重试状态。
    #[pallet::storage]
    #[pallet::getter(fn proposal_execution_retry_state)]
    pub type ProposalExecutionRetryStates<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, ExecutionRetryState<BlockNumberFor<T>>, OptionQuery>;

    /// 通过判定与业务执行解耦后的待执行队列。
    #[pallet::storage]
    pub type PendingProposalExecutions<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        crate::types::PendingExecutionState<BlockNumberFor<T>>,
        OptionQuery,
    >;

    /// 自动业务执行达到失败上限后，等待补齐终态副作用的独立队列。
    ///
    /// 提案已经进入 EXECUTION_FAILED，后续只能登记延迟清理、释放业务侧 pending
    /// 状态和执行 Track 终态钩子，绝不能再次调用通过提案的业务执行回调。
    #[pallet::storage]
    pub type PendingTerminalFinalizations<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        crate::types::PendingExecutionState<BlockNumberFor<T>>,
        OptionQuery,
    >;

    /// 终态副作用连续失败达到上限后的 dead-letter 记录，供治理人工检查。
    #[pallet::storage]
    pub type TerminalFinalizationDeadLetters<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, u8, OptionQuery>;

    /// 自动超时终结失败后的有限退避状态。
    #[pallet::storage]
    pub type AutoFinalizeRetryStates<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        crate::types::PendingExecutionState<BlockNumberFor<T>>,
        OptionQuery,
    >;

    /// 自动超时终结连续失败达到上限或重试桶已满后的 dead-letter 记录。
    #[pallet::storage]
    pub type AutoFinalizeDeadLetters<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, u8, OptionQuery>;

    /// 执行重试 deadline 重排失败后的待处理队列。
    ///
    /// 只要提案仍处于 PASSED + retry state，就必须保留一个可观测入口；
    /// 不能因为 deadline 桶连续满而丢失后续 on_initialize 处理机会。
    #[pallet::storage]
    pub type PendingExecutionRetryExpirations<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, BlockNumberFor<T>, OptionQuery>;

    /// 执行失败终态通知失败后的待处理队列。
    ///
    /// 提案已经进入 EXECUTION_FAILED 终态时，业务模块释放 pending 锁等
    /// 清理通知不能被静默吞掉；失败后保留 proposal_id，后续 on_initialize 有界重试。
    #[pallet::storage]
    pub type PendingTerminalCleanups<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, (), OptionQuery>;

    /// 执行重试超时队列：retry_deadline → proposal_id 列表。
    #[pallet::storage]
    #[pallet::getter(fn execution_retry_deadlines)]
    pub type ExecutionRetryDeadlines<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        BlockNumberFor<T>,
        BoundedVec<u64, T::MaxExecutionRetryDeadlinesPerBlock>,
        ValueQuery,
    >;

    /// 提案对象层元数据（对象类型 / 长度 / 哈希），由投票引擎统一存储和清理。
    #[pallet::storage]
    #[pallet::getter(fn proposal_object_meta)]
    pub type ProposalObjectMeta<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, ProposalObjectMetadata<T::Hash>, OptionQuery>;

    /// 提案对象层原始数据（例如 runtime wasm），由投票引擎统一存储和清理。
    #[pallet::storage]
    #[pallet::getter(fn proposal_object)]
    pub type ProposalObject<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, BoundedVec<u8, T::MaxProposalObjectLen>, OptionQuery>;

    /// 提案辅助元数据（创建时间、通过时间，由投票引擎统一存储和清理）。
    #[pallet::storage]
    #[pallet::getter(fn proposal_meta)]
    pub type ProposalMeta<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, ProposalMetadata<BlockNumberFor<T>>, OptionQuery>;

    /// 延迟清理 FIFO：序号 → 清理到期区块与 proposal_id。
    ///
    /// 所有任务使用同一固定保留期，因此写入顺序即到期顺序；无需有界区块桶和顺延扫描。
    #[pallet::storage]
    pub type ScheduledCleanups<T: Config> =
        StorageMap<_, Twox64Concat, u64, ScheduledCleanup<BlockNumberFor<T>>, OptionQuery>;

    #[pallet::storage]
    pub type ScheduledCleanupHead<T> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    pub type ScheduledCleanupTail<T> = StorageValue<_, u64, ValueQuery>;

    /// 已到期清理任务的公平 FIFO。每完成一个步骤，未结束任务排回队尾。
    #[pallet::storage]
    pub type PendingCleanupQueue<T> = StorageMap<_, Twox64Concat, u64, u64, OptionQuery>;

    #[pallet::storage]
    pub type PendingCleanupQueueHead<T> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    pub type PendingCleanupQueueTail<T> = StorageValue<_, u64, ValueQuery>;

    /// 每个主体的活跃提案 ID 列表（全局管控，不区分提案类型，上限由 Runtime 配置）。
    ///
    /// 机构类主体 key=ProposalSubject::InstitutionCid(cid_number);
    /// 个人多签 key=ProposalSubject::PersonalAccount(account)。
    #[pallet::storage]
    #[pallet::getter(fn active_proposals_by_subject)]
    pub type ActiveProposalsBySubject<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        ProposalSubject<T::AccountId>,
        BoundedVec<u64, T::MaxActiveProposals>,
        ValueQuery,
    >;

    /// 同一主体的内部提案互斥状态。
    #[pallet::storage]
    #[pallet::getter(fn internal_proposal_mutex)]
    pub type InternalProposalMutexes<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        ProposalSubject<T::AccountId>,
        InternalProposalMutexState,
        OptionQuery,
    >;

    /// 提案持有的互斥锁列表，用于终态或联合投票进入联合公投阶段时释放。
    #[pallet::storage]
    #[pallet::getter(fn proposal_mutex_bindings)]
    pub type ProposalMutexBindings<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        BoundedVec<InternalProposalMutexBinding<T::AccountId>, T::MaxInternalProposalMutexBindings>,
        ValueQuery,
    >;

    // ──── 双层 ID 与反向索引 ────

    /// 提案展示号:`proposal_id → (year, seq_in_year)`。
    ///
    /// 主键纯单调 u64 实质无上限;展示号在创建提案时同步写入,渲染走该表查询。
    /// 客户端"2026-#000123"格式由前端基于本表内容拼接,链端不持有展示格式。
    #[pallet::storage]
    pub type ProposalDisplayId<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, ProposalDisplayMeta, OptionQuery>;

    /// 反向索引:institution_code → 该机构码下所有提案 ID。
    /// 客户端按"国家储委会/省储委会/省储行/多签"分类查询时直接迭代该表,无需扫全表。
    #[pallet::storage]
    pub type ProposalsByCode<T: Config> =
        StorageDoubleMap<_, Twox64Concat, InstitutionCode, Twox64Concat, u64, (), OptionQuery>;

    /// 反向索引:cid_number → 该机构主体所有关联提案 ID。
    /// 机构详情页和订阅提案流以 CID 为唯一真源。
    #[pallet::storage]
    pub type ProposalsByCid<T: Config> =
        StorageDoubleMap<_, Twox64Concat, CidNumber, Twox64Concat, u64, (), OptionQuery>;

    /// 反向索引:业务模块 MODULE_TAG → 该模块所有提案 ID。
    /// "只看 runtime 升级提案 / 只看决议销毁提案"等视图走该表。
    #[pallet::storage]
    pub type ProposalsByOwner<T: Config> = StorageDoubleMap<
        _,
        Twox64Concat,
        BoundedVec<u8, T::MaxModuleTagLen>,
        Twox64Concat,
        u64,
        (),
        OptionQuery,
    >;

    /// 反向索引:年份 → 该年所有提案 ID。
    /// 历史年份归档查询走该表,不再依赖主键的"年份编码"心智。
    #[pallet::storage]
    pub type ProposalsByYear<T: Config> =
        StorageDoubleMap<_, Twox64Concat, u16, Twox64Concat, u64, (), OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 提案已创建，记录类型、阶段和截止区块。
        ProposalCreated {
            proposal_id: u64,
            kind: u8,
            stage: u8,
            end: BlockNumberFor<T>,
        },
        /// 联合投票阶段非全票通过或超时，提案推进到联合公投阶段。
        ProposalAdvancedToReferendum {
            proposal_id: u64,
            referendum_end: BlockNumberFor<T>,
            eligible_total: u64,
        },
        /// 投票阶段完成或执行状态变化；PASSED 是执行授权/可重试态，不是终态。
        ProposalFinalized { proposal_id: u64, status: u8 },
        /// 自动执行失败，提案进入 PASSED 可重试态。
        ProposalExecutionRetryScheduled {
            proposal_id: u64,
            retry_deadline: BlockNumberFor<T>,
        },
        /// 管理员手动执行已尝试。
        ProposalExecutionRetried {
            proposal_id: u64,
            manual_attempts: u8,
            outcome: u8,
        },
        /// PASSED 可重试提案超过宽限期，转入执行失败终态。
        ProposalExecutionRetryExpired { proposal_id: u64 },
        /// retry deadline 无法重新登记到 deadline 桶，已进入待处理重试队列。
        ProposalExecutionRetryExpirationQueued {
            proposal_id: u64,
            retry_deadline: BlockNumberFor<T>,
        },
        /// 执行失败终态通知业务模块失败，已进入待处理重试队列。
        ProposalTerminalCleanupQueued { proposal_id: u64 },
        /// 待处理的执行失败终态通知已补偿完成。
        ProposalTerminalCleanupCompleted { proposal_id: u64 },
        /// 管理员取消 PASSED 可重试提案，转入执行失败终态。
        ProposalExecutionCancelled { proposal_id: u64 },
        /// 通过提案已进入异步业务执行队列。
        ProposalExecutionQueued { proposal_id: u64 },
        /// 回调错误后已按指数退避登记下一次执行。
        ProposalExecutionDeferred {
            proposal_id: u64,
            attempts: u8,
            next_attempt_at: BlockNumberFor<T>,
        },
        /// 回调连续错误达到上限，已进入执行失败终态。
        ProposalExecutionDeadLettered { proposal_id: u64, attempts: u8 },
        /// EXECUTION_FAILED 的终态副作用失败，已按指数退避。
        ProposalTerminalFinalizationDeferred {
            proposal_id: u64,
            attempts: u8,
            next_attempt_at: BlockNumberFor<T>,
        },
        /// EXECUTION_FAILED 的终态副作用连续失败，已进入独立 dead-letter。
        ProposalTerminalFinalizationDeadLettered { proposal_id: u64, attempts: u8 },
        /// 自动超时终结失败，已退避到后续区块重试。
        ProposalAutoFinalizeDeferred {
            proposal_id: u64,
            attempts: u8,
            next_attempt_at: BlockNumberFor<T>,
        },
        /// 自动超时终结连续失败或重试桶已满，已进入 dead-letter。
        ProposalAutoFinalizeDeadLettered { proposal_id: u64, attempts: u8 },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// 提案不存在或已被清理。
        ProposalNotFound,
        /// 提案类型与当前操作不匹配（内部/联合）。
        InvalidProposalKind,
        /// 提案所处阶段与当前操作不匹配（内部/联合/公民）。
        InvalidProposalStage,
        /// 提案状态不允许当前操作（例如已终结的提案不可投票）。
        InvalidProposalStatus,
        /// 个人多签管理员快照缺失。
        MissingAdminSnapshot,
        /// 投票计划尚未绑定或对应岗位投票人快照为空。
        MissingVoterSnapshot,
        /// 提案已经绑定投票计划，禁止覆盖。
        VotePlanAlreadyBound,
        /// 投票计划与提案、业务数据或业务对象不一致。
        InvalidVotePlan,
        /// 机构标识不属于任何已知类型（NRC/PRC/PRB/多签）。
        InvalidInstitution,
        /// 调用者无权执行此操作（非管理员或外部 extrinsic 直接调用）。
        NoPermission,
        /// 投票已截止（当前区块 > end）。
        VoteClosed,
        /// 提案尚未到期，不可手动触发超时结算。
        VoteNotExpired,
        /// 同一身份已对该提案投过票。
        AlreadyVoted,
        /// 提案已终结，不可重复结算。
        ProposalAlreadyFinalized,
        /// 提案主键 u64 单调累加溢出(实质永不发生,1.84×10¹⁹ 上限)。
        ProposalIdOverflow,
        /// 年内累加器 u32 溢出(实质永不发生,42.9 亿/年上限)。
        YearCounterOverflow,
        /// 单个到期区块的提案数超出上限。
        TooManyProposalsAtExpiry,
        /// 该机构活跃提案数已达上限（10 个），需等待现有提案完成后再发起。
        ActiveProposalLimitReached,
        /// 同一治理账户已有管理员集合变更提案活跃，普通提案需等待其结束。
        AdminSetMutationProposalActive,
        /// 同一治理账户已有普通提案活跃，管理员更换需等待普通提案结束。
        RegularInternalProposalActive,
        /// 内部提案互斥计数溢出。
        InternalProposalMutexOverflow,
        /// 单个提案持有的互斥锁数量超出上限。
        TooManyInternalProposalMutexBindings,
        /// 管理员更换提案不是当前治理账户的独占锁 owner。
        InternalProposalMutexOwnerMismatch,
        /// 提案尚未绑定业务 owner。
        ProposalOwnerMissing,
        /// 提案 owner 与当前业务模块不匹配。
        ProposalOwnerMismatch,
        /// 提案业务数据已绑定，禁止跨模块覆写。
        ProposalDataAlreadyRegistered,
        /// 提案不是可手动执行状态。
        ProposalNotRetryable,
        /// 手动执行失败次数已达上限。
        ManualExecutionAttemptsExceeded,
        /// 手动执行宽限期已过。
        ExecutionRetryDeadlinePassed,
        /// 单个区块执行重试超时队列已满。
        TooManyExecutionRetryDeadlines,
        /// owner 模块没有明确允许取消该 PASSED 重试提案。
        ProposalCancellationNotAllowed,
        /// 终态提案的延迟清理队列连续多个目标区块已满，必须回滚终态写入。
        CleanupQueueSequenceExhausted,
        /// citizen-identity 尚未把四级有效人口完整推进到当前 UTC+8 日期。
        PopulationDataNotReady,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(n: BlockNumberFor<T>) -> Weight {
            let mut weight = Weight::zero();
            let max_auto_finalize = T::MaxAutoFinalizePerBlock::get() as usize;

            if max_auto_finalize > 0 {
                let db_weight = T::DbWeight::get();
                weight = weight.saturating_add(db_weight.reads(1));
                let mut budget = max_auto_finalize;
                let mut weight_budget = T::MaxAutoFinalizeWeightPerBlock::get();
                let pending = PendingExpiryBucket::<T>::get();
                let mut pending_has_remaining = false;

                if let Some(expiry) = pending {
                    if expiry <= n {
                        let (processed, has_remaining, processed_weight) =
                            Self::auto_finalize_expiry_bucket(expiry, n, budget, weight_budget);
                        weight = weight.saturating_add(processed_weight);
                        budget = budget.saturating_sub(processed);
                        weight_budget = weight_budget.saturating_sub(processed_weight);
                        pending_has_remaining = has_remaining;
                        if has_remaining {
                            PendingExpiryBucket::<T>::put(expiry);
                            weight = weight.saturating_add(db_weight.writes(1));
                        }
                        if !has_remaining {
                            PendingExpiryBucket::<T>::kill();
                            weight = weight.saturating_add(db_weight.writes(1));
                        }
                    }
                }

                if budget > 0 && !pending_has_remaining {
                    let (_processed, has_remaining, processed_weight) =
                        Self::auto_finalize_expiry_bucket(n, n, budget, weight_budget);
                    weight = weight.saturating_add(processed_weight);
                    if has_remaining {
                        PendingExpiryBucket::<T>::put(n);
                        weight = weight.saturating_add(db_weight.writes(1));
                    }
                }
            }

            weight = weight.saturating_add(Self::process_pending_proposal_executions(n));

            weight = weight.saturating_add(Self::process_pending_terminal_finalizations(n));

            weight = weight.saturating_add(Self::process_pending_execution_retry_expirations(n));

            weight = weight.saturating_add(Self::process_execution_retry_deadlines(n));

            weight = weight.saturating_add(Self::process_pending_terminal_cleanups());

            // 先把 90 天保留期已到的任务激活，再由公平 FIFO 执行有界清理步骤。
            let cleanup_budget = T::MaxCleanupWeightPerBlock::get();
            let scheduled_weight = cleanup::process_scheduled_cleanups::<T>(n, cleanup_budget);
            weight = weight.saturating_add(scheduled_weight);
            weight = weight.saturating_add(Self::process_pending_cleanup_steps(
                cleanup_budget.saturating_sub(scheduled_weight),
            ));

            weight
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        // 引擎核心仅承载生命周期 extrinsic(超时结算 / 重试 / 取消)。
        // mode-specific 投票 extrinsic 由各 sub-pallet 提供:
        //   - InternalVote::cast(20.0)
        //   - JointVote::cast_admin(21.0)
        //   - JointVote::cast_referendum(23.1)
        // call_index 从 3 起,0/1/2 留空。

        #[pallet::call_index(3)]
        #[pallet::weight(
            T::WeightInfo::finalize_proposal()
                .saturating_add(T::DbWeight::get().reads(1))
                .saturating_add(Pallet::<T>::track_timeout_weight(*proposal_id))
        )]
        pub fn finalize_proposal(
            origin: OriginFor<T>,
            proposal_id: u64,
        ) -> DispatchResultWithPostInfo {
            let _who = ensure_signed(origin)?;
            let proposal = Proposals::<T>::get(proposal_id).ok_or(Error::<T>::ProposalNotFound)?;

            let result = <T::TrackHandlers as crate::tracks::ProposalTracks<
                BlockNumberFor<T>,
                T::AccountId,
            >>::finalize_timeout(&proposal, proposal_id)
            .ok_or(Error::<T>::InvalidProposalStage)?;
            result?;

            // 调用注解已按 proposal_id 叠加所属 Track 权重，无需再返回 actual_weight。
            Ok(().into())
        }

        /// 统一手动执行已通过但自动执行失败的提案。
        ///
        /// 业务模块不得再各自暴露 execute_xxx 重试入口；所有手动执行
        /// 都必须经过投票引擎校验 PASSED 状态、机构岗位选民/个人管理员快照、
        /// 重试次数和宽限期。
        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::retry_passed_proposal())]
        pub fn retry_passed_proposal(origin: OriginFor<T>, proposal_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::retry_passed_proposal_inner(&who, proposal_id)
        }

        /// 统一取消已通过但无法继续执行的提案。
        ///
        /// 取消只允许 `PASSED -> EXECUTION_FAILED`，进入执行失败终态后
        /// 不再允许重试或再次取消。
        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::cancel_passed_proposal())]
        pub fn cancel_passed_proposal(
            origin: OriginFor<T>,
            proposal_id: u64,
            _reason: BoundedVec<u8, T::MaxProposalDataLen>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::cancel_passed_proposal_inner(&who, proposal_id)
        }
    }
}
