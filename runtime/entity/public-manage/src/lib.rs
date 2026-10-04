#![cfg_attr(not(feature = "std"), no_std)]

/// 模块标识前缀，用于在 ProposalData 中区分不同业务模块，防止跨模块误解码。
/// 长度 8 字节（`b"pub-mgmt"`）；admins 模块 / citizenwallet / citizenapp 三方解码必须保持一致。
pub const MODULE_TAG: &[u8] = b"pub-mgmt";

pub use pallet::*;
pub mod add;
#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;
pub mod close;
pub mod institution;
pub mod traits;
pub mod weights;

#[cfg(test)]
mod tests;

use admin_primitives::{
    is_public_admin_code, Admin, AdminAccountKind, InstitutionAdminLifecycle, InstitutionAdminQuery,
};
use codec::{Decode, Encode};
use frame_support::{
    ensure,
    pallet_prelude::*,
    traits::{Currency, ReservableCurrency},
    BoundedVec,
};
use frame_system::pallet_prelude::*;
use sp_core::sr25519::Public as Sr25519Public;
use sp_std::{collections::btree_set::BTreeSet, prelude::*};
pub use traits::{
    AccountValidator, InstitutionCidQuery, InstitutionMultisigQuery, ProtectedSourceChecker,
    RegistryAuthority, ReservedAccountGuard,
};
use votingengine::{
    types::{
        AuthorizationSubject, BusinessActionId, CidNumber, InstitutionCode, RoleCode, RoleSubject,
        VotePlanOf, VotingEngineKind,
    },
    InternalVoteEngine, InternalVoteResultCallback, ProposalExecutionOutcome,
};

pub use entity_primitives::{
    InstitutionAdminAssignment, InstitutionAssignmentSource, InstitutionAssignmentStatus,
    InstitutionGovernanceAction, InstitutionGovernanceProposal, InstitutionGovernanceResult,
    InstitutionRole, InstitutionRoleAuthorizationQuery, InstitutionRoleMutation,
    InstitutionRoleStatus, LegalRepresentative, RolePermissionOperation, RolePermissionSpec,
};
pub use institution::role::{
    InstitutionAdminAssignmentOf, InstitutionAdminAssignmentsOf, InstitutionRoleOf,
    InstitutionRolesOf, ModuleTagOf, RoleCodeOf, RolePermissionsOf,
};
pub use institution::types::{
    AddInstitutionAccountAction, CloseInstitutionAction, CreateInstitutionAccount,
    InstitutionAccountInfo, InstitutionInfo, InstitutionInitialAccount, RegisteredInstitution,
};
pub use primitives::account_derive::{AccountKind, RESERVED_NAME_FEE, RESERVED_NAME_MAIN};

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

        /// 公权机构管理员集合写入口。
        type AdminLifecycle: InstitutionAdminLifecycle<Self::AccountId, Admin<Self::AccountId>>;

        /// 兄弟机构 CID 查询入口，用于禁止同一 CID 在私权模块重复登记。
        type SiblingInstitutionQuery: InstitutionCidQuery<CidNumberOf<Self>>;

        /// 管理员统一查询入口，由 runtime 路由到公权/私权/创世管理员模块。
        type InstitutionAdminQuery: InstitutionAdminQuery<Self::AccountId>;

        /// 完整 CID 的顶层业务能力策略；未知能力必须拒绝。
        type InstitutionCapabilityPolicy: entity_primitives::InstitutionCapabilityPolicy;

        type AccountValidator: AccountValidator<Self::AccountId>;
        type ReservedAccountChecker: ReservedAccountGuard<Self::AccountId>;
        type ProtectedSourceChecker: ProtectedSourceChecker<Self::AccountId>;
        type InstitutionAsset: primitives::institution_asset::InstitutionAsset<Self::AccountId>;
        /// 操作机构账户关系查询；非零初始余额只能由同一 actor CID 的明确账户出资。
        type InstitutionQuery: entity_primitives::InstitutionMultisigQuery<Self::AccountId>;
        /// 投票回调中的链上交易费统一执行器。
        type OnchainFeeCharger: primitives::fee_policy::OnchainFeeCharger<
            Self::AccountId,
            BalanceOf<Self>,
        >;
        /// 注册局登记授权校验入口。
        ///
        /// 注册局管理员代登记/维护机构时,origin 是注册局管理员,目标 admins
        /// 是目标机构自己的管理员;二者不能再强制相同。本 trait 负责校验 FRG/CREG
        /// 对目标 CID 与机构码是否有登记权(省/市作用域由目标 CID 直接派生)。
        type RegistryAuthority: RegistryAuthority<Self::AccountId>;

        #[pallet::constant]
        type MaxAdmins: Get<u32>;

        #[pallet::constant]
        type MaxCidNumberLength: Get<u32>;

        /// 机构全称与机构账户名共用的最大字节长度。
        #[pallet::constant]
        type MaxAccountNameLength: Get<u32>;

        /// runtime 为单个机构自动生成的协议账户数量上限。
        #[pallet::constant]
        type MaxInstitutionAccounts: Get<u32>;

        type WeightInfo: crate::weights::WeightInfo;
    }

    pub type AdminsOf<T> =
        BoundedVec<<T as frame_system::Config>::AccountId, <T as Config>::MaxAdmins>;
    /// 机构原子初始化使用的管理员人员集合；姓名只展示，账户是唯一授权字段。
    pub type InstitutionAdminsInputOf<T> =
        BoundedVec<Admin<<T as frame_system::Config>::AccountId>, <T as Config>::MaxAdmins>;
    pub type InstitutionGovernanceActionOf<T> = InstitutionGovernanceAction<
        <T as frame_system::Config>::AccountId,
        Admin<<T as frame_system::Config>::AccountId>,
    >;
    pub type InstitutionGovernanceProposalOf<T> = InstitutionGovernanceProposal<
        <T as frame_system::Config>::AccountId,
        Admin<<T as frame_system::Config>::AccountId>,
    >;

    pub type CidNumberOf<T> = BoundedVec<u8, <T as Config>::MaxCidNumberLength>;
    pub type AccountNameOf<T> = BoundedVec<u8, <T as Config>::MaxAccountNameLength>;
    /// 「新增机构自定义命名账户」提案(call 7 `propose_add_institution_account`)的待新增账户名列表。
    pub type InstitutionAccountNamesOf<T> =
        BoundedVec<AccountNameOf<T>, <T as Config>::MaxInstitutionAccounts>;
    /// 机构创建时用户输入的账户初始余额列表项。
    pub type InstitutionInitialAccountOf<T> =
        InstitutionInitialAccount<AccountNameOf<T>, BalanceOf<T>>;
    /// 机构创建时用户输入的账户初始余额列表。
    pub type InstitutionInitialAccountsOf<T> =
        BoundedVec<InstitutionInitialAccountOf<T>, <T as Config>::MaxInstitutionAccounts>;
    /// 机构注册交易中保存的已派生账户项。
    pub type CreateInstitutionAccountOf<T> = CreateInstitutionAccount<
        AccountNameOf<T>,
        <T as frame_system::Config>::AccountId,
        BalanceOf<T>,
    >;
    /// 机构注册交易中保存的已派生账户列表。
    pub type CreateInstitutionAccountsOf<T> =
        BoundedVec<CreateInstitutionAccountOf<T>, <T as Config>::MaxInstitutionAccounts>;
    /// 机构级信息(链上最小集)。
    pub type InstitutionInfoOf<T> = InstitutionInfo<
        BlockNumberFor<T>,
        AccountNameOf<T>,
        CidNumberOf<T>,
        <T as frame_system::Config>::AccountId,
    >;
    /// 机构账户信息。
    pub type InstitutionAccountInfoOf<T> = InstitutionAccountInfo<
        <T as frame_system::Config>::AccountId,
        BalanceOf<T>,
        BlockNumberFor<T>,
    >;
    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    /// CID 机构登记反向索引：account_id -> { cid_number, nonce }
    #[pallet::storage]
    #[pallet::getter(fn account_registered_cid)]
    pub type AccountRegisteredCid<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        RegisteredInstitution<CidNumberOf<T>, AccountNameOf<T>>,
        OptionQuery,
    >;

    /// 机构级信息(链上最小集)：key 为 cid_number。
    ///
    /// 只保存全国可见的机构身份事实:名称(仅公权)、机构码与创建块号。
    /// 主账户/费用账户由 (cid_number, 保留名) 派生且常驻 InstitutionAccounts,不在此重复;
    /// 管理员集合长期真源在 admins 模块；机构治理阈值在本 entity 的
    /// `InstitutionGovernanceThresholds` 独立保存。投票引擎只在建案时读取岗位任职
    /// 与机构阈值并冻结提案快照，不回写机构配置。
    #[pallet::storage]
    #[pallet::getter(fn institution_of)]
    pub type Institutions<T: Config> =
        StorageMap<_, Blake2_128Concat, CidNumberOf<T>, InstitutionInfoOf<T>, OptionQuery>;

    /// 机构治理阈值唯一真源；与 admins 人数、岗位数量分别独立。
    #[pallet::storage]
    #[pallet::getter(fn institution_governance_threshold)]
    pub type InstitutionGovernanceThresholds<T: Config> =
        StorageMap<_, Blake2_128Concat, CidNumberOf<T>, u32, OptionQuery>;

    /// 机构岗位目录：同一个 `role_code` 只在本机构 CID 内唯一。
    #[pallet::storage]
    #[pallet::getter(fn institution_role_of)]
    pub type InstitutionRoles<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        CidNumberOf<T>,
        Blake2_128Concat,
        crate::institution::role::RoleCodeOf,
        crate::institution::role::InstitutionRoleOf<T>,
        OptionQuery,
    >;

    /// 机构岗位上的任职集合：岗位负责分组，记录负责绑定管理员账户和任职来源。
    #[pallet::storage]
    #[pallet::getter(fn institution_role_assignments)]
    pub type InstitutionRoleAssignments<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        CidNumberOf<T>,
        Blake2_128Concat,
        crate::institution::role::RoleCodeOf,
        crate::institution::role::RoleAssignmentsOf<T>,
        ValueQuery,
    >;

    /// 机构岗位不可变业务权限；岗位改名不得改写本表。
    #[pallet::storage]
    #[pallet::getter(fn institution_role_permissions)]
    pub type InstitutionRolePermissions<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        CidNumberOf<T>,
        Blake2_128Concat,
        crate::institution::role::RoleCodeOf,
        crate::institution::role::RolePermissionsOf<T>,
        ValueQuery,
    >;

    /// 每个机构单调递增的动态岗位码 nonce。
    #[pallet::storage]
    #[pallet::getter(fn institution_role_nonce)]
    pub type InstitutionRoleNonce<T: Config> =
        StorageMap<_, Blake2_128Concat, CidNumberOf<T>, u64, ValueQuery>;

    /// 机构内全部历史已用岗位码；岗位删除后永久保留。
    #[pallet::storage]
    #[pallet::getter(fn used_role_code)]
    pub type UsedRoleCodes<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        CidNumberOf<T>,
        Blake2_128Concat,
        crate::institution::role::RoleCodeOf,
        bool,
        ValueQuery,
    >;

    /// 机构账户表：(cid_number, account_name) -> 账户地址与激活状态。
    #[pallet::storage]
    #[pallet::getter(fn institution_account_of)]
    pub type InstitutionAccounts<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        CidNumberOf<T>,
        Blake2_128Concat,
        AccountNameOf<T>,
        InstitutionAccountInfoOf<T>,
        OptionQuery,
    >;

    /// 公权机构多签当前进行中的关闭提案 ID（防止并发注销提案）。
    /// 发起 propose_close 时写入，execute_close 成功或执行失败后清除。
    /// PendingCloseProposal 分两份:个人侧在 personal-manage 自持,
    /// 机构侧由本表承载,作用域为公权机构多签账户。
    #[pallet::storage]
    #[pallet::getter(fn institution_pending_close)]
    pub type InstitutionPendingClose<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u64, OptionQuery>;

    /// 公权机构当前进行中的新增账户提案 ID（防止并发新增提案）。
    /// 与关闭账户的 `InstitutionPendingClose` 对称,但新增账户在落库前无账户地址可作键,
    /// 故按机构 CID 号锁定:同一机构同一时刻只允许一笔进行中的新增账户提案。
    /// 发起 propose_add 时写入,执行成功、否决或执行失败终态后清除。
    #[pallet::storage]
    #[pallet::getter(fn institution_pending_add)]
    pub type InstitutionPendingAdd<T: Config> =
        StorageMap<_, Blake2_128Concat, CidNumberOf<T>, u64, OptionQuery>;

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
        /// 机构关闭提案已发起。
        InstitutionCloseProposed {
            proposal_id: u64,
            account_id: T::AccountId,
            proposer_account_id: T::AccountId,
            beneficiary_account_id: T::AccountId,
        },
        /// 机构关闭投票已提交。
        InstitutionCloseVoteSubmitted {
            proposal_id: u64,
            who: T::AccountId,
            approve: bool,
        },
        /// 机构关闭成功(投票通过,余额转出)。
        InstitutionClosed {
            proposal_id: u64,
            account_id: T::AccountId,
            fee_payer: T::AccountId,
            beneficiary_account_id: T::AccountId,
            amount: BalanceOf<T>,
            fee: BalanceOf<T>,
        },
        /// 机构关闭执行失败。
        InstitutionCloseExecutionFailed {
            proposal_id: u64,
            account_id: T::AccountId,
        },
        /// 机构注册创建成功：机构、账户和管理员集合均已激活。
        InstitutionCreated {
            cid_number: CidNumberOf<T>,
            main_account: T::AccountId,
            account_count: u32,
            initial_total: BalanceOf<T>,
            fee: BalanceOf<T>,
        },
        /// 已完成的业务结果原子更新机构岗位、任职和法定代表人。
        /// admins 是独立授权真源，治理结果不得反向生成或覆盖管理员。
        InstitutionGovernanceApplied {
            cid_number: CidNumberOf<T>,
            role_mutations: u32,
            assignment_changes: u32,
            admins_len: u32,
            legal_representative_updated: bool,
            result_source_ref: crate::institution::role::AssignmentSourceRefOf,
        },
        /// 机构内部治理提案已创建，后续由内部投票引擎计票和回调执行。
        InstitutionGovernanceProposed {
            proposal_id: u64,
            cid_number: CidNumberOf<T>,
            proposer_account_id: T::AccountId,
        },
        /// 注册局已直接登记目标机构管理员集合。
        InstitutionAdminsRegistered {
            cid_number: CidNumberOf<T>,
            admins_len: u32,
            submitter: T::AccountId,
        },
        /// 机构信息(全称/简称)已更新。
        InstitutionInfoUpdated {
            cid_number: CidNumberOf<T>,
            cid_full_name: AccountNameOf<T>,
            cid_short_name: AccountNameOf<T>,
            submitter: T::AccountId,
        },
        /// 机构新增账户提案已发起,后续由内部投票引擎计票和回调执行。
        InstitutionAccountAddProposed {
            proposal_id: u64,
            cid_number: CidNumberOf<T>,
            proposer_account_id: T::AccountId,
        },
        /// 已给存量机构新增账户(投票通过,finalizer 落库)。
        InstitutionAccountAdded {
            cid_number: CidNumberOf<T>,
            account_name: AccountNameOf<T>,
            account_id: T::AccountId,
            submitter: T::AccountId,
        },
        /// 机构新增账户执行失败。
        InstitutionAddAccountExecutionFailed {
            proposal_id: u64,
            cid_number: CidNumberOf<T>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// 参数不完整
        IncompleteParameters,
        /// 账户非法
        InvalidAccount,
        /// 账户为制度保留账户，不允许注册
        AccountReserved,
        /// 账户已存在（已初始化）
        AccountAlreadyExists,
        /// 管理员重复
        DuplicateAdmin,
        /// 管理员姓为空。
        InvalidFamilyName,
        /// 管理员名为空。
        InvalidGivenName,
        /// 阈值不合法
        InvalidThreshold,
        /// 金额不足
        InsufficientAmount,
        /// 初始余额非零时缺少明确资金账户。
        FundingAccountRequired,
        /// 初始余额为零时不得携带无实际用途的资金账户。
        UnexpectedFundingAccount,
        /// 资金账户不属于操作机构或不允许执行机构创建入金。
        InvalidFundingAccount,
        /// 创建金额低于最小门槛
        CreateAmountBelowMinimum,
        /// 机构账户初始余额低于最小门槛
        AccountInitialAmountBelowMinimum,
        /// 注销时账户余额低于最小门槛
        CloseBalanceBelowMinimum,
        /// 权限不足
        PermissionDenied,
        /// 注册局无权登记目标机构
        RegistryAuthorityDenied,
        /// 管理员数量不合法（必须 >=2）
        InvalidAdminsLen,
        /// 管理员数量与列表长度不一致
        AdminsLenMismatch,
        /// 机构账户管理员机构码只能是公权/私权法人机构码。
        /// 非法人必须由 CID 上层按所属法人归属显式路由。
        InvalidInstitutionCode,
        /// 多签账户不存在
        AccountNotFound,
        /// 注销收款账户非法（不允许等于 account_id）
        InvalidBeneficiary,
        /// 资金转出源地址受保护，不允许转出
        ProtectedSource,
        /// CID ID 重复登记
        CidAlreadyRegistered,
        /// CID ID 为空
        EmptyCidNumber,
        /// CID 号格式或机构码家族非法
        InvalidCidNumber,
        /// 目标机构不存在。
        InstitutionNotFound,
        /// 镇行政区机构必须携带 3 字节 town_code,非镇行政区机构必须为空。
        InvalidTownCode,
        /// 无法将派生地址转换为账户ID
        DerivedAccountDecodeFailed,
        /// 账户仍有保留余额，不允许注销
        ReservedBalanceRemaining,
        /// nonce 已耗尽
        NonceOverflow,
        /// runtime 配置不合法
        InvalidRuntimeConfig,
        /// 提案业务数据未找到
        ProposalActionNotFound,
        /// 转账失败
        TransferFailed,
        /// 管理员非本提案管理员
        UnauthorizedAdmin,
        /// 机构账户名为空
        EmptyAccountName,
        /// 法定代表人公开姓名为空
        EmptyLegalRepresentativeName,
        /// 法定代表人公民 CID 为空
        EmptyLegalRepresentativeCidNumber,
        /// 机构级创建缺少主账户
        MissingMainAccount,
        /// 机构级创建缺少费用账户
        MissingFeeAccount,
        /// 机构级创建账户名重复
        DuplicateAccountName,
        /// 机构已经存在
        InstitutionAlreadyExists,
        /// propose_close 校验:仅机构地址可走本入口(个人地址转 personal-manage)。
        NotInstitutionAccount,
        /// 机构账户列表为空
        EmptyInstitutionAccounts,
        /// 机构账户数量超过上限
        TooManyInstitutionAccounts,
        /// 初始余额累计溢出
        InitialAmountOverflow,
        /// 手续费扣取失败
        FeeWithdrawFailed,
        /// 注销后转账金额低于 ED
        CloseTransferBelowED,
        /// 该多签账户已有进行中的关闭提案，不允许重复发起
        CloseAlreadyPending,
        /// 该机构已有进行中的新增账户提案，不允许重复发起
        AddAlreadyPending,
        /// 账户名占用保留角色名（"主账户"/"费用账户" 必须走 Role::Main/Fee，
        /// 禁止作为 Role::Named 的自定义命名参数）
        ReservedAccountName,
        /// sr25519 签名长度必须恰好为 64 字节
        MalformedSignature,
        /// 创世写入的封存公权机构永不可注销关闭
        CannotCloseProtectedInstitution,
        /// 治理机构(国家储委会/省储委会/省储行)永不可注销关闭
        CannotCloseGovernance,
        /// 机构创建必须至少定义一个岗位。
        InstitutionRolesEmpty,
        /// 机构创建必须至少绑定一条管理员任职。
        InstitutionAssignmentsEmpty,
        /// 岗位所属 CID 与创建目标不一致。
        RoleCidMismatch,
        /// 岗位代码为空或超过边界。
        InvalidRoleCode,
        /// 岗位名称为空。
        InvalidRoleName,
        /// 初始岗位必须直接处于有效状态。
        InitialRoleMustBeActive,
        /// 同一机构内岗位代码重复。
        DuplicateRoleCode,
        /// 同一机构内岗位名称重复；同名席位必须归属于同一个岗位码。
        DuplicateRoleName,
        /// 任职所属 CID 与创建目标不一致。
        AssignmentCidMismatch,
        /// 任职来源与当前写入流程不一致。
        InvalidAssignmentSource,
        /// 初始任职必须直接处于有效状态。
        InitialAssignmentMustBeActive,
        /// 任职引用的岗位不存在。
        AssignmentRoleNotFound,
        /// 必须设置任期的岗位没有合法任期。
        InvalidAssignmentTerm,
        /// 无任期岗位携带了任期值。
        UnexpectedAssignmentTerm,
        /// 同一管理员在同一岗位存在重复任职。
        DuplicateAssignment,
        /// 初始岗位没有任何管理员任职。
        RoleHasNoAssignment,
        /// 任职去重后的管理员数量超过机构上限。
        TooManyInstitutionAdmins,
        /// 治理结果目标不是机构主账户或与机构码不匹配。
        InvalidAssignmentResultInstitution,
        /// 治理结果没有管理员或包含重复管理员。
        InvalidAssignmentResultAdmins,
        /// 固定岗位的结果人数不等于协议席位。
        FixedRoleSeatsMismatch,
        /// 治理结果缺少投票、选举或任命追溯引用。
        AssignmentSourceRefEmpty,
        /// 治理结果没有岗位、任职或法定代表人变化。
        GovernanceResultEmpty,
        /// 单次治理结果包含的岗位或任职变化超过机构上限。
        TooManyGovernanceChanges,
        /// 同一治理结果重复提交同一个岗位定义。
        DuplicateGovernanceRoleChange,
        /// 同一治理结果重复提交同一个岗位任职集合。
        DuplicateGovernanceAssignmentChange,
        /// 89 个受保护创世机构的岗位定义不可由运行期业务修改。
        FixedRoleDefinitionImmutable,
        /// 已停用岗位仍有有效任职。
        InactiveRoleHasAssignments,
        /// 法定成员机构缺少唯一指定成员岗位，或该岗位已被停用。
        RequiredMemberRoleMissing,
        /// 法定成员岗位名称不符合永久规格。
        RequiredMemberRoleNameMismatch,
        /// 法定成员岗位及 admins 人数超出永久区间。
        RequiredMemberCountOutOfRange,
        /// 辅助岗位引入了法定成员岗位之外的管理员账户。
        NonMemberAdminForbidden,
        /// 机构治理提案载荷为空或不能解码。
        InvalidInstitutionGovernanceAction,
        /// 动态岗位必须至少绑定一项不可变业务权限。
        RolePermissionsEmpty,
        /// 岗位权限字段或模块标签非法。
        InvalidRolePermission,
        /// 同一岗位重复提交相同业务权限。
        DuplicateRolePermission,
        /// 单个岗位权限数量超过协议上限。
        TooManyRolePermissions,
        /// 目标权限超出完整 CID 的顶层业务能力。
        InstitutionCapabilityDenied,
        /// 动态岗位 nonce 已耗尽。
        RoleNonceOverflow,
        /// 有限次碰撞重试后仍无法生成未使用岗位码。
        RoleCodeGenerationExhausted,
    }

    /// 提案操作类型标记：存储在 ProposalData 的第一个字节。
    /// ACTION = 1 永久保留空位,不复用。
    pub const ACTION_CLOSE: u8 = 2;
    pub const ACTION_GOVERNANCE: u8 = 3;
    /// 新增账户提案:仅用于 ProposalData 内部 finalizer 路由,与投票授权用的
    /// BusinessActionId(复用 `ACTION_INSTITUTION_CLOSE` 账户生命周期能力)相互正交。
    pub const ACTION_ADD_ACCOUNT: u8 = 4;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        // NOTE: `call_index` values are the on-chain ABI and must remain stable.

        // call_index = 0 永久保留空位,不复用

        // call_index = 5 永久关闭旧普通机构直写创建入口；恢复创建时必须使用新的
        // VotePlan 业务入口和真实 proposal_id，不复用旧载荷。

        /// 注册局更新机构全称/简称(链是机构信息唯一真源)。
        /// 机构码/CID/省市码物理编码在 CID 里,不可改故不作为参数。
        #[pallet::call_index(6)]
        #[pallet::weight(<T as pallet::Config>::WeightInfo::update_institution_info())]
        #[allow(clippy::too_many_arguments)]
        pub fn update_institution_info(
            origin: OriginFor<T>,
            cid_number: CidNumberOf<T>,
            cid_full_name: AccountNameOf<T>,
            cid_short_name: AccountNameOf<T>,
            actor_cid_number: Vec<u8>,
            actor_role_code: Vec<u8>,
        ) -> DispatchResult {
            let submitter = ensure_signed(origin)?;
            crate::institution::maintain::do_update_institution_info::<T>(
                submitter,
                cid_number,
                cid_full_name,
                cid_short_name,
                actor_cid_number,
                actor_role_code,
            )
        }

        /// 发起"给已存在机构新增自定义命名账户"提案。
        ///
        /// 与关闭账户完全对称:授权改为本机构自身(`build_institution_vote_plan` 校验
        /// 管理员名册 + 有效任职 + 岗位业务权限),发起时派生+校验并冻结进提案载荷,
        /// 内部投票通过后由 `add::execute_institution_add_account_with_finalizer` 落库。
        #[pallet::call_index(7)]
        #[pallet::weight(<T as pallet::Config>::WeightInfo::add_institution_account())]
        pub fn propose_add_institution_account(
            origin: OriginFor<T>,
            cid_number: CidNumberOf<T>,
            account_names: InstitutionAccountNamesOf<T>,
            proposer_role_code: RoleCodeOf,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            crate::add::do_propose_add_institution_account::<T>(
                who,
                cid_number,
                account_names,
                proposer_role_code,
            )
        }

        /// 本机构指定岗位任职人发起机构内部治理提案。
        ///
        /// 本入口只允许治理 `actor_cid_number == cid_number` 的本机构；注册局替
        /// 目标机构登记管理员集合必须走 `register_institution_admins`。
        #[pallet::call_index(8)]
        #[pallet::weight(<T as pallet::Config>::WeightInfo::propose_institution_governance())]
        #[allow(clippy::too_many_arguments)]
        pub fn propose_institution_governance(
            origin: OriginFor<T>,
            cid_number: CidNumberOf<T>,
            action: InstitutionGovernanceActionOf<T>,
            actor_cid_number: Vec<u8>,
            proposer_role_code: RoleCodeOf,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_propose_institution_governance(
                who,
                cid_number,
                action,
                actor_cid_number,
                proposer_role_code,
            )
        }

        /// 注册局直接登记目标公权机构管理员集合。
        ///
        /// 本入口不创建目标机构内部投票；权限来自注册局 `actor_cid_number`，
        /// 最终仍写入同一个 `public-admins::AdminAccounts[cid]` 真源。
        #[pallet::call_index(9)]
        #[pallet::weight(<T as pallet::Config>::WeightInfo::update_institution_info())]
        #[allow(clippy::too_many_arguments)]
        pub fn register_institution_admins(
            origin: OriginFor<T>,
            cid_number: CidNumberOf<T>,
            admins: InstitutionAdminsInputOf<T>,
            actor_cid_number: Vec<u8>,
            actor_role_code: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_register_institution_admins(
                who,
                cid_number,
                admins,
                actor_cid_number,
                actor_role_code,
            )
        }

        /// 发起"关闭公权机构多签账户"提案。
        ///
        /// 仅服务于 CID 注册机构地址(`AccountRegisteredCid` 命中);
        /// 个人多签关闭走 personal-manage::propose_close 入口,
        /// 输入个人地址会返回 `Error::NotInstitutionAccount`。
        #[pallet::call_index(1)]
        #[pallet::weight(<T as pallet::Config>::WeightInfo::propose_close_public_institution())]
        pub fn propose_close_public_institution(
            origin: OriginFor<T>,
            actor_cid_number: CidNumberOf<T>,
            proposer_role_code: RoleCodeOf,
            institution_account_id: T::AccountId,
            beneficiary_account_id: T::AccountId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            crate::close::do_propose_institution_close::<T>(
                who,
                actor_cid_number,
                proposer_role_code,
                institution_account_id,
                beneficiary_account_id,
            )
        }

        // call_index(4) 已永久废弃：拒绝和执行失败清理由 votingengine 终态回调完成。
    }

    impl<T: Config> Pallet<T> {
        /// 由 (cid_number, account_name) 派生机构账户地址 + 返回其 `AccountKind`。
        ///
        /// 唯一真源 = `primitives::account_derive`(op_tag/保留名/路由/payload/派生入口)。
        /// 本函数仅做薄适配:把 `account_derive` 的派生结果 + 注册策略翻译成本 pallet 的
        /// `DispatchError`,并把 32 字节 digest 解码为 `T::AccountId`:
        /// - 空 account_name → `EmptyAccountName`
        /// - `永久质押`/`安全基金`/`两和基金`(制度专属)→ `ReservedAccountName`(普通机构禁止注册)
        /// - `主账户`/`费用账户` → `InstitutionMain`/`InstitutionFee`(强制默认路由)
        /// - 其他非空 → `InstitutionNamed`
        ///
        /// 这是机构创建、维护与关闭等入口的唯一派生入口。
        ///
        /// 返回的 `AccountKind` 借用入参 `cid_number`/`account_name`，供调用方判断
        /// 协议账户或自定义命名账户。
        pub fn derive_institution_account<'a>(
            cid_number: &'a [u8],
            account_name: &'a [u8],
        ) -> Result<(T::AccountId, AccountKind<'a>), DispatchError> {
            ensure!(!account_name.is_empty(), Error::<T>::EmptyAccountName);
            // institution_kind_by_name 对非空名必返回 Some(空名已在上面拦截)。
            // 命中保留名 → Main/Fee;否则 → Named。质押/安全/两和已被 forbidden 拦下,
            // 故此处只可能得到 Main/Fee/Named 三种机构种类。
            let kind =
                primitives::account_derive::institution_kind_by_name(cid_number, account_name)
                    .ok_or(Error::<T>::EmptyAccountName)?;
            let digest = kind.derive(T::SS58Prefix::get());
            let account_id = T::AccountId::decode(&mut &digest[..])
                .map_err(|_| Error::<T>::DerivedAccountDecodeFailed)?;
            Ok((account_id, kind))
        }

        // derive_personal_account 在 personal-manage::Pallet;
        // public-manage 的机构地址只走 derive_institution_account。

        pub(crate) fn ensure_unique_admins(admins: &[T::AccountId]) -> Result<(), DispatchError> {
            let mut seen = BTreeSet::new();
            for admin in admins.iter() {
                ensure!(seen.insert(admin.clone()), Error::<T>::DuplicateAdmin);
            }
            Ok(())
        }

        /// 计算 `admins_root = blake2_256(SCALE.encode(sorted_admins))`。
        ///
        /// 排序规则:按 `AccountId` 的字节序(Substrate AccountId32 默认 Ord 即字典序)。
        /// citizenapp 端需要用同样的排序规则 + SCALE 布局,保证签名消息字节一致。
        pub fn compute_admins_root(admins: &AdminsOf<T>) -> [u8; 32] {
            let mut sorted: Vec<T::AccountId> = admins.iter().cloned().collect();
            sorted.sort();
            sp_io::hashing::blake2_256(&sorted.encode())
        }

        /// 把 `AccountId` 编码后的前 32 字节当作 sr25519 公钥。
        ///
        /// 铁律:项目内 `AccountId = AccountId32`,其 32 字节原始内容即对应 sr25519 公钥。
        /// 与 `offchain-transaction::settlement::pubkey_from_accountid` 语义对齐。
        pub fn pubkey_from_accountid(acc: &T::AccountId) -> Result<Sr25519Public, Error<T>> {
            let encoded = acc.encode();
            if encoded.len() < 32 {
                return Err(Error::<T>::MalformedSignature);
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&encoded[..32]);
            Ok(Sr25519Public::from_raw(arr))
        }

        pub(crate) fn ensure_admin_config(admins: &InstitutionAdminsInputOf<T>) -> DispatchResult {
            ensure!(T::MaxAdmins::get() >= 1, Error::<T>::InvalidRuntimeConfig);
            ensure!(!admins.is_empty(), Error::<T>::InvalidAdminsLen);
            let accounts: Vec<T::AccountId> = admins
                .iter()
                .map(|admin| admin.account_id.clone())
                .collect();
            Self::ensure_unique_admins(accounts.as_slice())?;
            Ok(())
        }

        pub(crate) fn set_institution_admins(
            cid_number: &CidNumberOf<T>,
            institution_code: InstitutionCode,
            admins: &InstitutionAdminsInputOf<T>,
        ) -> DispatchResult {
            Self::ensure_lifecycle_institution_code(&institution_code)?;
            T::AdminLifecycle::set_institution_admins(
                crate::MODULE_TAG,
                cid_number.to_vec(),
                institution_code,
                AdminAccountKind::PublicInstitution,
                admins.iter().cloned().collect(),
            )
        }

        fn governance_action_replaces_admins(action: &InstitutionGovernanceActionOf<T>) -> bool {
            matches!(
                action,
                InstitutionGovernanceAction::ReplaceAdmins { .. }
                    | InstitutionGovernanceAction::ReplaceAdminsAndMutateRoles { .. }
            )
        }

        fn ensure_governance_action_valid(
            action: &InstitutionGovernanceActionOf<T>,
        ) -> DispatchResult {
            match action {
                InstitutionGovernanceAction::ReplaceAdmins { admins } => {
                    let bounded: InstitutionAdminsInputOf<T> = admins
                        .clone()
                        .try_into()
                        .map_err(|_| Error::<T>::TooManyInstitutionAdmins)?;
                    Self::ensure_admin_config(&bounded)
                }
                InstitutionGovernanceAction::MutateRolesAndAssignments {
                    role_mutations,
                    assignment_changes,
                    legal_representative_change,
                } => {
                    for mutation in role_mutations {
                        if let InstitutionRoleMutation::Create { assignments, .. } = mutation {
                            for target in assignments {
                                ensure!(
                                    target.assignment_source
                                        == InstitutionAssignmentSource::InstitutionGovernance,
                                    Error::<T>::InvalidAssignmentSource
                                );
                            }
                        }
                    }
                    for change in assignment_changes {
                        for target in &change.assignments {
                            ensure!(
                                target.assignment_source
                                    == InstitutionAssignmentSource::InstitutionGovernance,
                                Error::<T>::InvalidAssignmentSource
                            );
                        }
                    }
                    ensure!(
                        !role_mutations.is_empty()
                            || !assignment_changes.is_empty()
                            || legal_representative_change.is_some(),
                        Error::<T>::GovernanceResultEmpty
                    );
                    Ok(())
                }
                InstitutionGovernanceAction::ReplaceAdminsAndMutateRoles {
                    admins,
                    role_mutations,
                    assignment_changes,
                    legal_representative_change,
                } => {
                    let bounded: InstitutionAdminsInputOf<T> = admins
                        .clone()
                        .try_into()
                        .map_err(|_| Error::<T>::TooManyInstitutionAdmins)?;
                    Self::ensure_admin_config(&bounded)?;
                    for mutation in role_mutations {
                        if let InstitutionRoleMutation::Create { assignments, .. } = mutation {
                            for target in assignments {
                                ensure!(
                                    target.assignment_source
                                        == InstitutionAssignmentSource::InstitutionGovernance,
                                    Error::<T>::InvalidAssignmentSource
                                );
                            }
                        }
                    }
                    for change in assignment_changes {
                        for target in &change.assignments {
                            ensure!(
                                target.assignment_source
                                    == InstitutionAssignmentSource::InstitutionGovernance,
                                Error::<T>::InvalidAssignmentSource
                            );
                        }
                    }
                    ensure!(
                        !role_mutations.is_empty()
                            || !assignment_changes.is_empty()
                            || legal_representative_change.is_some(),
                        Error::<T>::GovernanceResultEmpty
                    );
                    Ok(())
                }
            }
        }

        #[allow(clippy::too_many_arguments)]
        pub(crate) fn do_propose_institution_governance(
            who: T::AccountId,
            cid_number: CidNumberOf<T>,
            action: InstitutionGovernanceActionOf<T>,
            actor_cid_number: Vec<u8>,
            proposer_role_code: RoleCodeOf,
        ) -> DispatchResult {
            ensure!(!cid_number.is_empty(), Error::<T>::EmptyCidNumber);
            ensure!(
                actor_cid_number.as_slice() == cid_number.as_slice(),
                Error::<T>::RegistryAuthorityDenied
            );
            let info =
                Institutions::<T>::get(&cid_number).ok_or(Error::<T>::InstitutionNotFound)?;
            Self::ensure_lifecycle_institution_code(&info.institution_code)?;
            // `build_institution_vote_plan` 一次校验 CID、岗位码、任职账户和业务权限；
            // 禁止在业务入口把管理员名册成员身份单独当成授权。
            Self::ensure_governance_action_valid(&action)?;
            let proposal = InstitutionGovernanceProposal {
                institution_code: info.institution_code,
                cid_number: cid_number.to_vec(),
                action,
            };
            let mut data = Vec::from(crate::MODULE_TAG);
            data.push(ACTION_GOVERNANCE);
            data.extend_from_slice(&proposal.encode());
            let vote_plan = Self::build_institution_vote_plan(
                &who,
                cid_number.as_slice(),
                proposer_role_code.as_slice(),
                entity_primitives::business_action::ACTION_INSTITUTION_GOVERNANCE,
                &data,
            )?;
            let proposal_id = if Self::governance_action_replaces_admins(&proposal.action) {
                T::InternalVoteEngine::create_institution_admin_change_proposal_with_data(
                    who.clone(),
                    info.institution_code,
                    cid_number.to_vec(),
                    vote_plan,
                    data,
                )?
            } else {
                T::InternalVoteEngine::create_institution_proposal_with_data(
                    who.clone(),
                    info.institution_code,
                    cid_number.to_vec(),
                    None,
                    Vec::new(),
                    vote_plan,
                    data,
                )?
            };
            Self::deposit_event(Event::<T>::InstitutionGovernanceProposed {
                proposal_id,
                cid_number,
                proposer_account_id: who,
            });
            Ok(())
        }

        /// 业务模块唯一构造机构内部投票计划的入口。
        pub(crate) fn build_institution_vote_plan(
            who: &T::AccountId,
            cid_number: &[u8],
            proposer_role_code: &[u8],
            action_code: u32,
            business_data: &[u8],
        ) -> Result<VotePlanOf<T::AccountId>, sp_runtime::DispatchError> {
            let business_action_id = BusinessActionId {
                module_tag: crate::MODULE_TAG.to_vec(),
                action_code,
            };
            let proposer_subject = entity_primitives::RoleSubject {
                cid_number: cid_number.to_vec(),
                role_code: proposer_role_code.to_vec(),
            };
            ensure!(
                <Self as InstitutionRoleAuthorizationQuery<T::AccountId>>::is_authorized(
                    who,
                    &proposer_subject,
                    &business_action_id,
                    RolePermissionOperation::Propose,
                ),
                Error::<T>::PermissionDenied
            );
            let voter_roles =
                <Self as InstitutionRoleAuthorizationQuery<T::AccountId>>::role_subjects_with_permission(
                    cid_number,
                    &business_action_id,
                    RolePermissionOperation::Vote,
                );

            let owner: BoundedVec<
                u8,
                frame_support::traits::ConstU32<
                    { entity_primitives::BUSINESS_MODULE_TAG_MAX_BYTES },
                >,
            > = crate::MODULE_TAG
                .to_vec()
                .try_into()
                .map_err(|_| votingengine::Error::<T>::InvalidVotePlan)?;
            let proposer_cid = CidNumber::try_from(cid_number.to_vec())
                .map_err(|_| votingengine::Error::<T>::InvalidVotePlan)?;
            let proposer_role = RoleCode::try_from(proposer_role_code.to_vec())
                .map_err(|_| votingengine::Error::<T>::InvalidVotePlan)?;
            let voter_subjects = voter_roles
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
            VotePlanOf::<T::AccountId>::try_new(
                BusinessActionId {
                    module_tag: owner.clone(),
                    action_code,
                },
                owner,
                AuthorizationSubject::Institution(RoleSubject {
                    cid_number: proposer_cid,
                    role_code: proposer_role,
                }),
                voter_subjects,
                VotingEngineKind::Internal,
                sp_io::hashing::blake2_256(business_data),
            )
            .map_err(|_| votingengine::Error::<T>::InvalidVotePlan.into())
        }

        pub(crate) fn do_register_institution_admins(
            who: T::AccountId,
            cid_number: CidNumberOf<T>,
            admins: InstitutionAdminsInputOf<T>,
            actor_cid_number: Vec<u8>,
            actor_role_code: Vec<u8>,
        ) -> DispatchResult {
            ensure!(!cid_number.is_empty(), Error::<T>::EmptyCidNumber);
            let info =
                Institutions::<T>::get(&cid_number).ok_or(Error::<T>::InstitutionNotFound)?;
            Self::ensure_lifecycle_institution_code(&info.institution_code)?;
            Self::ensure_admin_config(&admins)?;
            // 授权唯一真源:extrinsic 签名者 `who` 必须是注册局(actor)机构在册管理员,
            // 且注册局对目标机构 CID/机构码有登记权(省/市作用域由目标 CID 直接派生)。
            ensure!(
                T::RegistryAuthority::can_register_institution_origin(
                    &who,
                    actor_cid_number.as_slice(),
                    actor_role_code.as_slice(),
                    cid_number.as_slice(),
                    info.institution_code,
                ),
                Error::<T>::RegistryAuthorityDenied
            );
            Self::set_institution_admins(&cid_number, info.institution_code, &admins)?;
            Self::deposit_event(Event::<T>::InstitutionAdminsRegistered {
                cid_number,
                admins_len: admins.len() as u32,
                submitter: who,
            });
            Ok(())
        }

        pub(crate) fn execute_governance_proposal(
            proposal_id: u64,
            proposal: InstitutionGovernanceProposalOf<T>,
        ) -> DispatchResult {
            let cid_number: CidNumberOf<T> = proposal
                .cid_number
                .clone()
                .try_into()
                .map_err(|_| Error::<T>::InvalidAssignmentResultInstitution)?;
            let result_source_ref = proposal_id.to_le_bytes().to_vec();
            match proposal.action {
                InstitutionGovernanceAction::ReplaceAdmins { admins } => {
                    let bounded: InstitutionAdminsInputOf<T> = admins
                        .try_into()
                        .map_err(|_| Error::<T>::TooManyInstitutionAdmins)?;
                    Self::set_institution_admins(&cid_number, proposal.institution_code, &bounded)
                }
                InstitutionGovernanceAction::MutateRolesAndAssignments {
                    role_mutations,
                    assignment_changes,
                    legal_representative_change,
                } => Self::apply_institution_governance_result(InstitutionGovernanceResult {
                    institution_code: proposal.institution_code,
                    cid_number: proposal.cid_number,
                    proposal_id,
                    role_mutations,
                    assignment_changes,
                    legal_representative_change,
                    result_source_ref,
                }),
                InstitutionGovernanceAction::ReplaceAdminsAndMutateRoles {
                    admins,
                    role_mutations,
                    assignment_changes,
                    legal_representative_change,
                } => {
                    let bounded: InstitutionAdminsInputOf<T> = admins
                        .try_into()
                        .map_err(|_| Error::<T>::TooManyInstitutionAdmins)?;
                    frame_support::storage::with_transaction(|| {
                        if let Err(error) = Self::set_institution_admins(
                            &cid_number,
                            proposal.institution_code,
                            &bounded,
                        ) {
                            return frame_support::storage::TransactionOutcome::Rollback(Err(
                                error,
                            ));
                        }
                        match Self::apply_institution_governance_result(
                            InstitutionGovernanceResult {
                                institution_code: proposal.institution_code,
                                cid_number: proposal.cid_number,
                                proposal_id,
                                role_mutations,
                                assignment_changes,
                                legal_representative_change,
                                result_source_ref,
                            },
                        ) {
                            Ok(()) => frame_support::storage::TransactionOutcome::Commit(Ok(())),
                            Err(error) => {
                                frame_support::storage::TransactionOutcome::Rollback(Err(error))
                            }
                        }
                    })
                }
            }
        }

        /// 公权机构管理员模块只接受公权法人机构码。
        pub(crate) fn ensure_lifecycle_institution_code(
            institution_code: &InstitutionCode,
        ) -> DispatchResult {
            ensure!(
                is_public_admin_code(institution_code),
                Error::<T>::InvalidInstitutionCode
            );
            Ok(())
        }

        /// 从任意机构账户反查管理员更换机构码。
        ///
        /// 机构账户必须使用公权/私权法人机构码；PMUL 只属于个人多签。
        pub fn resolve_institution_code_for_account(
            account_id: &T::AccountId,
        ) -> Option<InstitutionCode> {
            let registered = AccountRegisteredCid::<T>::get(account_id)?;
            Institutions::<T>::get(&registered.cid_number).map(|inst| inst.institution_code)
        }

        // 投票回调执行体:
        // - ACTION_CLOSE → crate::close::execute_institution_close_with_finalizer
        // (ACTION_CREATE_PERSONAL 在 personal-manage 独立 pallet)
    }
}

// ──── InstitutionMultisigQuery 实现:对 multisig-transfer / runtime config 暴露查询 ────
//
// 输入任意机构账户(主/费用/自创),直接以账户地址读取 admins 模块 账户。
// 再按 Institutions[cid_number].institution_code 读取机构码。这条路径保证
// 机构账户只使用公权/私权法人机构码,不再把机构账户错误塞到 PMUL。

impl<T: pallet::Config> traits::InstitutionMultisigQuery<T::AccountId> for pallet::Pallet<T> {
    fn lookup_institution_account(cid_number: &[u8], account_name: &[u8]) -> Option<T::AccountId> {
        let cid_number = pallet::CidNumberOf::<T>::try_from(cid_number.to_vec()).ok()?;
        let account_name = pallet::AccountNameOf::<T>::try_from(account_name.to_vec()).ok()?;
        let stored = pallet::InstitutionAccounts::<T>::get(&cid_number, &account_name)?;
        let reverse = pallet::AccountRegisteredCid::<T>::get(&stored.account_id)?;
        (reverse.cid_number == cid_number && reverse.account_name == account_name)
            .then_some(stored.account_id)
    }

    fn account_belongs_to(cid_number: &[u8], addr: &T::AccountId) -> bool {
        let Some(registered) = pallet::AccountRegisteredCid::<T>::get(addr) else {
            return false;
        };
        registered.cid_number.as_slice() == cid_number
            && pallet::InstitutionAccounts::<T>::get(
                &registered.cid_number,
                &registered.account_name,
            )
            .is_some_and(|stored| stored.account_id == *addr)
    }

    fn lookup_cid(addr: &T::AccountId) -> Option<Vec<u8>> {
        pallet::AccountRegisteredCid::<T>::get(addr)
            .map(|registered| registered.cid_number.to_vec())
    }

    fn lookup_org(addr: &T::AccountId) -> Option<InstitutionCode> {
        pallet::Pallet::<T>::resolve_institution_code_for_account(addr)
    }

    fn lookup_admin_config(
        addr: &T::AccountId,
    ) -> Option<primitives::multisig::MultisigConfigSnapshot<T::AccountId>> {
        let institution_code = Self::lookup_org(addr)?;
        let cid_number = Self::lookup_cid(addr)?;
        let admins =
            T::InstitutionAdminQuery::institution_admins(institution_code, cid_number.as_slice())?;
        let admins_len = admins.len() as u32;
        let cid: pallet::CidNumberOf<T> = cid_number.clone().try_into().ok()?;
        let threshold = pallet::InstitutionGovernanceThresholds::<T>::get(cid)?;
        Some(primitives::multisig::MultisigConfigSnapshot {
            admins,
            admins_len,
            threshold,
        })
    }

    fn account_exists(addr: &T::AccountId) -> bool {
        let Some(registered) = pallet::AccountRegisteredCid::<T>::get(addr) else {
            return false;
        };
        pallet::InstitutionAccounts::<T>::get(&registered.cid_number, &registered.account_name)
            .map(|account_id| account_id.account_id == *addr)
            .unwrap_or(false)
    }
}

impl<T: pallet::Config> traits::InstitutionCidQuery<pallet::CidNumberOf<T>> for pallet::Pallet<T> {
    fn cid_exists(cid_number: &pallet::CidNumberOf<T>) -> bool {
        pallet::Institutions::<T>::contains_key(cid_number)
    }
}

impl<T: pallet::Config> traits::InstitutionLegalRepresentativeQuery<T::AccountId>
    for pallet::Pallet<T>
{
    fn legal_representative(cid_number: &[u8]) -> Option<T::AccountId> {
        let cid_number = pallet::CidNumberOf::<T>::try_from(cid_number.to_vec()).ok()?;
        pallet::Institutions::<T>::get(cid_number)?
            .legal_representative
            .map(|representative| representative.account_id)
    }

    fn legal_representative_cid(cid_number: &[u8]) -> Option<Vec<u8>> {
        let cid_number = pallet::CidNumberOf::<T>::try_from(cid_number.to_vec()).ok()?;
        pallet::Institutions::<T>::get(cid_number)?
            .legal_representative
            .map(|representative| representative.cid_number.to_vec())
    }
}

// ──── 投票终态回调:把已通过的机构账户新增/关闭提案落地到链上 ────
//
// 投票统一由投票引擎承担,提案通过(或否决)经
// [`votingengine::InternalVoteResultCallback`] 广播回来。
// 本 Executor(机构侧)按 `MODULE_TAG + ACTION 字节` 认领机构管理提案:
// - `ACTION_ADD_ACCOUNT` + approved → 分派到 `add::execute_institution_add_account_with_finalizer`;
// - `ACTION_CLOSE` + approved → 分派到 `close::execute_institution_close_with_finalizer`;
// - `approved = false` → 清理对应 Pending(新增按 CID、关闭按账户),释放占用。
// (ACTION_CREATE_PERSONAL 在 personal-manage::InternalVoteExecutor)
pub struct InternalVoteExecutor<T>(core::marker::PhantomData<T>);

impl<T: pallet::Config> InternalVoteResultCallback for InternalVoteExecutor<T> {
    fn on_internal_vote_finalized(
        proposal_id: u64,
        approved: bool,
    ) -> Result<ProposalExecutionOutcome, sp_runtime::DispatchError> {
        use frame_support::storage::{with_transaction, TransactionOutcome};
        let raw = match votingengine::Pallet::<T>::get_proposal_data(proposal_id) {
            Some(raw) if raw.starts_with(crate::MODULE_TAG) => raw,
            _ => return Ok(ProposalExecutionOutcome::Ignored),
        };
        let tag = crate::MODULE_TAG;
        if raw.len() <= tag.len() {
            return Ok(ProposalExecutionOutcome::Ignored);
        }
        let action_byte = raw[tag.len()];

        if approved {
            match action_byte {
                ACTION_ADD_ACCOUNT => {
                    let action = AddInstitutionAccountAction::<
                        T::AccountId,
                        CidNumberOf<T>,
                        AccountNameOf<T>,
                    >::decode(&mut &raw[tag.len() + 1..])
                    .map_err(|_| pallet::Error::<T>::ProposalActionNotFound)?;
                    let exec_result = with_transaction(|| {
                        match crate::add::execute_institution_add_account_with_finalizer::<T>(
                            proposal_id,
                            &action,
                        ) {
                            Ok(()) => TransactionOutcome::Commit(Ok(())),
                            Err(e) => TransactionOutcome::Rollback(Err(e)),
                        }
                    });
                    if exec_result.is_err() {
                        pallet::Pallet::<T>::deposit_event(
                            pallet::Event::<T>::InstitutionAddAccountExecutionFailed {
                                proposal_id,
                                cid_number: action.actor_cid_number,
                            },
                        );
                        return Ok(ProposalExecutionOutcome::RetryableFailed);
                    }
                    return Ok(ProposalExecutionOutcome::Executed);
                }
                ACTION_CLOSE => {
                    let action = CloseInstitutionAction::<T::AccountId, CidNumberOf<T>>::decode(
                        &mut &raw[tag.len() + 1..],
                    )
                    .map_err(|_| pallet::Error::<T>::ProposalActionNotFound)?;
                    let exec_result = with_transaction(|| {
                        match crate::close::execute_institution_close_with_finalizer::<T>(
                            proposal_id,
                            &action,
                        ) {
                            Ok(()) => TransactionOutcome::Commit(Ok(())),
                            Err(e) => TransactionOutcome::Rollback(Err(e)),
                        }
                    });
                    if exec_result.is_err() {
                        pallet::Pallet::<T>::deposit_event(
                            pallet::Event::<T>::InstitutionCloseExecutionFailed {
                                proposal_id,
                                account_id: action.institution_account_id,
                            },
                        );
                        return Ok(ProposalExecutionOutcome::RetryableFailed);
                    }
                    return Ok(ProposalExecutionOutcome::Executed);
                }
                ACTION_GOVERNANCE => {
                    let proposal = pallet::InstitutionGovernanceProposalOf::<T>::decode(
                        &mut &raw[tag.len() + 1..],
                    )
                    .map_err(|_| pallet::Error::<T>::InvalidInstitutionGovernanceAction)?;
                    let exec_result =
                        with_transaction(
                            || match pallet::Pallet::<T>::execute_governance_proposal(
                                proposal_id,
                                proposal,
                            ) {
                                Ok(()) => TransactionOutcome::Commit(Ok(())),
                                Err(e) => TransactionOutcome::Rollback(Err(e)),
                            },
                        );
                    if exec_result.is_err() {
                        return Ok(ProposalExecutionOutcome::RetryableFailed);
                    }
                    return Ok(ProposalExecutionOutcome::Executed);
                }
                _ => return Ok(ProposalExecutionOutcome::Ignored),
            }
        } else {
            // 否决:清理对应 Pending 记录,释放占用(新增按 CID、关闭按账户)。
            if action_byte == ACTION_ADD_ACCOUNT {
                if let Ok(action) = AddInstitutionAccountAction::<
                    T::AccountId,
                    CidNumberOf<T>,
                    AccountNameOf<T>,
                >::decode(&mut &raw[tag.len() + 1..])
                {
                    InstitutionPendingAdd::<T>::remove(&action.actor_cid_number);
                }
            } else if action_byte == ACTION_CLOSE {
                if let Ok(action) = CloseInstitutionAction::<T::AccountId, CidNumberOf<T>>::decode(
                    &mut &raw[tag.len() + 1..],
                ) {
                    InstitutionPendingClose::<T>::remove(&action.institution_account_id);
                }
            }
        }
        Ok(ProposalExecutionOutcome::Executed)
    }

    fn on_execution_failed_terminal(proposal_id: u64) -> DispatchResult {
        let raw = match votingengine::Pallet::<T>::get_proposal_data(proposal_id) {
            Some(raw) if raw.starts_with(crate::MODULE_TAG) => raw,
            _ => return Ok(()),
        };
        let tag = crate::MODULE_TAG;
        ensure!(
            raw.len() > tag.len(),
            pallet::Error::<T>::ProposalActionNotFound
        );
        if raw[tag.len()] == ACTION_ADD_ACCOUNT {
            let action = AddInstitutionAccountAction::<
                T::AccountId,
                CidNumberOf<T>,
                AccountNameOf<T>,
            >::decode(&mut &raw[tag.len() + 1..])
            .map_err(|_| pallet::Error::<T>::ProposalActionNotFound)?;
            InstitutionPendingAdd::<T>::remove(&action.actor_cid_number);
        } else if raw[tag.len()] == ACTION_CLOSE {
            let action = CloseInstitutionAction::<T::AccountId, CidNumberOf<T>>::decode(
                &mut &raw[tag.len() + 1..],
            )
            .map_err(|_| pallet::Error::<T>::ProposalActionNotFound)?;
            InstitutionPendingClose::<T>::remove(&action.institution_account_id);
        }
        Ok(())
    }
}
