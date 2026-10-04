// This is free and unencumbered software released into the public domain.
//
// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.
//
// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.
//
// For more information, please refer to <http://unlicense.org>

// Substrate and Polkadot dependencies
use admin_primitives::{AdminAccountQuery, InstitutionAdminQuery};
use alloc::vec::Vec;
use codec::Decode;
use entity_primitives::InstitutionMultisigQuery;
use entity_primitives::{
    InstitutionRoleAuthorizationQuery, InstitutionRoleQuery, RolePermissionOperation,
};
#[cfg(not(feature = "runtime-benchmarks"))]
use frame_support::traits::UnfilteredDispatchable;
use frame_support::{
    derive_impl,
    dispatch::{DispatchClass, DispatchResult},
    parameter_types,
    traits::{
        fungible::{Balanced, Credit},
        ConstU128, ConstU32, ConstU64, ConstU8, Contains, EnsureOrigin, FindAuthor, OnUnbalanced,
        VariantCountOf,
    },
    weights::{
        constants::{RocksDbWeight, WEIGHT_REF_TIME_PER_SECOND},
        ConstantMultiplier, Weight,
    },
};
use frame_system::limits::{BlockLength, BlockWeights};
use onchain::NrcAccountProvider as _;
use pallet_transaction_payment::{ConstFeeMultiplier, Multiplier};
use sp_core::sr25519;
use sp_core::Void;
use sp_io::crypto::sr25519_verify;
#[cfg(feature = "runtime-benchmarks")]
use sp_runtime::{traits::IdentifyAccount, MultiSigner};
use sp_runtime::{traits::One, Perbill};
use sp_version::RuntimeVersion;

// Local module imports
use super::{
    AccountId, Assets, Balance, Balances, Block, BlockNumber, CitizenIssuance, ElectionVote,
    GenesisPallet, Hash, InternalVote, JointVote, LegislationVote, LegislationYuan, Nonce,
    PalletInfo, PrivateAdmins, PrivateManage, PublicAdmins, PublicManage, Runtime, RuntimeCall,
    RuntimeEvent, RuntimeFreezeReason, RuntimeHoldReason, RuntimeOrigin, RuntimeTask, System,
    BLOCK_HASH_COUNT, EXISTENTIAL_DEPOSIT, VERSION,
};
#[cfg(not(feature = "runtime-benchmarks"))]
use super::{ResolutionIssuance, RuntimeUpgrade};

const NORMAL_DISPATCH_RATIO: Perbill =
    Perbill::from_percent(primitives::core_const::NORMAL_DISPATCH_PERCENT);

parameter_types! {
    pub const BlockHashCount: BlockNumber = BLOCK_HASH_COUNT;
    /// 使用 BlockNumber 类型声明重试宽限期，避免与具体 u32 常量类型耦合。
    pub const VotingExecutionRetryGraceBlocks: BlockNumber = 21_600;
    pub const Version: RuntimeVersion = VERSION;

    /// 每个区块允许 60 秒计算预算（weight ref_time）。
    pub RuntimeBlockWeights: BlockWeights = BlockWeights::with_sensible_defaults(
        Weight::from_parts(60u64 * WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
        NORMAL_DISPATCH_RATIO,
    );
    pub RuntimeBlockLength: BlockLength = BlockLength::builder()
        .max_length(primitives::core_const::MAX_BLOCK_BYTES)
        .modify_max_length_for_class(
            DispatchClass::Normal,
            |max_length| *max_length = NORMAL_DISPATCH_RATIO * *max_length,
        )
        .build();
    /// 公民币主链地址编号（SS58 前缀）：统一来源于 primitives 常量。
    pub const SS58Prefix: u16 = primitives::core_const::SS58_FORMAT;
}

/// All migrations of the runtime, aside from the ones declared in the pallets.
///
/// This can be a tuple of types, each implementing `OnRuntimeUpgrade`.
///
/// 开发期零用户、重新创世模型：各 pallet 不设迁移；这里仅保留 runtime 级独立迁移集合，当前为空。
type SingleBlockMigrations = ();

pub fn is_stake_account(address: &AccountId) -> bool {
    primitives::cid::china::china_ch::CHINA_CH
        .iter()
        .any(|n| address == &AccountId::new(n.stake_account))
}

fn is_reserved_fee_account(address: &AccountId) -> bool {
    primitives::cid::china::china_ch::CHINA_CH
        .iter()
        .any(|n| address == &AccountId::new(n.fee_account))
}

/// 检查是否为国家储委会安全基金账户。
fn is_safety_fund_account(address: &AccountId) -> bool {
    address == &AccountId::new(primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT)
}

/// 检查是否为国家储委会两和基金账户。
fn is_nrc_he_account(address: &AccountId) -> bool {
    address == &AccountId::new(primitives::cid::china::china_cb::NRC_HE_ACCOUNT)
}

/// 检查是否为储委会费用账户（44 个机构的 fee_account）。
fn is_cb_fee_account(address: &AccountId) -> bool {
    primitives::cid::china::china_cb::CHINA_CB
        .iter()
        .any(|n| address == &AccountId::new(n.fee_account))
}

/// 检查是否为公民链技术发展基金会费用账户。
fn is_citizenchain_fee_account(address: &AccountId) -> bool {
    address
        == &AccountId::new(
            primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION.fee_account,
        )
}

fn is_reserved_main_account(address: &AccountId) -> bool {
    let raw: &[u8] = address.as_ref();
    if raw.len() != 32 {
        return false;
    }
    let mut addr = [0u8; 32];
    addr.copy_from_slice(raw);
    primitives::cid::china::china_zb::is_reserved_main_account(&addr)
}

/// 是否为某私法人股份公司（SFGF）的【清算账户】（承载 L2 用户存款准备金）。
///
/// 清算账户是运行期动态注册的私权机构账户，不在任何创世保留表内，故用私法人反查
/// 索引 `private_manage::AccountRegisteredCid` 按 `account_name == "清算账户"` 判定。
/// 清算行已确认是私法人（SFGF），只查 private_manage。
fn is_clearing_account(address: &AccountId) -> bool {
    private_manage::AccountRegisteredCid::<Runtime>::get(address)
        .map(|info| {
            info.account_name.as_slice() == primitives::account_derive::RESERVED_NAME_CLEARING
        })
        .unwrap_or(false)
}

/// 是否为联邦安全局（FSC）的【联邦公民安全基金】账户。
///
/// FSC 是总统府下属公权机构，故查 `public_manage::AccountRegisteredCid` 反查索引，
/// 按 `account_name == "联邦公民安全基金"` 判定。该基金为 FSC 专属，其他机构没有。
fn is_federal_citizen_security_fund_account(address: &AccountId) -> bool {
    public_manage::AccountRegisteredCid::<Runtime>::get(address)
        .map(|info| info.account_name.as_slice() == primitives::account_derive::RESERVED_NAME_FCSF)
        .unwrap_or(false)
}

pub struct RuntimeCallFilter;

impl Contains<RuntimeCall> for RuntimeCallFilter {
    fn contains(call: &RuntimeCall) -> bool {
        match call {
            // Balances 只作为底层余额账本和内部 Currency 能力保留。
            // 外部单账户链上转账唯一入口是 OnchainTransaction::transfer_with_remark。
            RuntimeCall::Balances(_) => false,
            // ADR-011 铁律:pallet_assets 内核所有原生 extrinsic 一律 reject。
            // 业务调用必须经由 OnchainIssuance::propose_* → InternalVote/JointVote callback → 内部 root 调用。
            // 任何外部 extrinsic 直接打到 pallet_assets 全部不入块,
            // 这是用户代币治理唯一入口铁律的链端兜底。
            RuntimeCall::Assets(_) => false,
            // 未启用模块:onchain-issuance(ADR-011 用户代币,当前为空壳)与
            // offchain-transaction(链下清算行,业务未启用)一律 reject 直接外部调用。
            // offchain-transaction 的开户、充值、提现、换行、批次、费率和节点登记
            // 全部保持整体禁用；任何单个调用均不得因 origin 或参数不同绕过本过滤器。
            // 后续启用必须另行确认完整业务规则、岗位权限和指定投票引擎，并通过
            // runtime 升级删除对应分支；不能只解除过滤就宣称业务可用。
            RuntimeCall::OnchainIssuance(_) => false,
            RuntimeCall::OffchainTransaction(_) => false,

            // ── 放行:逐 pallet 显式列出,不设 `_` 通配分支 ──
            // 与同文件 `fee_route` 保持同一策略:新增 pallet 会触发编译期
            // non-exhaustive 错误,强制作者显式决定放行还是拒绝,
            // 不会因为默认放行而悄悄把未审查的 extrinsic 暴露给外部。
            // 系统与共识基础。
            RuntimeCall::System(_) | RuntimeCall::Timestamp(_) | RuntimeCall::Grandpa(_) => true,
            // 交易与发行。
            RuntimeCall::OnchainTransaction(_)
            | RuntimeCall::MultisigTransfer(_)
            | RuntimeCall::FullnodeIssuance(_)
            | RuntimeCall::ResolutionIssuance(_)
            | RuntimeCall::CitizenIssuance(_) => true,
            // 投票引擎核心与四个 sub-pallet。
            RuntimeCall::VotingEngine(_)
            | RuntimeCall::InternalVote(_)
            | RuntimeCall::JointVote(_)
            | RuntimeCall::ElectionVote(_)
            | RuntimeCall::LegislationVote(_) => true,
            // 治理与协议升级。
            RuntimeCall::RuntimeUpgrade(_)
            | RuntimeCall::ResolutionDestroy(_)
            | RuntimeCall::GrandpaKeyChange(_)
            | RuntimeCall::LegislationYuan(_) => true,
            // 实体生命周期与管理员。
            RuntimeCall::PublicManage(_)
            | RuntimeCall::PrivateManage(_)
            | RuntimeCall::PersonalManage(_)
            | RuntimeCall::PersonalAdmins(_) => true,
            // 身份与业务索引。
            RuntimeCall::CitizenIdentity(_)
            | RuntimeCall::AddressRegistry(_)
            | RuntimeCall::SquarePost(_) => true,
        }
    }
}

/// The default types are being injected by [`derive_impl`](`frame_support::derive_impl`) from
/// [`SoloChainDefaultConfig`](`struct@frame_system::config_preludes::SolochainDefaultConfig`),
/// but overridden as needed.
#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
impl frame_system::Config for Runtime {
    /// The block type for the runtime.
    type Block = Block;
    /// Block & extrinsics weights: base values and limits.
    type BlockWeights = RuntimeBlockWeights;
    /// The maximum length of a block (in bytes).
    type BlockLength = RuntimeBlockLength;
    /// The identifier used to distinguish between accounts.
    type AccountId = AccountId;
    /// The type for storing how many extrinsics an account has signed.
    type Nonce = Nonce;
    /// The type for hashing blocks and tries.
    type Hash = Hash;
    /// Maximum number of block number to block hash mappings to keep (oldest pruned first).
    type BlockHashCount = BlockHashCount;
    /// The weight of database operations that the runtime can invoke.
    type DbWeight = RocksDbWeight;
    /// Version of the runtime.
    type Version = Version;
    /// The data to be stored in an account.
    type AccountData = pallet_balances::AccountData<Balance>;
    /// 地址显示编号（SS58 前缀），统一来自 primitives 制度常量。
    type SS58Prefix = SS58Prefix;
    /// 全局调用过滤器，禁止 stake_account 参与 force_* 余额调用，并封禁强制总发行量调整入口。
    type BaseCallFilter = RuntimeCallFilter;
    type MaxConsumers = frame_support::traits::ConstU32<16>;
    type SingleBlockMigrations = SingleBlockMigrations;
}

impl pallet_timestamp::Config for Runtime {
    /// A timestamp: milliseconds since the unix epoch.
    type Moment = u64;
    // 纯 PoW 共识：时间戳不再依赖 Aura 插槽回调。
    type OnTimestampSet = ();
    // PoW 找到即出块；这里只要求时间戳至少递增 1ms，不用时间戳人为节流。
    type MinimumPeriod = ConstU64<1>;
    type WeightInfo = pallet_timestamp::weights::SubstrateWeight<Runtime>;
}

impl pallet_balances::Config for Runtime {
    type MaxLocks = ConstU32<50>;
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    /// The type for recording an account's balance.
    type Balance = Balance;
    /// The ubiquitous event type.
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = RuntimeDustHandler;
    type ExistentialDeposit = ConstU128<EXISTENTIAL_DEPOSIT>;
    type AccountStore = System;
    type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
    type FreezeIdentifier = RuntimeFreezeReason;
    type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type DoneSlashHandler = ();
}

parameter_types! {
    pub const MaxGrandpaAuthorities: u32 = 64;
    pub const MaxGrandpaNominators: u32 = 0;
    // 保留最近若干 set_id 与会话映射，便于后续接入等值投票追溯/举报能力。
    pub const MaxSetIdSessionEntries: u64 = 128;
}

impl pallet_grandpa::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
    type MaxAuthorities = MaxGrandpaAuthorities;
    type MaxNominators = MaxGrandpaNominators;
    type MaxSetIdSessionEntries = MaxSetIdSessionEntries;
    // 当前版本不启用链上等值投票惩罚（无 session/historical 证明体系）。
    // 但保留 MaxSetIdSessionEntries 以便后续平滑接入。
    type KeyOwnerProof = Void;
    type EquivocationReportSystem = ();
}

parameter_types! {
    pub FeeMultiplier: Multiplier = Multiplier::one();
}

impl pallet_transaction_payment::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnChargeTransaction = onchain::OnchainChargeAdapter<
        Balances,
        onchain::OnchainFeeRouter<
            Runtime,
            Balances,
            PowDigestAuthor,
            RuntimeNrcAccountProvider,
            RuntimeSafetyFundAccountProvider,
        >,
        RuntimeFeeRouter,
    >;
    type OperationalFeeMultiplier = ConstU8<{ primitives::fee_policy::OPERATIONAL_FEE_MULTIPLIER }>;
    type WeightToFee = ConstantMultiplier<Balance, ConstU128<0>>;
    type LengthToFee = ConstantMultiplier<Balance, ConstU128<0>>;
    type FeeMultiplierUpdate = ConstFeeMultiplier<FeeMultiplier>;
    type WeightInfo = pallet_transaction_payment::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    /// 链上交易费率下发到 metadata 的转发值；真源恒为 `primitives::fee_policy`。
    pub const RuntimeOnchainFeeRate: Perbill = primitives::fee_policy::ONCHAIN_FEE_RATE;
}

impl onchain::pallet::Config for Runtime {
    type Currency = Balances;
    type MaxTransferRemarkLen = ConstU32<99>;
    // 三项收费参数只做转发，不在此处另立数字；客户端从 metadata 读取后自行预估费用。
    type OnchainMinFee = ConstU128<{ primitives::fee_policy::ONCHAIN_MIN_FEE }>;
    type OnchainFeeRate = RuntimeOnchainFeeRate;
    type VoteFlatFee = ConstU128<{ primitives::fee_policy::VOTE_FLAT_FEE }>;
}

pub struct RuntimeNrcAccountProvider;

impl onchain::NrcAccountProvider<AccountId> for RuntimeNrcAccountProvider {
    fn nrc_account() -> Option<AccountId> {
        Some(AccountId::new(
            primitives::cid::china::china_cb::CHINA_CB[0].fee_account,
        ))
    }
}

pub struct RuntimeSafetyFundAccountProvider;

impl onchain::SafetyFundAccountProvider<AccountId> for RuntimeSafetyFundAccountProvider {
    fn safety_fund_account() -> AccountId {
        AccountId::new(primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT)
    }
}

pub struct RuntimeDustHandler;

impl OnUnbalanced<Credit<AccountId, Balances>> for RuntimeDustHandler {
    fn on_nonzero_unbalanced(amount: Credit<AccountId, Balances>) {
        if let Some(nrc_account) = RuntimeNrcAccountProvider::nrc_account() {
            if let Err(remaining) = Balances::resolve(&nrc_account, amount) {
                drop(remaining);
            }
        } else {
            drop(amount);
        }
    }
}

pub struct RuntimeFeeRouter;

fn signer_onchain_route(
    who: &AccountId,
    transaction_amount: Balance,
) -> primitives::fee_policy::FeeRoute<AccountId, Balance> {
    primitives::fee_policy::FeeRoute::Onchain {
        transaction_amount,
        payer_account_id: who.clone(),
    }
}

fn signer_vote_route(who: &AccountId) -> primitives::fee_policy::FeeRoute<AccountId, Balance> {
    primitives::fee_policy::FeeRoute::Vote {
        payer_account_id: who.clone(),
    }
}

/// 严格读取 `(cid_number, 费用账户)`；公权/私权重复、正反索引不一致或账户缺失均失败。
fn exact_institution_fee_account(cid_number: &[u8]) -> Option<AccountId> {
    RuntimeInstitutionQuery::lookup_institution_account(
        cid_number,
        primitives::account_derive::RESERVED_NAME_FEE,
    )
}

/// 账户型机构交易必须显式携带同一 CID 下的具体机构账户，禁止由账户反推或跨 CID 使用。
fn exact_institution_account_matches(cid_number: &[u8], account: &AccountId) -> bool {
    RuntimeInstitutionQuery::account_belongs_to(cid_number, account)
}

fn is_authorized_institution_actor(who: &AccountId, cid_number: &[u8]) -> bool {
    let Ok(text) = core::str::from_utf8(cid_number) else {
        return false;
    };
    let Some(institution_code) = primitives::cid::code::institution_code_from_cid_number(text)
    else {
        return false;
    };
    // 费用路由是调用者门：换绑管理员的机构 extrinsic 亦须放行，故用 CID 解析而非 account_id。
    RuntimeInstitutionAdminQuery::resolve_admin_account(institution_code, cid_number, who).is_some()
}

fn institution_fee_payer(who: &AccountId, cid_number: &[u8]) -> Option<AccountId> {
    if !is_authorized_institution_actor(who, cid_number) {
        return None;
    }
    exact_institution_fee_account(cid_number)
}

fn institution_onchain_route(
    who: &AccountId,
    cid_number: &[u8],
) -> primitives::fee_policy::FeeRoute<AccountId, Balance> {
    institution_onchain_amount_route(who, cid_number, 0)
}

fn institution_onchain_amount_route(
    who: &AccountId,
    cid_number: &[u8],
    transaction_amount: Balance,
) -> primitives::fee_policy::FeeRoute<AccountId, Balance> {
    match institution_fee_payer(who, cid_number) {
        Some(payer_account_id) => primitives::fee_policy::FeeRoute::Onchain {
            transaction_amount,
            payer_account_id,
        },
        None => primitives::fee_policy::FeeRoute::Reject,
    }
}

fn institution_account_onchain_route(
    who: &AccountId,
    cid_number: &[u8],
    institution_account_id: &AccountId,
) -> primitives::fee_policy::FeeRoute<AccountId, Balance> {
    if !exact_institution_account_matches(cid_number, institution_account_id) {
        return primitives::fee_policy::FeeRoute::Reject;
    }
    institution_onchain_route(who, cid_number)
}

fn proposal_operation_route(
    who: &AccountId,
    proposal_id: u64,
) -> primitives::fee_policy::FeeRoute<AccountId, Balance> {
    let Some(proposal) = votingengine::Pallet::<Runtime>::proposals(proposal_id) else {
        return primitives::fee_policy::FeeRoute::Reject;
    };
    match proposal.actor_cid_number {
        Some(cid_number) => institution_onchain_route(who, cid_number.as_slice()),
        None => signer_onchain_route(who, 0),
    }
}

impl onchain::CallFeeRoute<AccountId, RuntimeCall, Balance> for RuntimeFeeRouter {
    fn fee_route(
        who: &AccountId,
        call: &RuntimeCall,
    ) -> primitives::fee_policy::FeeRoute<AccountId, Balance> {
        use primitives::fee_policy::FeeRoute;

        match call {
            RuntimeCall::OnchainTransaction(onchain::pallet::Call::transfer_with_remark {
                amount,
                ..
            }) => signer_onchain_route(who, *amount),

            // 个人多签不是机构；创建提案和管理员变更属于普通链上操作，只有 cast 才是投票。
            RuntimeCall::PersonalManage(personal_manage::pallet::Call::propose_create {
                ..
            })
            | RuntimeCall::PersonalManage(personal_manage::pallet::Call::propose_close {
                ..
            })
            | RuntimeCall::PersonalAdmins(
                personal_admins::pallet::Call::propose_admin_set_change { .. },
            ) => signer_onchain_route(who, 0),

            // 注册局机构操作：管理员只签名，交易费严格从 actor CID 的费用账户扣取。
            RuntimeCall::PublicManage(
                public_manage::pallet::Call::update_institution_info {
                    actor_cid_number, ..
                }
                | public_manage::pallet::Call::propose_institution_governance {
                    actor_cid_number,
                    ..
                }
                | public_manage::pallet::Call::register_institution_admins {
                    actor_cid_number, ..
                },
            ) => institution_onchain_route(who, actor_cid_number.as_slice()),
            // 新增账户已改为本机构自身提案：交易费从本机构(cid_number)费用账户扣取,
            // 与 propose_institution_governance 等机构自身发起的操作同口径。
            RuntimeCall::PublicManage(
                public_manage::pallet::Call::propose_add_institution_account { cid_number, .. },
            ) => institution_onchain_route(who, cid_number.as_slice()),
            RuntimeCall::PublicManage(
                public_manage::pallet::Call::propose_close_public_institution {
                    actor_cid_number,
                    institution_account_id,
                    ..
                },
            ) => institution_account_onchain_route(
                who,
                actor_cid_number.as_slice(),
                institution_account_id,
            ),
            RuntimeCall::PrivateManage(
                private_manage::pallet::Call::update_institution_info {
                    actor_cid_number, ..
                }
                | private_manage::pallet::Call::propose_institution_governance {
                    actor_cid_number,
                    ..
                }
                | private_manage::pallet::Call::register_institution_admins {
                    actor_cid_number, ..
                },
            ) => institution_onchain_route(who, actor_cid_number.as_slice()),
            // 新增账户已改为本机构自身提案：交易费从本机构(cid_number)费用账户扣取,
            // 与 propose_institution_governance 等机构自身发起的操作同口径。
            RuntimeCall::PrivateManage(
                private_manage::pallet::Call::propose_add_institution_account {
                    cid_number, ..
                },
            ) => institution_onchain_route(who, cid_number.as_slice()),
            RuntimeCall::PrivateManage(
                private_manage::pallet::Call::propose_close_private_institution {
                    actor_cid_number,
                    institution_account_id,
                    ..
                },
            ) => institution_account_onchain_route(
                who,
                actor_cid_number.as_slice(),
                institution_account_id,
            ),

            RuntimeCall::AddressRegistry(
                address_registry::pallet::Call::set_catalog_version {
                    actor_cid_number, ..
                }
                | address_registry::pallet::Call::set_address_name {
                    actor_cid_number, ..
                }
                | address_registry::pallet::Call::remove_address_name {
                    actor_cid_number, ..
                }
                | address_registry::pallet::Call::set_address {
                    actor_cid_number, ..
                }
                | address_registry::pallet::Call::remove_address {
                    actor_cid_number, ..
                },
            ) => institution_onchain_route(who, actor_cid_number.as_slice()),

            // 框架固有、共识公益和 Root/内部维护调用免费。
            RuntimeCall::System(_)
            | RuntimeCall::Timestamp(_)
            | RuntimeCall::CitizenIssuance(_)
            | RuntimeCall::Grandpa(_) => FeeRoute::Free,
            RuntimeCall::ResolutionIssuance(
                resolution_issuance::pallet::Call::propose_issuance {
                    actor_cid_number, ..
                },
            ) => institution_onchain_route(who, actor_cid_number.as_slice()),
            RuntimeCall::ResolutionDestroy(resolution_destroy::pallet::Call::propose_destroy {
                actor_cid_number,
                institution_account_id,
                ..
            }) => institution_account_onchain_route(
                who,
                actor_cid_number.as_slice(),
                institution_account_id,
            ),

            RuntimeCall::VotingEngine(votingengine::pallet::Call::finalize_proposal { .. }) => {
                FeeRoute::Free
            }
            RuntimeCall::VotingEngine(
                votingengine::pallet::Call::retry_passed_proposal { proposal_id }
                | votingengine::pallet::Call::cancel_passed_proposal { proposal_id, .. },
            ) => proposal_operation_route(who, *proposal_id),

            RuntimeCall::CitizenIdentity(
                citizen_identity::pallet::Call::self_occupy_cid { .. }
                | citizen_identity::pallet::Call::self_rebind_cid_account_id { .. },
            ) => signer_onchain_route(who, 0),

            RuntimeCall::CitizenIdentity(
                citizen_identity::pallet::Call::register_voting_identity {
                    actor_cid_number, ..
                }
                | citizen_identity::pallet::Call::upgrade_to_candidate_identity {
                    actor_cid_number,
                    ..
                }
                | citizen_identity::pallet::Call::update_voting_identity {
                    actor_cid_number, ..
                }
                | citizen_identity::pallet::Call::update_candidate_identity {
                    actor_cid_number, ..
                }
                | citizen_identity::pallet::Call::revoke_identity {
                    actor_cid_number, ..
                }
                | citizen_identity::pallet::Call::occupy_cid {
                    actor_cid_number, ..
                }
                | citizen_identity::pallet::Call::admin_rebind_cid_account_id {
                    actor_cid_number,
                    ..
                }
                | citizen_identity::pallet::Call::revoke_cid {
                    actor_cid_number, ..
                },
            ) => institution_onchain_route(who, actor_cid_number.as_slice()),

            // 广场内容域：只有发帖、订阅、取消、换档和创作者档位管理属于签名动作；
            // 到期续费由 runtime 内部时间戳调度执行，不进入外部 call 路由。
            RuntimeCall::SquarePost(
                square_post::pallet::Call::publish_post { .. }
                | square_post::pallet::Call::subscribe { .. }
                | square_post::pallet::Call::cancel { .. }
                | square_post::pallet::Call::set_creator_plans { .. }
                | square_post::pallet::Call::change_subscription_plan { .. },
            )
            | RuntimeCall::FullnodeIssuance(
                fullnode_issuance::pallet::Call::bind_reward_account { .. }
                | fullnode_issuance::pallet::Call::rebind_reward_account { .. },
            ) => signer_onchain_route(who, 0),
            RuntimeCall::SquarePost(square_post::pallet::Call::propose_set_platform_price {
                actor_cid_number,
                ..
            }) => institution_onchain_route(who, actor_cid_number.as_slice()),

            RuntimeCall::RuntimeUpgrade(
                runtime_upgrade::pallet::Call::propose_runtime_upgrade {
                    actor_cid_number, ..
                }
                | runtime_upgrade::pallet::Call::developer_direct_upgrade {
                    actor_cid_number, ..
                },
            ) => institution_onchain_route(who, actor_cid_number.as_slice()),
            RuntimeCall::GrandpaKeyChange(
                grandpakey_change::pallet::Call::propose_emergency_grandpa_key_recovery {
                    actor_cid_number,
                    ..
                }
                | grandpakey_change::pallet::Call::schedule_grandpa_key_rotation {
                    actor_cid_number,
                    ..
                },
            ) => institution_onchain_route(who, actor_cid_number.as_slice()),
            RuntimeCall::LegislationYuan(
                legislation_yuan::pallet::Call::propose_enact_law {
                    actor_cid_number, ..
                }
                | legislation_yuan::pallet::Call::propose_amend_law {
                    actor_cid_number, ..
                }
                | legislation_yuan::pallet::Call::propose_repeal_law {
                    actor_cid_number, ..
                },
            ) => institution_onchain_route(who, actor_cid_number.as_slice()),

            RuntimeCall::MultisigTransfer(multisig::pallet::Call::propose_transfer {
                actor_cid_number,
                funding_account_id,
                ..
            }) => match actor_cid_number {
                Some(cid_number) => institution_account_onchain_route(
                    who,
                    cid_number.as_slice(),
                    funding_account_id,
                ),
                None => signer_onchain_route(who, 0),
            },
            RuntimeCall::MultisigTransfer(
                multisig::pallet::Call::propose_safety_fund_transfer {
                    actor_cid_number,
                    institution_account_id,
                    ..
                }
                | multisig::pallet::Call::propose_sweep_to_main {
                    actor_cid_number,
                    institution_account_id,
                    ..
                },
            ) => institution_account_onchain_route(
                who,
                actor_cid_number.as_slice(),
                institution_account_id,
            ),

            RuntimeCall::OffchainTransaction(offchain::pallet::Call::bind_clearing_bank {
                ..
            })
            | RuntimeCall::OffchainTransaction(offchain::pallet::Call::switch_bank { .. }) => {
                signer_onchain_route(who, 0)
            }
            RuntimeCall::OffchainTransaction(offchain::pallet::Call::deposit { amount })
            | RuntimeCall::OffchainTransaction(offchain::pallet::Call::withdraw { amount }) => {
                signer_onchain_route(who, *amount)
            }
            RuntimeCall::OffchainTransaction(offchain::pallet::Call::submit_offchain_batch {
                actor_cid_number,
                institution_account_id,
                batch,
                ..
            }) => {
                if !exact_institution_account_matches(
                    actor_cid_number.as_slice(),
                    institution_account_id,
                ) {
                    return FeeRoute::Reject;
                }
                // 链下费用由各 item 的付款公民承担；这里仍须验证提交机构管理员，
                // 并保证作为手续费收款方的机构费用账户唯一且正反索引一致。
                if !is_authorized_institution_actor(who, actor_cid_number.as_slice())
                    || exact_institution_fee_account(actor_cid_number.as_slice()).is_none()
                {
                    return FeeRoute::Reject;
                }
                let fee_amount = batch
                    .iter()
                    .fold(0u128, |sum, item| sum.saturating_add(item.fee_amount));
                FeeRoute::Offchain {
                    fee_amount,
                    payer_account_id: primitives::fee_policy::OffchainFeePayer::BatchItemPayers,
                }
            }
            RuntimeCall::OffchainTransaction(offchain::pallet::Call::propose_l2_fee_rate {
                actor_cid_number,
                institution_account_id,
                ..
            }) => institution_account_onchain_route(
                who,
                actor_cid_number.as_slice(),
                institution_account_id,
            ),
            RuntimeCall::OffchainTransaction(
                offchain::pallet::Call::register_clearing_bank {
                    actor_cid_number, ..
                }
                | offchain::pallet::Call::update_clearing_bank_endpoint {
                    actor_cid_number, ..
                }
                | offchain::pallet::Call::unregister_clearing_bank {
                    actor_cid_number, ..
                },
            ) => institution_onchain_route(who, actor_cid_number.as_slice()),
            RuntimeCall::OffchainTransaction(offchain::pallet::Call::set_max_l2_fee_rate {
                ..
            }) => FeeRoute::Free,

            // 只有实际投票/表决动作支付固定 1 元，并且始终由投票签名者本人支付。
            RuntimeCall::InternalVote(internal_vote::pallet::Call::cast { .. })
            | RuntimeCall::JointVote(joint_vote::pallet::Call::cast_admin { .. })
            | RuntimeCall::JointVote(joint_vote::pallet::Call::cast_referendum { .. })
            | RuntimeCall::LegislationVote(
                legislation_vote::pallet::Call::cast_representative_vote { .. }
                | legislation_vote::pallet::Call::cast_referendum_vote { .. }
                | legislation_vote::pallet::Call::executive_sign { .. }
                | legislation_vote::pallet::Call::override_sign { .. }
                | legislation_vote::pallet::Call::guard_vote { .. },
            )
            | RuntimeCall::ElectionVote(
                election_vote::pallet::Call::cast_popular_vote { .. }
                | election_vote::pallet::Call::cast_mutual_vote { .. },
            ) => signer_vote_route(who),
            // onchain-issuance 当前 10 个公开 call 都是明确的业务占位，授权后直接
            // `Ok(())`，尚未创建投票或执行资产逻辑。未实装前统一拒绝，禁止形成
            // “扣了机构操作费但没有业务结果”的假交易。
            RuntimeCall::OnchainIssuance(_) => FeeRoute::Reject,

            // FRAME call enum 为元数据稳定性生成 `__Ignore` 隐藏分支；每个业务 pallet
            // 仅把未显式列出的内部 call 归为 Reject。外层 RuntimeCall 不设通配分支，
            // 因此新增 pallet 仍会触发编译期 non-exhaustive 错误。
            RuntimeCall::OnchainTransaction(_)
            | RuntimeCall::FullnodeIssuance(_)
            | RuntimeCall::ResolutionIssuance(_)
            | RuntimeCall::VotingEngine(_)
            | RuntimeCall::InternalVote(_)
            | RuntimeCall::JointVote(_)
            | RuntimeCall::ElectionVote(_)
            | RuntimeCall::CitizenIdentity(_)
            | RuntimeCall::RuntimeUpgrade(_)
            | RuntimeCall::ResolutionDestroy(_)
            | RuntimeCall::GrandpaKeyChange(_)
            | RuntimeCall::PersonalManage(_)
            | RuntimeCall::PersonalAdmins(_)
            | RuntimeCall::MultisigTransfer(_)
            | RuntimeCall::OffchainTransaction(_)
            | RuntimeCall::LegislationYuan(_)
            | RuntimeCall::LegislationVote(_)
            | RuntimeCall::PublicManage(_)
            | RuntimeCall::PrivateManage(_)
            | RuntimeCall::AddressRegistry(_)
            | RuntimeCall::SquarePost(_) => FeeRoute::Reject,

            // 两个内核 pallet 的外部入口被 BaseCallFilter 禁用；显式 Reject，不伪装成免费。
            RuntimeCall::Assets(_) | RuntimeCall::Balances(_) => FeeRoute::Reject,
        }
    }
}

/// 省储行利息模块配置：
/// - 使用 Balances 作为铸币/记账货币
/// - 每年区块数统一采用 primitives 中的制度常量
impl provincialbank_interest::Config for Runtime {
    type Currency = Balances;
    type BlocksPerYear = ConstU64<{ primitives::pow_const::BLOCKS_PER_YEAR }>;
    type WeightInfo = provincialbank_interest::weights::SubstrateWeight<Runtime>;
}

/// PoW 作者解析器：
/// 从区块 pre-runtime digest 中读取 POW_ENGINE_ID 的负载（sr25519 公钥），
/// 派生为 AccountId。配合 seal 中的签名实现矿工身份密码学绑定。
pub struct PowDigestAuthor;

impl FindAuthor<AccountId> for PowDigestAuthor {
    fn find_author<'a, I>(digests: I) -> Option<AccountId>
    where
        I: 'a + IntoIterator<Item = (sp_runtime::ConsensusEngineId, &'a [u8])>,
    {
        digests.into_iter().find_map(|(engine_id, data)| {
            if engine_id == sp_consensus_pow::POW_ENGINE_ID {
                sp_core::sr25519::Public::decode(&mut &data[..])
                    .ok()
                    .map(|public| {
                        use sp_runtime::traits::IdentifyAccount;
                        sp_runtime::MultiSigner::from(public).into_account()
                    })
            } else {
                None
            }
        })
    }
}

/// 全节点发行模块配置：
/// - 链上货币使用 Balances
/// - 作者识别完全基于 PoW digest（不依赖 Aura/Grandpa）
impl fullnode_issuance::Config for Runtime {
    type Currency = Balances;
    type FindAuthor = PowDigestAuthor;
    type WeightInfo = fullnode_issuance::weights::SubstrateWeight<Runtime>;
}

pub struct RuntimeAccountValidator;

impl primitives::multisig::AccountValidator<AccountId> for RuntimeAccountValidator {
    fn is_valid(account: &AccountId) -> bool {
        // 禁止零账户。
        if account == &AccountId::new([0u8; 32]) {
            return false;
        }

        // 禁止占用“国家储委会/省储委会”的制度保留交易账户。
        if primitives::cid::china::china_cb::CHINA_CB
            .iter()
            .any(|n| account == &AccountId::new(n.main_account))
        {
            return false;
        }

        // 禁止占用“省储行”的制度保留交易账户。
        if primitives::cid::china::china_ch::CHINA_CH
            .iter()
            .any(|n| account == &AccountId::new(n.main_account))
        {
            return false;
        }

        // 禁止占用省储行费用账户（BLAKE2-256 派生）。
        if primitives::cid::china::china_ch::CHINA_CH
            .iter()
            .any(|n| account == &AccountId::new(n.fee_account))
        {
            return false;
        }

        // 禁止占用国家储委会安全基金账户。
        if is_safety_fund_account(account) {
            return false;
        }

        // 禁止占用国家储委会两和基金账户。
        if is_nrc_he_account(account) {
            return false;
        }

        // 禁止占用储委会费用账户（44 个机构）。
        if is_cb_fee_account(account) {
            return false;
        }

        true
    }
}

pub struct RuntimeReservedAccountGuard;
pub struct RuntimeRegistryAuthority;

pub struct RuntimeProtectedSourceChecker;
pub struct RuntimeInstitutionAsset;

impl primitives::multisig::ProtectedSourceChecker<AccountId> for RuntimeProtectedSourceChecker {
    fn is_protected(address: &AccountId) -> bool {
        is_stake_account(address)
    }
}

impl primitives::institution_asset::InstitutionAsset<AccountId> for RuntimeInstitutionAsset {
    fn can_spend(
        source: &AccountId,
        action: primitives::institution_asset::InstitutionAssetAction,
    ) -> bool {
        // 匹配顺序很重要——更具体的账户类型必须放在更宽泛的类型之前。
        // fee_account 同时出现在 CHINA_RESERVED_MAIN_ACCOUNTS 列表中（同由 BLAKE2 派生且统一保留），
        // 如果 is_reserved_main_account 先匹配，fee_account 会被错误地按主账户规则放行。

        // 1. 无私钥系统账户：全禁
        if is_stake_account(source) {
            return false;
        }

        // 2. 省储行费用账户（最具体）：只允许手续费归集
        if is_reserved_fee_account(source) {
            return matches!(
                action,
                primitives::institution_asset::InstitutionAssetAction::OffchainFeeSweepExecute
            );
        }

        // 3. 储委会费用账户（44 个机构）：只允许手续费归集
        if is_cb_fee_account(source) {
            return matches!(
                action,
                primitives::institution_asset::InstitutionAssetAction::OffchainFeeSweepExecute
            );
        }

        // 4. 公民链基金会费用账户：同样只允许手续费归集；必须先于宽泛保留账户匹配。
        if is_citizenchain_fee_account(source) {
            return matches!(
                action,
                primitives::institution_asset::InstitutionAssetAction::OffchainFeeSweepExecute
            );
        }

        // 5. 国家储委会安全基金账户：只允许安全基金转账
        if source == &AccountId::new(primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT) {
            return matches!(
                action,
                primitives::institution_asset::InstitutionAssetAction::NrcSafetyFundTransfer
            );
        }

        // 6. 多签保留主账户：可为机构创建提供本金，也可执行多签转账和关闭。
        if is_reserved_main_account(source) {
            return matches!(
                action,
                primitives::institution_asset::InstitutionAssetAction::InstitutionCreateFunding
                    | primitives::institution_asset::InstitutionAssetAction::MultisigTransferExecute
                    | primitives::institution_asset::InstitutionAssetAction::MultisigCloseExecute
            );
        }

        // 7. 清算行【清算账户】：装用户 L2 存款准备金。只准扫码清算扣款与用户提现；
        //    管理员经 multisig-transfer / 关闭 / 机构注资一律拒绝，堵死挪用池子。
        //    清算行主账户不命中此谓词，仍走普通账户放行（银行自有营运资金照常可动）。
        if is_clearing_account(source) {
            return matches!(
                action,
                primitives::institution_asset::InstitutionAssetAction::L2ClearingDebit
                    | primitives::institution_asset::InstitutionAssetAction::L3WithdrawOut
            );
        }

        // 8. 联邦公民安全基金（FSC 专属）：只允许经 FSC 内部投票的多签转账支出。
        //    该基金由联邦安全局的 LR + 局长两岗经内部投票（2/2 严格过半）复用通用多签
        //    转账动作从中转出；其余资金动作（注资/关闭/费用归集等）一律拒绝，杜绝旁路挪用。
        if is_federal_citizen_security_fund_account(source) {
            return matches!(
                action,
                primitives::institution_asset::InstitutionAssetAction::MultisigTransferExecute
            );
        }

        // 9. 普通账户：全放行
        true
    }
}

impl primitives::multisig::ReservedAccountGuard<AccountId> for RuntimeReservedAccountGuard {
    fn is_reserved(account: &AccountId) -> bool {
        // 禁止占用省储行 stake_account（制度保留账户）。
        if primitives::cid::china::china_ch::CHINA_CH
            .iter()
            .any(|n| account == &AccountId::new(n.stake_account))
        {
            return true;
        }

        // 禁止占用省储行费用账户（BLAKE2-256 派生）。
        if primitives::cid::china::china_ch::CHINA_CH
            .iter()
            .any(|n| account == &AccountId::new(n.fee_account))
        {
            return true;
        }

        // 禁止占用国家储委会安全基金账户。
        if is_safety_fund_account(account) {
            return true;
        }

        // 禁止占用国家储委会两和基金账户。
        if is_nrc_he_account(account) {
            return true;
        }

        // 禁止占用储委会费用账户（44 个机构）。
        if is_cb_fee_account(account) {
            return true;
        }

        // 禁止占用任一 SFGF 清算账户地址（承载 L2 用户存款准备金）。
        // 按名注册已由 account_derive::is_forbidden_account_name 挡下，此处补地址级保护。
        if is_clearing_account(account) {
            return true;
        }

        // 禁止占用联邦安全局的联邦公民安全基金账户地址。
        if is_federal_citizen_security_fund_account(account) {
            return true;
        }

        is_reserved_main_account(account)
    }
}

fn cid_institution_code(cid_number: &[u8]) -> Option<primitives::cid::code::InstitutionCode> {
    let text = core::str::from_utf8(cid_number).ok()?;
    primitives::cid::code::institution_code_from_cid_number(text.trim())
}

impl entity_primitives::RegistryAuthority<AccountId> for RuntimeRegistryAuthority {
    fn can_register_institution_origin(
        registrar: &AccountId,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        target_cid_number: &[u8],
        target_institution_code: primitives::cid::code::InstitutionCode,
    ) -> bool {
        let Some(actor_code) = cid_institution_code(actor_cid_number) else {
            return false;
        };
        let role_subject = entity_primitives::RoleSubject {
            cid_number: actor_cid_number.to_vec(),
            role_code: actor_role_code.to_vec(),
        };
        if !RuntimeInstitutionRoleAuthorization::is_authorized(
            registrar,
            &role_subject,
            &entity_primitives::BusinessActionId {
                module_tag: entity_primitives::business_action::MODULE_INSTITUTION_REGISTRATION
                    .to_vec(),
                action_code: entity_primitives::business_action::ACTION_REGISTER_INSTITUTION,
            },
            RolePermissionOperation::Propose,
        ) {
            return false;
        }
        let Some(parsed_target_code) = cid_institution_code(target_cid_number) else {
            return false;
        };
        if parsed_target_code != target_institution_code
            || primitives::cid::code::is_fixed_governance_code(&target_institution_code)
            || primitives::institution_constraints::is_permanent_singleton_code(
                &target_institution_code,
            )
        {
            return false;
        }

        let Ok((target_province_code, target_city_code)) =
            primitives::cid::number::cid_scope_codes(target_cid_number)
        else {
            return false;
        };

        const CITY_REGISTRY_CODE: primitives::cid::code::InstitutionCode = *b"CREG";
        if actor_code == admin_primitives::FRG {
            let expected = primitives::governance_skeleton::province_commissioner_role_code(
                target_province_code,
            );
            return actor_role_code == expected.as_slice();
        }

        if actor_code == CITY_REGISTRY_CODE {
            if target_institution_code == CITY_REGISTRY_CODE {
                return false;
            }
            let Ok((issuer_province_code, issuer_city_code)) =
                primitives::cid::number::cid_scope_codes(actor_cid_number)
            else {
                return false;
            };
            // CREG 只能登记本市非 CREG 机构;市归属由 CID R5 直接校验。
            return issuer_province_code == target_province_code
                && issuer_city_code == target_city_code;
        }

        false
    }
}

pub struct RuntimeAddressAuthority;

impl address_registry::AddressUpdateAuthority<AccountId> for RuntimeAddressAuthority {
    fn can_update_catalog(
        who: &AccountId,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        action_code: u32,
    ) -> bool {
        let Ok(actor_text) = core::str::from_utf8(actor_cid_number) else {
            return false;
        };
        if primitives::cid::code::institution_code_from_cid_number(actor_text)
            != Some(primitives::cid::code::FRG)
        {
            return false;
        }
        let role_subject = entity_primitives::RoleSubject {
            cid_number: actor_cid_number.to_vec(),
            role_code: actor_role_code.to_vec(),
        };
        RuntimeInstitutionRoleAuthorization::is_authorized(
            who,
            &role_subject,
            &entity_primitives::BusinessActionId {
                module_tag: entity_primitives::business_action::MODULE_ADDRESS_REGISTRY.to_vec(),
                action_code,
            },
            RolePermissionOperation::Propose,
        )
    }

    fn can_update_address(
        who: &AccountId,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        province_code: &[u8],
        city_code: &[u8],
        action_code: u32,
    ) -> bool {
        if province_code.is_empty() || city_code.is_empty() {
            return false;
        }
        let Ok(actor_text) = core::str::from_utf8(actor_cid_number) else {
            return false;
        };
        let Some(actor_code) = primitives::cid::code::institution_code_from_cid_number(actor_text)
        else {
            return false;
        };
        let role_subject = entity_primitives::RoleSubject {
            cid_number: actor_cid_number.to_vec(),
            role_code: actor_role_code.to_vec(),
        };
        if !RuntimeInstitutionRoleAuthorization::is_authorized(
            who,
            &role_subject,
            &entity_primitives::BusinessActionId {
                module_tag: entity_primitives::business_action::MODULE_ADDRESS_REGISTRY.to_vec(),
                action_code,
            },
            RolePermissionOperation::Propose,
        ) {
            return false;
        }

        if actor_code == primitives::cid::code::FRG {
            if province_code.len() != 2 {
                return false;
            }
            let mut code = [0_u8; 2];
            code.copy_from_slice(province_code);
            return actor_role_code
                == primitives::governance_skeleton::province_commissioner_role_code(code)
                    .as_slice();
        }

        const CITY_REGISTRY_CODE: primitives::cid::code::InstitutionCode = *b"CREG";
        if actor_code != CITY_REGISTRY_CODE {
            return false;
        }
        let Ok((issuer_province_code, issuer_city_code)) =
            primitives::cid::number::cid_scope_codes(actor_cid_number)
        else {
            return false;
        };
        // CREG 管理员只能更新本市地址。镇以下地址名称与完整地址仍走本市注册局。
        issuer_province_code.as_ref() == province_code && issuer_city_code.as_ref() == city_code
    }
}

impl address_registry::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AddressAuthority = RuntimeAddressAuthority;
    type MaxCodeLen = ConstU32<16>;
    type MaxVersionLen = ConstU32<32>;
    type MaxAddressNameCodeLen = ConstU32<3>;
    type MaxAddressLocalNoLen = ConstU32<4>;
    type MaxAddressNameLen = ConstU32<96>;
    type MaxAddressDetailLen = ConstU32<128>;
}

fn sr25519_signature_from_bytes(signature: &[u8]) -> Option<sr25519::Signature> {
    if signature.len() != 64 {
        return None;
    }
    let mut sig_raw = [0u8; 64];
    sig_raw.copy_from_slice(signature);
    Some(sr25519::Signature::from_raw(sig_raw))
}

/// 公民身份四个签名域共用的唯一 sr25519 验签实现。
///
/// benchmark 与生产 runtime 必须执行同一函数；feature 只能改变 benchmark 夹具，
/// 绝不能改变密码学验证结果。
fn verify_citizen_identity_sr25519_signature(
    account_id: &AccountId,
    payload: &[u8],
    signature: &citizen_identity::pallet::SignatureOf<Runtime>,
    op_tag: u8,
) -> bool {
    let Ok(raw_account) = <[u8; 32]>::try_from(account_id.as_ref()) else {
        return false;
    };
    let Some(signature) = sr25519_signature_from_bytes(signature.as_slice()) else {
        return false;
    };
    let public = sr25519::Public::from_raw(raw_account);
    let message = primitives::sign::signing_message(op_tag, payload);
    sr25519_verify(&signature, &message, &public)
}

/// FRAME benchmark 的临时 sr25519 签名夹具。
///
/// 使用 Polkadot SDK host crypto 把私钥只放在 benchmark externalities keystore；
/// runtime 代码和链上 Storage 均不保存 benchmark 私钥。
#[cfg(feature = "runtime-benchmarks")]
pub struct RuntimeCitizenIdentityBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl citizen_identity::BenchmarkHelper<AccountId, citizen_identity::pallet::SignatureOf<Runtime>>
    for RuntimeCitizenIdentityBenchmarkHelper
{
    fn signer() -> (sr25519::Public, AccountId) {
        let public = sp_io::crypto::sr25519_generate(0.into(), None);
        let account_id = MultiSigner::Sr25519(public).into_account();
        (public, account_id)
    }

    #[allow(clippy::expect_used)]
    fn sign(
        signer: &sr25519::Public,
        message: &[u8],
    ) -> citizen_identity::pallet::SignatureOf<Runtime> {
        // BenchmarkHelper trait 不返回 Result；夹具缺失或越界属于基准环境配置错误，必须停止。
        let signature = sp_io::crypto::sr25519_sign(0.into(), signer, message)
            .expect("benchmark keystore must contain the generated sr25519 signer");
        signature
            .0
            .to_vec()
            .try_into()
            .expect("sr25519 signature must fit the runtime signature bound")
    }
}

// 机构自定义账户关闭由签名账户提交明确 CID 与岗位码；业务 pallet 通过统一岗位授权
// 查询同时校验管理员名册、有效任职和 BusinessActionId，不保留独立审批凭证。

/// 完整 CID 的顶层业务能力策略：固定创世机构走共享白名单，普通机构仅开放自身治理。
pub struct RuntimeInstitutionCapabilityPolicy;

impl entity_primitives::InstitutionCapabilityPolicy for RuntimeInstitutionCapabilityPolicy {
    fn allows(
        cid_number: &[u8],
        business_action_id: &entity_primitives::BusinessActionId<Vec<u8>>,
        operation: entity_primitives::RolePermissionOperation,
    ) -> bool {
        let Ok(parts) = primitives::cid::number::parse_cid_number_parts_bytes(cid_number) else {
            return false;
        };
        let public_cid =
            public_manage::pallet::CidNumberOf::<Runtime>::try_from(cid_number.to_vec()).ok();
        let private_cid =
            private_manage::pallet::CidNumberOf::<Runtime>::try_from(cid_number.to_vec()).ok();
        let in_public = public_cid
            .as_ref()
            .is_some_and(public_manage::Institutions::<Runtime>::contains_key);
        let in_private = private_cid
            .as_ref()
            .is_some_and(private_manage::Institutions::<Runtime>::contains_key);
        let expected_module = match (in_public, in_private) {
            (true, false) => public_manage::MODULE_TAG,
            (false, true) => private_manage::MODULE_TAG,
            _ => return false,
        };
        if entity_primitives::fixed_institution_capability_allows(
            parts.institution,
            cid_number,
            business_action_id.module_tag.as_slice(),
            business_action_id.action_code,
            operation,
        ) {
            return true;
        }
        if in_public
            && entity_primitives::business_action::legislation_institution_capability_allows(
                parts.institution,
                business_action_id.module_tag.as_slice(),
                business_action_id.action_code,
                operation,
            )
        {
            return true;
        }
        business_action_id.module_tag.as_slice() == expected_module
            && matches!(
                business_action_id.action_code,
                entity_primitives::business_action::ACTION_INSTITUTION_CLOSE
                    | entity_primitives::business_action::ACTION_INSTITUTION_GOVERNANCE
            )
    }
}

impl public_manage::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type InternalVoteEngine = InternalVote;
    type AdminLifecycle = PublicAdmins;
    type SiblingInstitutionQuery = PrivateManage;
    type InstitutionAdminQuery = RuntimeInstitutionAdminQuery;
    type InstitutionCapabilityPolicy = RuntimeInstitutionCapabilityPolicy;
    type AccountValidator = RuntimeAccountValidator;
    type ReservedAccountChecker = RuntimeReservedAccountGuard;
    type ProtectedSourceChecker = RuntimeProtectedSourceChecker;
    type InstitutionAsset = RuntimeInstitutionAsset;
    type InstitutionQuery = RuntimeInstitutionQuery;
    type OnchainFeeCharger =
        onchain::OnchainExecutionFeeCharger<Runtime, Balances, OnchainExecutionFeeDistributor>;
    type RegistryAuthority = RuntimeRegistryAuthority;
    type MaxAdmins = MaxAdminsPerInstitution;
    type MaxCidNumberLength = ConstU32<{ primitives::core_const::CID_NUMBER_MAX_BYTES }>;
    type MaxAccountNameLength = ConstU32<128>;
    type MaxInstitutionAccounts = ConstU32<16>;
    type WeightInfo = public_manage::weights::SubstrateWeight<Runtime>;
}

impl private_manage::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type InternalVoteEngine = InternalVote;
    type AdminLifecycle = PrivateAdmins;
    type SiblingInstitutionQuery = PublicManage;
    type InstitutionAdminQuery = RuntimeInstitutionAdminQuery;
    type InstitutionCapabilityPolicy = RuntimeInstitutionCapabilityPolicy;
    type AccountValidator = RuntimeAccountValidator;
    type ReservedAccountChecker = RuntimeReservedAccountGuard;
    type ProtectedSourceChecker = RuntimeProtectedSourceChecker;
    type InstitutionAsset = RuntimeInstitutionAsset;
    type InstitutionQuery = RuntimeInstitutionQuery;
    type OnchainFeeCharger =
        onchain::OnchainExecutionFeeCharger<Runtime, Balances, OnchainExecutionFeeDistributor>;
    type RegistryAuthority = RuntimeRegistryAuthority;
    type ChainPhase = GenesisPallet;
    type MaxAdmins = MaxAdminsPerInstitution;
    type MaxCidNumberLength = ConstU32<{ primitives::core_const::CID_NUMBER_MAX_BYTES }>;
    type MaxAccountNameLength = ConstU32<128>;
    type MaxInstitutionAccounts = ConstU32<16>;
    type WeightInfo = private_manage::weights::SubstrateWeight<Runtime>;
}

impl personal_manage::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type InternalVoteEngine = InternalVote;
    type AccountValidator = RuntimeAccountValidator;
    type ReservedAccountChecker = RuntimeReservedAccountGuard;
    type ProtectedSourceChecker = RuntimeProtectedSourceChecker;
    type InstitutionAsset = RuntimeInstitutionAsset;
    type PersonalAdminLifecycle = personal_admins::Pallet<Runtime>;
    type PersonalAdminQuery = personal_admins::Pallet<Runtime>;
    type OnchainFeeCharger =
        onchain::OnchainExecutionFeeCharger<Runtime, Balances, OnchainExecutionFeeDistributor>;
    type MaxAccountNameLength = ConstU32<128>;
    type MaxPersonalAccountAdmins = MaxPersonalAccountAdmins;
    type MinCreateAmount = ConstU128<111>;
    type WeightInfo = personal_manage::weights::SubstrateWeight<Runtime>;
}

impl personal_admins::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type InternalVoteEngine = InternalVote;
    type MaxPersonalAccountAdmins = MaxPersonalAccountAdmins;
    type WeightInfo = personal_admins::weights::SubstrateWeight<Runtime>;
}

pub struct RuntimeCitizenIdentityAuthority;

impl
    citizen_identity::CitizenIdentityAuthority<
        AccountId,
        citizen_identity::pallet::SignatureOf<Runtime>,
    > for RuntimeCitizenIdentityAuthority
{
    fn can_manage_voting_identity(
        registrar: &AccountId,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        residence_province_code: &[u8],
        residence_city_code: &[u8],
        _level: citizen_identity::CitizenIdentityLevel,
        action_code: u32,
    ) -> bool {
        if residence_province_code.is_empty() || residence_city_code.is_empty() {
            return false;
        }
        let Ok(actor_text) = core::str::from_utf8(actor_cid_number) else {
            return false;
        };
        let Some(actor_code) = primitives::cid::code::institution_code_from_cid_number(actor_text)
        else {
            return false;
        };
        let role_subject = entity_primitives::RoleSubject {
            cid_number: actor_cid_number.to_vec(),
            role_code: actor_role_code.to_vec(),
        };
        let business_action_id = entity_primitives::BusinessActionId {
            module_tag: entity_primitives::business_action::MODULE_CITIZEN_IDENTITY.to_vec(),
            action_code,
        };
        if !RuntimeInstitutionRoleAuthorization::is_authorized(
            registrar,
            &role_subject,
            &business_action_id,
            RolePermissionOperation::Propose,
        ) {
            return false;
        }

        if actor_code == primitives::cid::code::FRG {
            let mut province = [0_u8; 2];
            if residence_province_code.len() != province.len() {
                return false;
            }
            province.copy_from_slice(residence_province_code);
            return actor_role_code
                == primitives::governance_skeleton::province_commissioner_role_code(province)
                    .as_slice();
        }

        const CITY_REGISTRY_CODE: primitives::cid::code::InstitutionCode = *b"CREG";
        if actor_code != CITY_REGISTRY_CODE {
            return false;
        }
        let Ok((registry_province_code, registry_city_code)) =
            primitives::cid::number::cid_scope_codes(actor_cid_number)
        else {
            return false;
        };
        // CREG 管理员只能管理本市公民身份；出生地不参与居住地注册权限。
        registry_province_code.as_ref() == residence_province_code
            && registry_city_code.as_ref() == residence_city_code
    }

    fn verify_citizen_signature(
        account_id: &AccountId,
        payload: &[u8],
        signature: &citizen_identity::pallet::SignatureOf<Runtime>,
    ) -> bool {
        verify_citizen_identity_sr25519_signature(
            account_id,
            payload,
            signature,
            primitives::sign::OP_SIGN_CITIZEN_IDENTITY,
        )
    }

    fn verify_rebind_signature(
        account_id: &AccountId,
        payload: &[u8],
        signature: &citizen_identity::pallet::SignatureOf<Runtime>,
    ) -> bool {
        verify_citizen_identity_sr25519_signature(
            account_id,
            payload,
            signature,
            primitives::sign::OP_SIGN_CID_REBIND,
        )
    }

    fn can_manage_anonymous_cid(
        registrar: &AccountId,
        actor_cid_number: &[u8],
        actor_role_code: &[u8],
        action_code: u32,
    ) -> bool {
        let Ok(actor_text) = core::str::from_utf8(actor_cid_number) else {
            return false;
        };
        let Some(actor_code) = primitives::cid::code::institution_code_from_cid_number(actor_text)
        else {
            return false;
        };
        // 匿名 CID 为全国号、无省市归属:只要求发起主体是在册注册局(省级 FRG /
        // 市级 CREG)且持 citizen-identity 管理权,不做辖区匹配。
        const CITY_REGISTRY_CODE: primitives::cid::code::InstitutionCode = *b"CREG";
        if actor_code != primitives::cid::code::FRG && actor_code != CITY_REGISTRY_CODE {
            return false;
        }
        let role_subject = entity_primitives::RoleSubject {
            cid_number: actor_cid_number.to_vec(),
            role_code: actor_role_code.to_vec(),
        };
        let business_action_id = entity_primitives::BusinessActionId {
            module_tag: entity_primitives::business_action::MODULE_CITIZEN_IDENTITY.to_vec(),
            action_code,
        };
        RuntimeInstitutionRoleAuthorization::is_authorized(
            registrar,
            &role_subject,
            &business_action_id,
            RolePermissionOperation::Propose,
        )
    }

    fn verify_occupy_signature(
        account_id: &AccountId,
        payload: &[u8],
        signature: &citizen_identity::pallet::SignatureOf<Runtime>,
    ) -> bool {
        verify_citizen_identity_sr25519_signature(
            account_id,
            payload,
            signature,
            primitives::sign::OP_SIGN_CID_OCCUPY,
        )
    }

    fn verify_admin_rebind_signature(
        account_id: &AccountId,
        payload: &[u8],
        signature: &citizen_identity::pallet::SignatureOf<Runtime>,
    ) -> bool {
        verify_citizen_identity_sr25519_signature(
            account_id,
            payload,
            signature,
            primitives::sign::OP_SIGN_CID_ADMIN_REBIND,
        )
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_authority() -> Option<(
        AccountId,
        citizen_identity::CidNumberBound,
        citizen_identity::RoleCodeBound,
        citizen_identity::AreaCodeBound,
        citizen_identity::AreaCodeBound,
    )> {
        // 基准 externalities 使用 fresh spec-genesis；这里选择 FRG 首个省专员岗位，
        // 不伪造“管理员即有权”的旁路。计时区间内仍由正式岗位权限目录完成授权。
        let province = primitives::cid::code::PROVINCE_CODE_INFOS
            .first()?
            .province_code;
        let registrar =
            AccountId::new(*primitives::cid::china::china_zf::FEDERAL_REGISTRY_ADMINS.first()?);
        let institution = primitives::governance_skeleton::federal_registry_institution();
        Some((
            registrar,
            institution.cid_number.as_bytes().to_vec().try_into().ok()?,
            primitives::governance_skeleton::province_commissioner_role_code(province)
                .try_into()
                .ok()?,
            province.to_vec().try_into().ok()?,
            b"ZS01".to_vec().try_into().ok()?,
        ))
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_set_timestamp(timestamp_millis: u64) {
        pallet_timestamp::Pallet::<Runtime>::set_timestamp(timestamp_millis);
    }
}

impl citizen_identity::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type MaxCitizenSignatureLength = ConstU32<64>;
    type CitizenIdentityAuthority = RuntimeCitizenIdentityAuthority;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = RuntimeCitizenIdentityBenchmarkHelper;
    type OnVotingIdentityRegistered = CitizenIssuance;
    type TimeProvider = crate::Timestamp;
    type MaxPopulationDaysPerBlock = ConstU32<366>;
    type MaxPopulationTransitionsPerBlock = ConstU32<2_048>;
    type MaxPopulationMaintenanceWeightPerBlock =
        citizen_identity::PopulationMaintenanceWeightFraction<Runtime, 8>;
    type WeightInfo = citizen_identity::weights::SubstrateWeight<Runtime>;
}

/// 公权管理员公民 CID 与账户绑定校验器。
///
/// `citizen-identity` 同时维护 CID→账户与账户→CID 两条索引；这里只接受两条索引
/// 完全一致的正常绑定，不在管理员模块复制或修正公民身份数据。
pub struct RuntimePublicAdminCitizenIdentityBinding;

impl admin_primitives::CitizenIdentityBindingQuery<AccountId>
    for RuntimePublicAdminCitizenIdentityBinding
{
    fn matches_citizen_account(cid_number: &[u8], account: &AccountId) -> bool {
        let Ok(cid): Result<citizen_identity::CidNumberBound, _> = cid_number.to_vec().try_into()
        else {
            return false;
        };
        citizen_identity::Pallet::<Runtime>::citizen_subject(account)
            .is_some_and(|subject| subject.cid_number == cid)
    }
}

pub struct RuntimeSquarePostCitizenIdentity;

impl square_post::SquarePostCitizenIdentityProvider<AccountId>
    for RuntimeSquarePostCitizenIdentity
{
    fn active_cid_number(account_id: &AccountId) -> Option<Vec<u8>> {
        let cid_number = citizen_identity::CidByAccountId::<Runtime>::get(account_id)?;
        if citizen_identity::AccountIdByCid::<Runtime>::get(&cid_number).as_ref()
            != Some(account_id)
        {
            return None;
        }
        let record = citizen_identity::CidRegistry::<Runtime>::get(&cid_number)?;
        (record.status == citizen_identity::CidRecordStatus::Active).then(|| cid_number.to_vec())
    }

    fn current_account_id(cid_number: &[u8]) -> Option<AccountId> {
        let cid_number = citizen_identity::CidNumberBound::try_from(cid_number.to_vec()).ok()?;
        let record = citizen_identity::CidRegistry::<Runtime>::get(&cid_number)?;
        if record.status != citizen_identity::CidRecordStatus::Active {
            return None;
        }
        let account_id = citizen_identity::AccountIdByCid::<Runtime>::get(&cid_number)?;
        (citizen_identity::CidByAccountId::<Runtime>::get(&account_id).as_ref()
            == Some(&cid_number))
        .then_some(account_id)
    }

    fn is_campaign_eligible(cid_number: &[u8], account_id: &AccountId) -> bool {
        <citizen_identity::Pallet<Runtime> as citizen_identity::CitizenIdentityProvider<
            AccountId,
        >>::candidate_subject(account_id, &citizen_identity::PopulationScope::Country)
        .is_some_and(|subject| {
            subject.cid_number.as_slice() == cid_number && subject.account_id == *account_id
        })
    }

    #[cfg(feature = "runtime-benchmarks")]
    #[allow(clippy::expect_used)]
    fn benchmark_seed_identity(account_id: &AccountId) -> Vec<u8> {
        // Benchmark 接口必须返回有效 CID；种子未形成双向绑定时立即停止，禁止伪造默认身份。
        <RuntimeCitizenIdentityReader as votingengine::CitizenIdentityReader<AccountId>>::benchmark_seed_identity(
            account_id,
            &citizen_identity::PopulationScope::Country,
        );
        Self::active_cid_number(account_id)
            .expect("square-post benchmark identity must be active and bidirectionally bound")
    }
}

impl square_post::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type CitizenIdentity = RuntimeSquarePostCitizenIdentity;
    type Currency = Balances;
    type TimeProvider = crate::Timestamp;
    type InstitutionAccountQuery = RuntimeInstitutionQuery;
    type InternalVoteEngine = InternalVote;
    type InstitutionRoleAuthorization = RuntimeInstitutionRoleAuthorization;
    type MaxSquarePostIdLen = ConstU32<64>;
    type MaxSquareCidNumberLen = ConstU32<32>;
    type MaxSquareStorageReceiptIdLen = ConstU32<96>;
    type MaxSubscriptionRenewalsPerBlock = ConstU32<50_000>;
    type WeightInfo = square_post::weights::SubstrateWeight<Runtime>;
}

impl citizen_issuance::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type WeightInfo = citizen_issuance::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    /// 决议发行治理参数（统一来源于 primitives 常量）。
    pub const ResolutionIssuanceMaxReasonLen: u32 = primitives::count_const::RESOLUTION_ISSUANCE_MAX_REASON_LEN;
    pub const ResolutionIssuanceMaxAllocations: u32 = primitives::count_const::RESOLUTION_ISSUANCE_MAX_ALLOCATIONS;
    pub const ResolutionIssuanceMaxTotalIssuance: u128 = u128::MAX;
    pub const ResolutionIssuanceMaxSingleIssuance: u128 = 14_434_973_780_000;
    /// Runtime 升级治理提案备注最大长度。
    pub const RuntimeUpgradeMaxReasonLen: u32 = 1024;
    /// Runtime wasm 最大长度（字节）。
    pub const RuntimeUpgradeMaxCodeSize: u32 = 5 * 1024 * 1024;
    /// 管理员治理：单个注册机构账户管理员上限。
    ///
    /// 物理 BoundedVec 上限必须覆盖机构账户 1989 人场景；个人账户
    /// 另由 MaxPersonalAccountAdmins 限制为 64。
    pub const MaxAdminsPerInstitution: u32 = 1989;
    /// 管理员治理：单个个人账户管理员上限。
    pub const MaxPersonalAccountAdmins: u32 = 64;
    /// GRANDPA authority set 变更生效延迟（单位：区块）。
    /// 取非 0，给运维注入新 gran 私钥预留窗口，避免立即切换导致短时失票。
    pub const GrandpaAuthoritySetChangeDelay: u32 = 30;
}

impl public_admins::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type MaxAdminsPerInstitution = MaxAdminsPerInstitution;
    type CitizenIdentityBinding = RuntimePublicAdminCitizenIdentityBinding;
    type ChainPhase = GenesisPallet;
}

impl private_admins::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type MaxAdminsPerInstitution = MaxAdminsPerInstitution;
    type ChainPhase = GenesisPallet;
    type CitizenIdentityBinding = RuntimePublicAdminCitizenIdentityBinding;
    type LegalRepresentativeQuery = RuntimeInstitutionLegalRepresentativeQuery;
}

impl resolution_destroy::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type InternalVoteEngine = InternalVote;
    type InstitutionRoleAuthorization = RuntimeInstitutionRoleAuthorization;
    type InstitutionQuery = RuntimeInstitutionQuery;
    type OnchainFeeCharger =
        onchain::OnchainExecutionFeeCharger<Runtime, Balances, OnchainExecutionFeeDistributor>;
    type WeightInfo = resolution_destroy::weights::SubstrateWeight<Runtime>;
}

impl grandpakey_change::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type GrandpaChangeDelay = GrandpaAuthoritySetChangeDelay;
    type InternalVoteEngine = InternalVote;
    type InstitutionRoleAuthorization = RuntimeInstitutionRoleAuthorization;
    type WeightInfo = grandpakey_change::weights::SubstrateWeight<Runtime>;
}

/// 执行期手续费分账适配器：把 `Currency` 产生的 `NegativeImbalance`
/// 转成统一分账器接收的 `Credit`（80% 全节点 / 10% 国家储委会 / 10% 安全基金）。
pub struct OnchainExecutionFeeDistributor;

impl frame_support::traits::OnUnbalanced<pallet_balances::NegativeImbalance<Runtime>>
    for OnchainExecutionFeeDistributor
{
    fn on_nonzero_unbalanced(amount: pallet_balances::NegativeImbalance<Runtime>) {
        use frame_support::traits::fungible::Balanced;
        // 将 NegativeImbalance 等额转换为统一分账器使用的 Credit。
        let value = frame_support::traits::Imbalance::peek(&amount);
        // 消费 NegativeImbalance，让付款账户的余额变化正式生效。
        drop(amount);
        // 用 Balanced trait 从“零”铸造等额 Credit 并交给统一分账器。
        // 注意：drop(NegativeImbalance) 已将资金从流通中移除，
        // issue() 再铸回等额 Credit 让 router 分配，总量不变。
        let credit = <Balances as Balanced<AccountId>>::issue(value);

        type DistributionRouter = onchain::OnchainFeeRouter<
            Runtime,
            Balances,
            PowDigestAuthor,
            RuntimeNrcAccountProvider,
            RuntimeSafetyFundAccountProvider,
        >;
        <DistributionRouter as frame_support::traits::tokens::imbalance::OnUnbalanced<_>>::on_unbalanced(credit);
    }
}

impl multisig::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type InternalVoteEngine = InternalVote;
    type InstitutionRoleAuthorization = RuntimeInstitutionRoleAuthorization;
    type InstitutionAsset = RuntimeInstitutionAsset;
    type ProtectedSourceChecker = RuntimeProtectedSourceChecker;
    type MaxRemarkLen = ConstU32<256>;
    type OnchainFeeCharger =
        onchain::OnchainExecutionFeeCharger<Runtime, Balances, OnchainExecutionFeeDistributor>;
    // 多签 admin 配置查询拆给个人生命周期 pallet 与 runtime 机构聚合查询。
    // 转账治理时 multisig-transfer 通过 union 调用,先问个人侧、再问机构侧。
    type PersonalQuery = personal_manage::Pallet<Runtime>;
    type InstitutionQuery = RuntimeInstitutionQuery;
    type WeightInfo = multisig::weights::SubstrateWeight<Runtime>;
}

/// 机构生命周期聚合查询。
///
/// 下游交易模块只依赖本适配器；runtime 内部按公权、私权顺序查询两个生命周期 pallet。
pub struct RuntimeInstitutionQuery;

impl entity_primitives::InstitutionMultisigQuery<AccountId> for RuntimeInstitutionQuery {
    fn lookup_institution_account(cid_number: &[u8], account_name: &[u8]) -> Option<AccountId> {
        let public =
            public_manage::Pallet::<Runtime>::lookup_institution_account(cid_number, account_name);
        let private =
            private_manage::Pallet::<Runtime>::lookup_institution_account(cid_number, account_name);
        match (public, private) {
            (Some(account), None) | (None, Some(account)) => Some(account),
            _ => None,
        }
    }

    fn account_belongs_to(cid_number: &[u8], addr: &AccountId) -> bool {
        let public = public_manage::Pallet::<Runtime>::account_belongs_to(cid_number, addr);
        let private = private_manage::Pallet::<Runtime>::account_belongs_to(cid_number, addr);
        public ^ private
    }

    fn lookup_cid(addr: &AccountId) -> Option<Vec<u8>> {
        match (
            public_manage::Pallet::<Runtime>::lookup_cid(addr),
            private_manage::Pallet::<Runtime>::lookup_cid(addr),
        ) {
            (Some(cid), None) | (None, Some(cid)) => Some(cid),
            _ => None,
        }
    }

    fn lookup_org(addr: &AccountId) -> Option<votingengine::types::InstitutionCode> {
        match (
            public_manage::Pallet::<Runtime>::lookup_org(addr),
            private_manage::Pallet::<Runtime>::lookup_org(addr),
        ) {
            (Some(code), None) | (None, Some(code)) => Some(code),
            _ => None,
        }
    }

    fn lookup_admin_config(
        addr: &AccountId,
    ) -> Option<primitives::multisig::MultisigConfigSnapshot<AccountId>> {
        match (
            public_manage::Pallet::<Runtime>::lookup_admin_config(addr),
            private_manage::Pallet::<Runtime>::lookup_admin_config(addr),
        ) {
            (Some(config), None) | (None, Some(config)) => Some(config),
            _ => None,
        }
    }

    fn account_exists(addr: &AccountId) -> bool {
        public_manage::Pallet::<Runtime>::account_exists(addr)
            ^ private_manage::Pallet::<Runtime>::account_exists(addr)
    }
}

/// 机构存在性统一按 CID 查询；公权、私权只是存储分区，不形成第二身份。
pub struct RuntimeInstitutionCidQuery;

impl entity_primitives::InstitutionCidQuery<votingengine::types::CidNumber>
    for RuntimeInstitutionCidQuery
{
    fn cid_exists(cid_number: &votingengine::types::CidNumber) -> bool {
        public_manage::Institutions::<Runtime>::contains_key(cid_number)
            || private_manage::Institutions::<Runtime>::contains_key(cid_number)
    }
}

/// 通用机构治理结果路由适配器。
///
/// 已完成自身业务校验的任免/治理模块可用它按机构码选择 entity 模组；
/// `election-vote` 不使用本适配器，选举结果必须先回到创建提案的具体选举业务模块复核。
pub struct RuntimeInstitutionGovernanceResultHandler;

impl entity_primitives::InstitutionGovernanceResultHandler<AccountId>
    for RuntimeInstitutionGovernanceResultHandler
{
    fn apply_institution_governance_result(
        result: entity_primitives::InstitutionGovernanceResult<AccountId>,
    ) -> DispatchResult {
        if admin_primitives::is_public_admin_code(&result.institution_code) {
            return public_manage::Pallet::<Runtime>::apply_institution_governance_result(result);
        }
        if admin_primitives::is_private_admin_code(&result.institution_code) {
            return private_manage::Pallet::<Runtime>::apply_institution_governance_result(result);
        }
        Err(sp_runtime::DispatchError::Other(
            "UnsupportedInstitutionGovernanceResultCode",
        ))
    }
}

/// 机构管理员唯一查询路由：CID 是 key，公权/私权只决定落在哪个 storage pallet。
pub struct RuntimeInstitutionAdminQuery;

impl admin_primitives::InstitutionAdminQuery<AccountId> for RuntimeInstitutionAdminQuery {
    fn institution_admins_exist(
        institution_code: primitives::cid::code::InstitutionCode,
        cid_number: &[u8],
    ) -> bool {
        Self::institution_admins(institution_code, cid_number).is_some()
    }

    fn is_institution_admin(
        institution_code: primitives::cid::code::InstitutionCode,
        cid_number: &[u8],
        who: &AccountId,
    ) -> bool {
        if admin_primitives::is_public_admin_code(&institution_code) {
            return <public_admins::Pallet<Runtime> as admin_primitives::InstitutionAdminQuery<
                AccountId,
            >>::is_institution_admin(institution_code, cid_number, who);
        }
        if admin_primitives::is_private_admin_code(&institution_code) {
            return <private_admins::Pallet<Runtime> as admin_primitives::InstitutionAdminQuery<
                AccountId,
            >>::is_institution_admin(institution_code, cid_number, who);
        }
        if admin_primitives::is_unincorporated_admin_code(&institution_code) {
            return <public_admins::Pallet<Runtime> as admin_primitives::InstitutionAdminQuery<
                AccountId,
            >>::is_institution_admin(institution_code, cid_number, who)
                || <private_admins::Pallet<Runtime> as admin_primitives::InstitutionAdminQuery<
                    AccountId,
                >>::is_institution_admin(institution_code, cid_number, who);
        }
        false
    }

    fn institution_admins(
        institution_code: primitives::cid::code::InstitutionCode,
        cid_number: &[u8],
    ) -> Option<Vec<AccountId>> {
        if admin_primitives::is_public_admin_code(&institution_code) {
            return <public_admins::Pallet<Runtime> as admin_primitives::InstitutionAdminQuery<
                AccountId,
            >>::institution_admins(institution_code, cid_number);
        }
        if admin_primitives::is_private_admin_code(&institution_code) {
            return <private_admins::Pallet<Runtime> as admin_primitives::InstitutionAdminQuery<
                AccountId,
            >>::institution_admins(institution_code, cid_number);
        }
        if admin_primitives::is_unincorporated_admin_code(&institution_code) {
            return <public_admins::Pallet<Runtime> as admin_primitives::InstitutionAdminQuery<
                AccountId,
            >>::institution_admins(institution_code, cid_number)
            .or_else(|| {
                <private_admins::Pallet<Runtime> as admin_primitives::InstitutionAdminQuery<
                    AccountId,
                >>::institution_admins(institution_code, cid_number)
            });
        }
        None
    }

    fn institution_admins_len(
        institution_code: primitives::cid::code::InstitutionCode,
        cid_number: &[u8],
    ) -> Option<u32> {
        Self::institution_admins(institution_code, cid_number).map(|admins| admins.len() as u32)
    }

    fn resolve_admin_account(
        institution_code: primitives::cid::code::InstitutionCode,
        cid_number: &[u8],
        caller: &AccountId,
    ) -> Option<AccountId> {
        if admin_primitives::is_public_admin_code(&institution_code) {
            return <public_admins::Pallet<Runtime> as admin_primitives::InstitutionAdminQuery<
                AccountId,
            >>::resolve_admin_account(institution_code, cid_number, caller);
        }
        if admin_primitives::is_private_admin_code(&institution_code) {
            return <private_admins::Pallet<Runtime> as admin_primitives::InstitutionAdminQuery<
                AccountId,
            >>::resolve_admin_account(institution_code, cid_number, caller);
        }
        if admin_primitives::is_unincorporated_admin_code(&institution_code) {
            return <public_admins::Pallet<Runtime> as admin_primitives::InstitutionAdminQuery<
                AccountId,
            >>::resolve_admin_account(institution_code, cid_number, caller)
            .or_else(|| {
                <private_admins::Pallet<Runtime> as admin_primitives::InstitutionAdminQuery<
                    AccountId,
                >>::resolve_admin_account(institution_code, cid_number, caller)
            });
        }
        None
    }
}

pub struct RuntimeAdminAccountQuery;

impl AdminAccountQuery<AccountId> for RuntimeAdminAccountQuery {
    fn active_admin_account_exists(
        institution_code: primitives::cid::code::InstitutionCode,
        personal_account_id: AccountId,
    ) -> bool {
        if admin_primitives::is_personal_admin_code(&institution_code) {
            return personal_admins::Pallet::<Runtime>::active_admin_account_exists(
                institution_code,
                personal_account_id,
            );
        }
        false
    }

    fn is_active_account_admin(
        institution_code: primitives::cid::code::InstitutionCode,
        personal_account_id: AccountId,
        who: &AccountId,
    ) -> bool {
        if admin_primitives::is_personal_admin_code(&institution_code) {
            return personal_admins::Pallet::<Runtime>::is_active_account_admin(
                institution_code,
                personal_account_id,
                who,
            );
        }
        false
    }

    fn active_account_admins(
        institution_code: primitives::cid::code::InstitutionCode,
        personal_account_id: AccountId,
    ) -> Option<Vec<AccountId>> {
        if admin_primitives::is_personal_admin_code(&institution_code) {
            return personal_admins::Pallet::<Runtime>::active_account_admins(
                institution_code,
                personal_account_id,
            );
        }
        None
    }

    fn active_account_admin_records(
        institution_code: primitives::cid::code::InstitutionCode,
        personal_account_id: AccountId,
    ) -> Option<Vec<admin_primitives::Admin<AccountId>>> {
        if admin_primitives::is_personal_admin_code(&institution_code) {
            return personal_admins::Pallet::<Runtime>::active_account_admin_records(
                institution_code,
                personal_account_id,
            );
        }
        None
    }

    fn active_account_admins_len(
        institution_code: primitives::cid::code::InstitutionCode,
        personal_account_id: AccountId,
    ) -> Option<u32> {
        if admin_primitives::is_personal_admin_code(&institution_code) {
            return personal_admins::Pallet::<Runtime>::active_account_admins_len(
                institution_code,
                personal_account_id,
            );
        }
        None
    }

    fn pending_account_exists_for_snapshot(
        institution_code: primitives::cid::code::InstitutionCode,
        personal_account_id: AccountId,
    ) -> bool {
        Self::pending_account_admins_len_for_snapshot(institution_code, personal_account_id)
            .is_some()
    }

    fn is_pending_account_admin_for_snapshot(
        institution_code: primitives::cid::code::InstitutionCode,
        personal_account_id: AccountId,
        who: &AccountId,
    ) -> bool {
        if admin_primitives::is_personal_admin_code(&institution_code) {
            return personal_admins::Pallet::<Runtime>::is_pending_account_admin_for_snapshot(
                institution_code,
                personal_account_id,
                who,
            );
        }
        false
    }

    fn pending_account_admins_for_snapshot(
        institution_code: primitives::cid::code::InstitutionCode,
        personal_account_id: AccountId,
    ) -> Option<Vec<AccountId>> {
        if admin_primitives::is_personal_admin_code(&institution_code) {
            return personal_admins::Pallet::<Runtime>::pending_account_admins_for_snapshot(
                institution_code,
                personal_account_id,
            );
        }
        None
    }

    fn pending_account_admin_records_for_snapshot(
        institution_code: primitives::cid::code::InstitutionCode,
        personal_account_id: AccountId,
    ) -> Option<Vec<admin_primitives::Admin<AccountId>>> {
        if admin_primitives::is_personal_admin_code(&institution_code) {
            return personal_admins::Pallet::<Runtime>::pending_account_admin_records_for_snapshot(
                institution_code,
                personal_account_id,
            );
        }
        None
    }

    fn pending_account_admins_len_for_snapshot(
        institution_code: primitives::cid::code::InstitutionCode,
        personal_account_id: AccountId,
    ) -> Option<u32> {
        if admin_primitives::is_personal_admin_code(&institution_code) {
            return personal_admins::Pallet::<Runtime>::pending_account_admins_len_for_snapshot(
                institution_code,
                personal_account_id,
            );
        }
        None
    }
}

/// 机构法定代表人聚合查询。公开事实只从 entity 读取，不再经过 admins。
pub struct RuntimeInstitutionLegalRepresentativeQuery;

impl entity_primitives::InstitutionLegalRepresentativeQuery<AccountId>
    for RuntimeInstitutionLegalRepresentativeQuery
{
    fn legal_representative(cid_number: &[u8]) -> Option<AccountId> {
        let institution_code = cid_institution_code(cid_number)?;
        if admin_primitives::is_public_admin_code(&institution_code) {
            return <public_manage::Pallet<Runtime> as entity_primitives::InstitutionLegalRepresentativeQuery<AccountId>>::legal_representative(
                cid_number,
            );
        }
        if admin_primitives::is_private_admin_code(&institution_code) {
            return <private_manage::Pallet<Runtime> as entity_primitives::InstitutionLegalRepresentativeQuery<AccountId>>::legal_representative(
                cid_number,
            );
        }
        if admin_primitives::is_unincorporated_admin_code(&institution_code) {
            return <public_manage::Pallet<Runtime> as entity_primitives::InstitutionLegalRepresentativeQuery<AccountId>>::legal_representative(
                cid_number,
            )
            .or_else(|| {
                <private_manage::Pallet<Runtime> as entity_primitives::InstitutionLegalRepresentativeQuery<AccountId>>::legal_representative(
                    cid_number,
                )
            });
        }
        None
    }

    fn legal_representative_cid(cid_number: &[u8]) -> Option<Vec<u8>> {
        let institution_code = cid_institution_code(cid_number)?;
        if admin_primitives::is_public_admin_code(&institution_code) {
            return <public_manage::Pallet<Runtime> as entity_primitives::InstitutionLegalRepresentativeQuery<AccountId>>::legal_representative_cid(
                cid_number,
            );
        }
        if admin_primitives::is_private_admin_code(&institution_code) {
            return <private_manage::Pallet<Runtime> as entity_primitives::InstitutionLegalRepresentativeQuery<AccountId>>::legal_representative_cid(
                cid_number,
            );
        }
        if admin_primitives::is_unincorporated_admin_code(&institution_code) {
            return <public_manage::Pallet<Runtime> as entity_primitives::InstitutionLegalRepresentativeQuery<AccountId>>::legal_representative_cid(
                cid_number,
            )
            .or_else(|| {
                <private_manage::Pallet<Runtime> as entity_primitives::InstitutionLegalRepresentativeQuery<AccountId>>::legal_representative_cid(
                    cid_number,
                )
            });
        }
        None
    }
}

// 链下交易清算模块配置
/// CID 机构登记表查询实现。
///
/// 委托给 runtime 的公权/私权机构生命周期聚合查询；管理员账户校验统一转给
/// `admins` 集合查询，岗位任职事实仍只读取 entity。
pub struct MultisigCidAccountQuery;

impl offchain::bank_check::CidAccountQuery<AccountId> for MultisigCidAccountQuery {
    fn account_info(addr: &AccountId) -> Option<(Vec<u8>, Vec<u8>)> {
        public_manage::AccountRegisteredCid::<Runtime>::get(addr)
            .map(|info| (info.cid_number.to_vec(), info.account_name.to_vec()))
            .or_else(|| {
                private_manage::AccountRegisteredCid::<Runtime>::get(addr)
                    .map(|info| (info.cid_number.to_vec(), info.account_name.to_vec()))
            })
    }

    fn find_account(cid_number: &[u8], account_name: &[u8]) -> Option<AccountId> {
        let public_id: public_manage::CidNumberOf<Runtime> = cid_number.to_vec().try_into().ok()?;
        let public_name: public_manage::AccountNameOf<Runtime> =
            account_name.to_vec().try_into().ok()?;
        if let Some(info) =
            public_manage::InstitutionAccounts::<Runtime>::get(&public_id, &public_name)
        {
            return Some(info.account_id);
        }

        let private_id: private_manage::CidNumberOf<Runtime> =
            cid_number.to_vec().try_into().ok()?;
        let private_name: private_manage::AccountNameOf<Runtime> =
            account_name.to_vec().try_into().ok()?;
        private_manage::InstitutionAccounts::<Runtime>::get(&private_id, &private_name)
            .map(|info| info.account_id)
    }

    fn account_exists(addr: &AccountId) -> bool {
        RuntimeInstitutionQuery::account_exists(addr)
    }

    fn is_institution_role_authorized(
        cid_number: &[u8],
        role_code: &[u8],
        who: &AccountId,
        action_code: u32,
    ) -> bool {
        RuntimeInstitutionRoleAuthorization::is_authorized(
            who,
            &entity_primitives::RoleSubject {
                cid_number: cid_number.to_vec(),
                role_code: role_code.to_vec(),
            },
            &entity_primitives::BusinessActionId {
                module_tag: entity_primitives::business_action::MODULE_OFFCHAIN.to_vec(),
                action_code,
            },
            RolePermissionOperation::Propose,
        )
    }

    /// 清算行资格由身份注册局 eligible-search 负责筛选。
    /// 链上不保存 subject_property/sub_type/parent_cid_number，这里只确认该地址属于已登记的
    /// CID 机构账户,避免把 CID 内部机构类型字段重复落到链上。
    fn is_clearing_bank_eligible(addr: &AccountId) -> bool {
        RuntimeInstitutionQuery::account_exists(addr)
    }

    /// 判定 `bank` 主账户对应的机构是否
    /// 已声明为清算行节点(链上 `ClearingBankNodes` 存在该 cid_number 记录)。
    fn is_registered_clearing_node(bank: &AccountId) -> bool {
        let Some((cid_number, _account_name)) = Self::account_info(bank) else {
            return false;
        };
        let key: offchain::InstitutionCidNumber = match cid_number.try_into() {
            Ok(b) => b,
            Err(_) => return false,
        };
        offchain::pallet::ClearingBankNodes::<Runtime>::contains_key(&key)
    }
}

impl offchain::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type MaxBatchSize = ConstU32<100_000>;
    type MaxBatchSignatureLength = ConstU32<128>;
    type InstitutionAsset = RuntimeInstitutionAsset;
    type CidAccountQuery = MultisigCidAccountQuery;
    type OnchainFeeCharger =
        onchain::OnchainExecutionFeeCharger<Runtime, Balances, OnchainExecutionFeeDistributor>;
    type WeightInfo = offchain::weights::SubstrateWeight<Runtime>;
}

pub struct EnsureNrcAdmin;

#[cfg(feature = "runtime-benchmarks")]
fn seed_benchmark_public_admin_account(
    cid_number: &'static str,
    institution_code: primitives::cid::code::InstitutionCode,
    raw_admins: &[[u8; 32]],
) -> Result<AccountId, ()> {
    let first_admin = AccountId::new(raw_admins.first().copied().ok_or(())?);
    let admins: public_admins::AdminsOf<Runtime> = raw_admins
        .iter()
        .map(|raw_admin| admin_primitives::Admin {
            account_id: AccountId::new(*raw_admin),
            cid_number: Default::default(),
            family_name: Default::default(),
            given_name: Default::default(),
        })
        .collect::<Vec<_>>()
        .try_into()
        .map_err(|_| ())?;
    let cid_number: admin_primitives::AdminCidNumber =
        cid_number.as_bytes().to_vec().try_into().map_err(|_| ())?;
    public_admins::AdminAccounts::<Runtime>::insert(
        cid_number,
        admin_primitives::InstitutionAdmins {
            institution_code,
            admins,
        },
    );
    Ok(first_admin)
}

#[cfg(feature = "runtime-benchmarks")]
fn seed_benchmark_joint_role(
    cid_number: &'static str,
    role_code: &[u8],
    role_name: &[u8],
    raw_admins: &[[u8; 32]],
) -> Result<(), ()> {
    let cid: public_manage::pallet::CidNumberOf<Runtime> =
        cid_number.as_bytes().to_vec().try_into().map_err(|_| ())?;
    let role_code: public_manage::institution::role::RoleCodeOf =
        role_code.to_vec().try_into().map_err(|_| ())?;
    let role_name: public_manage::pallet::AccountNameOf<Runtime> =
        role_name.to_vec().try_into().map_err(|_| ())?;
    public_manage::InstitutionRoles::<Runtime>::insert(
        &cid,
        &role_code,
        entity_primitives::InstitutionRole {
            cid_number: cid.clone(),
            role_code: role_code.clone(),
            role_name,
            term_required: false,
            role_status: entity_primitives::InstitutionRoleStatus::Active,
        },
    );
    let assignments: public_manage::institution::role::RoleAssignmentsOf<Runtime> = raw_admins
        .iter()
        .map(|raw_admin| entity_primitives::InstitutionAdminAssignment {
            cid_number: cid.clone(),
            account_id: AccountId::new(*raw_admin),
            role_code: role_code.clone(),
            term_start: 0,
            term_end: 0,
            assignment_source: entity_primitives::InstitutionAssignmentSource::Genesis,
            assignment_source_ref: Default::default(),
            assignment_status: entity_primitives::InstitutionAssignmentStatus::Active,
        })
        .collect::<Vec<_>>()
        .try_into()
        .map_err(|_| ())?;
    public_manage::InstitutionRoleAssignments::<Runtime>::insert(&cid, &role_code, assignments);
    public_manage::Pallet::<Runtime>::store_genesis_fixed_role_permissions(&cid, &role_code)
        .map_err(|_| ())
}

#[cfg(feature = "runtime-benchmarks")]
fn seed_benchmark_joint_admins_origin() -> Result<RuntimeOrigin, ()> {
    let nrc = primitives::cid::china::china_cb::CHINA_CB
        .first()
        .ok_or(())?;
    let nrc_cid: public_manage::pallet::CidNumberOf<Runtime> = nrc
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .map_err(|_| ())?;
    if !public_manage::Institutions::<Runtime>::contains_key(&nrc_cid) {
        // benchmark 外部状态默认不执行完整链规创世；联合提案必须使用真实创世岗位、
        // 任职和权限目录，禁止重新伪造一套“管理员即有权”的 benchmark 数据。
        genesis_pallet::institution::build::<Runtime>();
    }
    // spec-genesis 与 benchmark WASM 之间只通过 storage 交接；在岗位目录完成后按当前
    // `admins` SCALE 类型重写管理员集合，避免宿主侧旧编码污染岗位授权读取。
    let admin = seed_benchmark_public_admin_account(
        nrc.cid_number,
        primitives::cid::code::NRC,
        nrc.admins,
    )?;
    seed_benchmark_joint_role(
        nrc.cid_number,
        primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER,
        primitives::governance_skeleton::ROLE_NAME_COMMITTEE_MEMBER,
        nrc.admins,
    )?;
    for entry in primitives::cid::china::china_cb::CHINA_CB.iter().skip(1) {
        seed_benchmark_public_admin_account(
            entry.cid_number,
            primitives::cid::code::PRC,
            entry.admins,
        )?;
        seed_benchmark_joint_role(
            entry.cid_number,
            primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER,
            primitives::governance_skeleton::ROLE_NAME_COMMITTEE_MEMBER,
            entry.admins,
        )?;
    }
    for entry in primitives::cid::china::china_ch::CHINA_CH.iter() {
        seed_benchmark_public_admin_account(
            entry.cid_number,
            primitives::cid::code::PRB,
            entry.admins,
        )?;
        seed_benchmark_joint_role(
            entry.cid_number,
            primitives::governance_skeleton::ROLE_CODE_DIRECTOR,
            primitives::governance_skeleton::ROLE_NAME_DIRECTOR,
            entry.admins,
        )?;
    }
    Ok(RuntimeOrigin::from(frame_system::RawOrigin::Signed(admin)))
}

impl EnsureOrigin<RuntimeOrigin> for EnsureNrcAdmin {
    type Success = AccountId;

    fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
        let who = frame_system::EnsureSigned::<AccountId>::try_origin(o)?;
        if is_nrc_admin(&who) {
            Ok(who)
        } else {
            Err(RuntimeOrigin::from(frame_system::RawOrigin::Signed(who)))
        }
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
        seed_benchmark_joint_admins_origin()
    }
}

pub(crate) fn is_nrc_admin(who: &AccountId) -> bool {
    // NRC 是固定创世机构，清单缺失属于不可恢复的 runtime 配置错误。
    let nrc_cid_number = primitives::cid::china::china_cb::CHINA_CB
        .first()
        .map(|n| n.cid_number.as_bytes())
        .unwrap_or_else(|| panic!("NRC CID must exist"));

    // 创世后只信任链上管理员治理模块中的统一账户表。
    RuntimeInstitutionAdminQuery::is_institution_admin(
        votingengine::types::NRC,
        nrc_cid_number,
        who,
    )
}

/// 联合提案发起权限：国家储委会（CHINA_CB[0]）+ 43个省储委会（CHINA_CB[1..44]）。
pub struct EnsureJointProposer;

impl EnsureOrigin<RuntimeOrigin> for EnsureJointProposer {
    type Success = AccountId;

    fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
        frame_system::EnsureSigned::<AccountId>::try_origin(o)
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
        seed_benchmark_joint_admins_origin()
    }
}

impl resolution_issuance::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type ProposeOrigin = EnsureJointProposer;
    type WeightInfo = resolution_issuance::weights::SubstrateWeight<Runtime>;
    type JointVoteEngine = JointVote;
    type InstitutionRoleAuthorization = public_manage::Pallet<Runtime>;
    type MaxReasonLen = ResolutionIssuanceMaxReasonLen;
    type MaxAllocations = ResolutionIssuanceMaxAllocations;
    type MaxTotalIssuance = ResolutionIssuanceMaxTotalIssuance;
    type MaxSingleIssuance = ResolutionIssuanceMaxSingleIssuance;
}

impl runtime_upgrade::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type ProposeOrigin = EnsureJointProposer;
    type DeveloperUpgradeOrigin = EnsureNrcAdmin;
    type JointVoteEngine = JointVote;
    type InstitutionRoleAuthorization = public_manage::Pallet<Runtime>;
    type RuntimeCodeExecutor = RuntimeSetCodeExecutor;
    type DeveloperUpgradeCheck = GenesisPallet;
    type MaxReasonLen = RuntimeUpgradeMaxReasonLen;
    type MaxRuntimeCodeSize = RuntimeUpgradeMaxCodeSize;
    type WeightInfo = runtime_upgrade::weights::SubstrateWeight<Runtime>;
}

pub struct RuntimeSetCodeExecutor;

impl runtime_upgrade::RuntimeCodeExecutor for RuntimeSetCodeExecutor {
    fn execute_runtime_code(
        code: &[u8],
        pow_params: pow_difficulty::PowDifficultyParams,
        activate_at: u32,
    ) -> DispatchResult {
        #[cfg(feature = "runtime-benchmarks")]
        {
            // benchmark 需要衡量治理编排本身的真实路径，
            // 但不应真的改写 runtime :code 存储，因此这里使用成功的 no-op 执行器。
            if code.is_empty() || pow_params.validate().is_err() || activate_at == 0 {
                Err(sp_runtime::DispatchError::Other("empty runtime code"))
            } else {
                Ok(())
            }
        }

        #[cfg(not(feature = "runtime-benchmarks"))]
        {
            super::PowDifficulty::stage_params(pow_params, activate_at)?;
            let set_code_call = frame_system::Call::<Runtime>::set_code {
                code: code.to_vec(),
            };
            set_code_call
                .dispatch_bypass_filter(frame_system::RawOrigin::Root.into())
                .map(|_| ())
                .map_err(|e| e.error)
        }
    }
}

parameter_types! {
    // 立法院模块边界常量(ADR-027,第1步)
    pub const LegislationMaxTitleLen: u32 = 256;
    pub const LegislationMaxTextLen: u32 = 8192; // 条/款正文(宪法部分条较长)
    pub const LegislationMaxClausesPerArticle: u32 = 50;
    pub const LegislationMaxArticlesPerSection: u32 = 200;
    pub const LegislationMaxSectionsPerChapter: u32 = 50;
    pub const LegislationMaxChaptersPerLaw: u32 = 50;
    pub const LegislationMaxLawsPerScope: u32 = 1000;
    pub const LegislationMaxPendingActivations: u32 = 100;
}

impl legislation_yuan::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    // 立法投票引擎接真实 legislation-vote sub-pallet(ADR-027 第2步),投票端到端流程打通。
    type LegislationVoteEngine = LegislationVote;
    type InstitutionCidQuery = RuntimeInstitutionCidQuery;
    type InstitutionRoleAuthorization = RuntimeInstitutionRoleAuthorization;
    type MaxTitleLen = LegislationMaxTitleLen;
    type MaxTextLen = LegislationMaxTextLen;
    type MaxClausesPerArticle = LegislationMaxClausesPerArticle;
    type MaxArticlesPerSection = LegislationMaxArticlesPerSection;
    type MaxSectionsPerChapter = LegislationMaxSectionsPerChapter;
    type MaxChaptersPerLaw = LegislationMaxChaptersPerLaw;
    type MaxLawsPerScope = LegislationMaxLawsPerScope;
    type MaxPendingActivations = LegislationMaxPendingActivations;
    type WeightInfo = ();
}

pub struct RuntimeJointVoteResultCallback;

impl votingengine::JointVoteResultCallback for RuntimeJointVoteResultCallback {
    fn on_joint_vote_finalized(
        vote_proposal_id: u64,
        approved: bool,
    ) -> Result<votingengine::ProposalExecutionOutcome, sp_runtime::DispatchError> {
        #[cfg(feature = "runtime-benchmarks")]
        {
            let _ = (vote_proposal_id, approved);
            Ok(votingengine::ProposalExecutionOutcome::Ignored)
        }

        #[cfg(not(feature = "runtime-benchmarks"))]
        {
            if resolution_issuance::Pallet::<Runtime>::owns_proposal(vote_proposal_id) {
                return <ResolutionIssuance as votingengine::JointVoteResultCallback>::on_joint_vote_finalized(
                vote_proposal_id,
                approved,
            );
            }

            if runtime_upgrade::Pallet::<Runtime>::owns_proposal(vote_proposal_id) {
                return <RuntimeUpgrade as votingengine::JointVoteResultCallback>::on_joint_vote_finalized(
                    vote_proposal_id,
                    approved,
                );
            }

            Err(sp_runtime::DispatchError::Other(
                "joint vote proposal not found in any module",
            ))
        }
    }
}

impl votingengine::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type MaxVoteNonceLength = ConstU32<64>;
    type MaxVoteSignatureLength = ConstU32<64>;
    type MaxAutoFinalizePerBlock = ConstU32<2_048>;
    type MaxAutoFinalizeWeightPerBlock = votingengine::BlockWeightFraction<Runtime, 4>;
    type MaxExecutionWeightPerBlock = votingengine::BlockWeightFraction<Runtime, 4>;
    type MaxCleanupWeightPerBlock = votingengine::BlockWeightFraction<Runtime, 8>;
    type MaxProposalsPerExpiry = ConstU32<2_048>;
    type MaxInternalProposalMutexBindings = ConstU32<256>;
    type MaxActiveProposals = ConstU32<10>;
    type MaxProposalDataLen = ConstU32<{ 100 * 1024 }>;
    type MaxProposalObjectLen = ConstU32<{ 10 * 1024 * 1024 }>;
    type MaxModuleTagLen = ConstU32<32>;
    type MaxManualExecutionAttempts = ConstU32<3>;
    type ExecutionRetryGraceBlocks = VotingExecutionRetryGraceBlocks;
    type MaxExecutionRetryDeadlinesPerBlock = ConstU32<2_048>;
    type MaxPendingRetryExpirationsPerBlock = ConstU32<256>;
    type MaxCleanupStepsPerBlock = ConstU32<8>;
    type MaxCleanupActivationsPerBlock = ConstU32<64>;
    type CleanupKeysPerStep = ConstU32<256>;
    type CitizenIdentityReader = RuntimeCitizenIdentityReader;
    type JointVoteResultCallback = RuntimeJointVoteResultCallback;
    // 内部投票终态回调注册 6 个顶层槽位；公权/私权机构生命周期共用一个 tuple 槽位，
    // 个人多签生命周期和个人多签管理员共用一个 tuple 槽位。
    // 顺序按调用频率降序:transfer / multisig manage 类业务最频繁,
    // grandpa key 替换最稀有放最后(tuple iterate 时命中越早越省 gas)。
    // 每个 Executor 通过 MODULE_TAG 前缀 + 独立存储键互斥认领本模块提案,
    // 非己方提案直接 Ok(()) skip,顺序不影响行为正确性。
    type InternalVoteResultCallback = (
        multisig::InternalVoteExecutor<Runtime>,
        (
            public_manage::InternalVoteExecutor<Runtime>,
            private_manage::InternalVoteExecutor<Runtime>,
        ),
        (
            personal_manage::InternalVoteExecutor<Runtime>,
            personal_admins::InternalVoteExecutor<Runtime>,
        ),
        resolution_destroy::InternalVoteExecutor<Runtime>,
        grandpakey_change::InternalVoteExecutor<Runtime>,
        square_post::InternalVoteExecutor<Runtime>,
    );
    type InternalAdminProvider = RuntimeInternalAdminProvider;
    type MaxAdminsPerInstitution = MaxAdminsPerInstitution;
    type TimeProvider = pallet_timestamp::Pallet<Runtime>;
    type WeightInfo = votingengine::weights::SubstrateWeight<Runtime>;
    // 四类 timeout / cleanup / mode 终态副作用通过递归 Track tuple 派发。
    type TrackHandlers = (
        InternalVote,
        (JointVote, (LegislationVote, (ElectionVote, ()))),
    );
    // 立法投票(ADR-027):终态业务回调接 legislation-yuan，Track 接 legislation-vote。
    // ProposalOwner 决定由法律、任免或预算业务认领；B1 先装配法律业务壳。
    type LegislationVoteResultCallback = (LegislationYuan,);
    type ElectionVoteResultCallback = ElectionVote;
}

// Sub-pallet Config 注入。共用基础设施 votingengine::Config 已 impl 完;
// sub-pallet 各自 Config 需 RuntimeEvent + 自家 WeightInfo。
impl internal_vote::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type InstitutionRoleProvider = RuntimeInstitutionRoleProvider;
    type WeightInfo = internal_vote::weights::SubstrateWeight<Runtime>;
}

impl joint_vote::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type InstitutionRoleProvider = RuntimeInstitutionRoleProvider;
    type WeightInfo = joint_vote::weights::SubstrateWeight<Runtime>;
}

impl election_vote::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type MaxElectionCandidates = ConstU32<256>;
    // 互选选民来自 VotePlan 指定岗位的有效任职快照，不从 admins 推导。
    type InstitutionRoleProvider = RuntimeInstitutionRoleProvider;
    type WeightInfo = election_vote::weights::SubstrateWeight<Runtime>;
}

impl legislation_vote::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type InstitutionRoleProvider = RuntimeInstitutionRoleProvider;
    type WeightInfo = legislation_vote::weights::SubstrateWeight<Runtime>;
}

impl pow_difficulty::Config for Runtime {
    type WeightInfo = pow_difficulty::weights::SubstrateWeight<Runtime>;
}

frame_support::parameter_types! {
    pub const MaxDeclarationLen: u32 = 2048;
}

/// 创世机构 seeding 注入实现:runtime 侧调用 institution::build。
/// Runtime 本就实现 public_manage/public_admins::Config,天然满足 build 的治理 where 约束,
/// 因此治理耦合留在 runtime 层,不再作为 genesis pallet Config 的 supertrait。
pub struct RuntimeGenesisSeeder;
impl genesis_pallet::GenesisInstitutionSeeder for RuntimeGenesisSeeder {
    fn seed() {
        genesis_pallet::institution::build::<Runtime>();
    }
}

impl genesis_pallet::Config for Runtime {
    type WeightInfo = genesis_pallet::weights::SubstrateWeight<Runtime>;
    type MaxDeclarationLen = MaxDeclarationLen;
    type InstitutionSeeder = RuntimeGenesisSeeder;
}

pub struct RuntimeInternalAdminProvider;

impl votingengine::InternalAdminProvider<AccountId> for RuntimeInternalAdminProvider {
    fn is_institution_admin(
        institution_code: votingengine::types::InstitutionCode,
        cid_number: &[u8],
        who: &AccountId,
    ) -> bool {
        // 名册成员按 account_id（枚举/快照构建语义）。投票人身份解析走下面的 resolver。
        RuntimeInstitutionAdminQuery::is_institution_admin(institution_code, cid_number, who)
    }

    fn resolve_institution_voter(cid_number: &[u8], caller: &AccountId) -> Option<AccountId> {
        // 复用 3a 的名册解析器：运行期有 CID 按 citizen-identity 绑定解析（换绑不掉权），
        // 创世期或无 CID 管理员按 account_id；None = 非该机构当前管理员。
        let institution_code = cid_institution_code(cid_number)?;
        RuntimeInstitutionAdminQuery::resolve_admin_account(institution_code, cid_number, caller)
    }

    #[cfg(feature = "runtime-benchmarks")]
    #[allow(clippy::expect_used)]
    fn benchmark_seed_institution_voter(cid_number: &[u8], voter: &AccountId) {
        // Benchmark 接口不返回 Result；无效机构 CID 或越界夹具必须立即停止，不能静默降级。
        let institution_code =
            cid_institution_code(cid_number).expect("benchmark institution CID must be valid");
        assert!(
            admin_primitives::is_public_admin_code(&institution_code),
            "election benchmark currently seeds a public institution"
        );
        let cid_number: admin_primitives::AdminCidNumber = cid_number
            .to_vec()
            .try_into()
            .expect("benchmark institution CID fits admin bounds");
        let admins: public_admins::AdminsOf<Runtime> = Vec::from([admin_primitives::Admin {
            account_id: voter.clone(),
            cid_number: Default::default(),
            family_name: Default::default(),
            given_name: Default::default(),
        }])
        .try_into()
        .expect("single benchmark admin fits bounds");
        public_admins::AdminAccounts::<Runtime>::insert(
            cid_number,
            admin_primitives::InstitutionAdmins {
                institution_code,
                admins,
            },
        );
    }

    fn institution_threshold(
        institution_code: votingengine::types::InstitutionCode,
        cid_number: &[u8],
    ) -> Option<u32> {
        let cid = votingengine::types::CidNumber::try_from(cid_number.to_vec()).ok()?;
        if admin_primitives::is_public_admin_code(&institution_code) {
            return public_manage::InstitutionGovernanceThresholds::<Runtime>::get(cid);
        }
        if admin_primitives::is_private_admin_code(&institution_code) {
            return private_manage::InstitutionGovernanceThresholds::<Runtime>::get(cid);
        }
        None
    }

    fn is_pending_personal_admin(personal_account_id: AccountId, who: &AccountId) -> bool {
        RuntimeAdminAccountQuery::is_pending_account_admin_for_snapshot(
            votingengine::types::PMUL,
            personal_account_id,
            who,
        )
    }

    fn get_pending_personal_admins(
        personal_account_id: AccountId,
    ) -> Option<alloc::vec::Vec<AccountId>> {
        RuntimeAdminAccountQuery::pending_account_admins_for_snapshot(
            votingengine::types::PMUL,
            personal_account_id,
        )
    }

    fn is_personal_admin(personal_account_id: AccountId, who: &AccountId) -> bool {
        RuntimeAdminAccountQuery::is_active_account_admin(
            votingengine::types::PMUL,
            personal_account_id,
            who,
        )
    }

    fn get_personal_admins(personal_account_id: AccountId) -> Option<Vec<AccountId>> {
        RuntimeAdminAccountQuery::active_account_admins(
            votingengine::types::PMUL,
            personal_account_id,
        )
    }

    fn legal_representative(cid_number: &[u8]) -> Option<AccountId> {
        <RuntimeInstitutionLegalRepresentativeQuery as entity_primitives::InstitutionLegalRepresentativeQuery<AccountId>>::legal_representative(
            cid_number,
        )
    }

    fn constitution_guard_members() -> Vec<AccountId> {
        <public_manage::Pallet<Runtime> as InstitutionRoleQuery<AccountId>>::active_accounts_for_role(
            primitives::cid::china::china_sf::CHINA_SF[0]
                .cid_number
                .as_bytes(),
            primitives::governance_skeleton::ROLE_CODE_CONSTITUTION_GUARD,
        )
    }
}

/// 跨业务机构岗位授权路由；CID 必须只存在于公权或私权一个机构目录中。
pub struct RuntimeInstitutionRoleAuthorization;

impl InstitutionRoleAuthorizationQuery<AccountId> for RuntimeInstitutionRoleAuthorization {
    fn role_has_permission(
        role_subject: &entity_primitives::RoleSubject<Vec<u8>, Vec<u8>>,
        business_action_id: &entity_primitives::BusinessActionId<Vec<u8>>,
        operation: RolePermissionOperation,
    ) -> bool {
        match institution_role_directory(role_subject.cid_number.as_slice()) {
            Some(true) => <public_manage::Pallet<Runtime> as InstitutionRoleAuthorizationQuery<
                AccountId,
            >>::role_has_permission(
                role_subject, business_action_id, operation
            ),
            Some(false) => <private_manage::Pallet<Runtime> as InstitutionRoleAuthorizationQuery<
                AccountId,
            >>::role_has_permission(
                role_subject, business_action_id, operation
            ),
            None => false,
        }
    }

    fn is_authorized(
        who: &AccountId,
        role_subject: &entity_primitives::RoleSubject<Vec<u8>, Vec<u8>>,
        business_action_id: &entity_primitives::BusinessActionId<Vec<u8>>,
        operation: RolePermissionOperation,
    ) -> bool {
        match institution_role_directory(role_subject.cid_number.as_slice()) {
            Some(true) => <public_manage::Pallet<Runtime> as InstitutionRoleAuthorizationQuery<
                AccountId,
            >>::is_authorized(
                who, role_subject, business_action_id, operation
            ),
            Some(false) => <private_manage::Pallet<Runtime> as InstitutionRoleAuthorizationQuery<
                AccountId,
            >>::is_authorized(
                who, role_subject, business_action_id, operation
            ),
            None => false,
        }
    }

    fn role_subjects_with_permission(
        cid_number: &[u8],
        business_action_id: &entity_primitives::BusinessActionId<Vec<u8>>,
        operation: RolePermissionOperation,
    ) -> Vec<entity_primitives::RoleSubject<Vec<u8>, Vec<u8>>> {
        match institution_role_directory(cid_number) {
            Some(true) => <public_manage::Pallet<Runtime> as InstitutionRoleAuthorizationQuery<
                AccountId,
            >>::role_subjects_with_permission(
                cid_number, business_action_id, operation
            ),
            Some(false) => <private_manage::Pallet<Runtime> as InstitutionRoleAuthorizationQuery<
                AccountId,
            >>::role_subjects_with_permission(
                cid_number, business_action_id, operation
            ),
            None => Vec::new(),
        }
    }
}

/// 返回 `Some(true)` 表示公权、`Some(false)` 表示私权；不存在或双重登记均拒绝。
fn institution_role_directory(cid_number: &[u8]) -> Option<bool> {
    let public_cid =
        public_manage::pallet::CidNumberOf::<Runtime>::try_from(cid_number.to_vec()).ok();
    let private_cid =
        private_manage::pallet::CidNumberOf::<Runtime>::try_from(cid_number.to_vec()).ok();
    let in_public = public_cid
        .as_ref()
        .is_some_and(public_manage::Institutions::<Runtime>::contains_key);
    let in_private = private_cid
        .as_ref()
        .is_some_and(private_manage::Institutions::<Runtime>::contains_key);
    match (in_public, in_private) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

/// 内部投票与联合投票统一读取公权或私权机构的岗位任职快照。
pub struct RuntimeInstitutionRoleProvider;

impl votingengine::InstitutionRoleProvider<AccountId> for RuntimeInstitutionRoleProvider {
    fn is_active_assignment(cid_number: &[u8], who: &AccountId, role_code: &[u8]) -> bool {
        match institution_role_directory(cid_number) {
            Some(true) => <public_manage::Pallet<Runtime> as InstitutionRoleQuery<
                AccountId,
            >>::is_active_assignment(cid_number, who, role_code),
            Some(false) => <private_manage::Pallet<Runtime> as InstitutionRoleQuery<
                AccountId,
            >>::is_active_assignment(cid_number, who, role_code),
            None => false,
        }
    }

    fn active_accounts_for_role(cid_number: &[u8], role_code: &[u8]) -> Vec<AccountId> {
        match institution_role_directory(cid_number) {
            Some(true) => <public_manage::Pallet<Runtime> as InstitutionRoleQuery<
                AccountId,
            >>::active_accounts_for_role(cid_number, role_code),
            Some(false) => <private_manage::Pallet<Runtime> as InstitutionRoleQuery<
                AccountId,
            >>::active_accounts_for_role(cid_number, role_code),
            None => Vec::new(),
        }
    }
}

pub struct RuntimeCitizenIdentityReader;

impl votingengine::CitizenIdentityReader<AccountId> for RuntimeCitizenIdentityReader {
    fn citizen_subject(who: &AccountId) -> Option<citizen_identity::CitizenSubject<AccountId>> {
        <citizen_identity::Pallet<Runtime> as citizen_identity::CitizenIdentityProvider<
            AccountId,
        >>::citizen_subject(who)
    }

    fn voting_subject(
        who: &AccountId,
        scope: &citizen_identity::PopulationScope,
    ) -> Option<citizen_identity::CitizenSubject<AccountId>> {
        <citizen_identity::Pallet<Runtime> as citizen_identity::CitizenIdentityProvider<
            AccountId,
        >>::voting_subject(who, scope)
    }

    fn candidate_subject(
        who: &AccountId,
        scope: &citizen_identity::PopulationScope,
    ) -> Option<citizen_identity::CitizenSubject<AccountId>> {
        <citizen_identity::Pallet<Runtime> as citizen_identity::CitizenIdentityProvider<
            AccountId,
        >>::candidate_subject(who, scope)
    }

    fn population_data(
        scope: &citizen_identity::PopulationScope,
    ) -> Option<citizen_identity::PopulationData> {
        <citizen_identity::Pallet<Runtime> as citizen_identity::CitizenIdentityProvider<
            AccountId,
        >>::population_data(scope)
    }

    fn voting_subject_at(
        who: &AccountId,
        population_data: &citizen_identity::PopulationData,
    ) -> Option<citizen_identity::CitizenSubject<AccountId>> {
        <citizen_identity::Pallet<Runtime> as citizen_identity::CitizenIdentityProvider<
            AccountId,
        >>::voting_subject_at(who, population_data)
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_seed_identity(who: &AccountId, scope: &citizen_identity::PopulationScope) {
        use citizen_identity::{
            AccountIdByCid, CandidateIdentity, CandidateIdentityByCid, CidByAccountId, CidRecord,
            CidRecordStatus, CitizenStatus, CountryVotingCount, NextEligibilityRevision,
            PopulationReadyDate, VotingEligibilityVersion, VotingEligibilityVersionCount,
            VotingEligibilityVersions, VotingIdentity, VotingIdentityByCid,
        };

        // citizen-identity 按 timestamp 校验护照窗口；benchmark externalities 的
        // 创世时间为 0，先推进到稳定的 2027 年时间点。
        pallet_timestamp::Pallet::<Runtime>::set_timestamp(1_800_000_000_000);
        PopulationReadyDate::<Runtime>::put(citizen_identity::Pallet::<Runtime>::current_date_int());
        let now = frame_system::Pallet::<Runtime>::block_number();
        // 每个 benchmark 账户必须得到不同的公民 CID；固定哈希长度不会超过协议边界。
        let cid_number: citizen_identity::CidNumberBound = sp_io::hashing::blake2_256(who.as_ref())
            .to_vec()
            .try_into()
            .unwrap_or_else(|error| panic!("bounded CID: {error:?}"));
        let identity = VotingIdentity {
            passport_valid_from: 19700101,
            passport_valid_until: 29991231,
            citizen_status: CitizenStatus::Normal,
            residence_province_code: Default::default(),
            residence_city_code: Default::default(),
            residence_town_code: Default::default(),
            updated_at: now,
        };
        let revision = NextEligibilityRevision::<Runtime>::get().saturating_add(1);
        let version_index = VotingEligibilityVersionCount::<Runtime>::get(&cid_number);
        if version_index > 0 {
            VotingEligibilityVersions::<Runtime>::mutate(
                &cid_number,
                version_index.saturating_sub(1),
                |version| {
                    if let Some(version) = version {
                        version.valid_until_revision = Some(revision);
                    }
                },
            );
        }
        VotingEligibilityVersions::<Runtime>::insert(
            &cid_number,
            version_index,
            VotingEligibilityVersion {
                identity: identity.clone(),
                valid_from_revision: revision,
                valid_until_revision: None,
            },
        );
        VotingEligibilityVersionCount::<Runtime>::insert(
            &cid_number,
            version_index.saturating_add(1),
        );
        NextEligibilityRevision::<Runtime>::put(revision);
        VotingIdentityByCid::<Runtime>::insert(&cid_number, identity);
        AccountIdByCid::<Runtime>::insert(&cid_number, who);
        CidByAccountId::<Runtime>::insert(who, &cid_number);
        citizen_identity::CidRegistry::<Runtime>::insert(
            &cid_number,
            CidRecord {
                registrar_cid_number: b"benchmark-registrar"
                    .to_vec()
                    .try_into()
                    .unwrap_or_else(|error| panic!("bounded registrar CID: {error:?}")),
                commitment: [0u8; 32],
                residence_province_code: Default::default(),
                residence_city_code: Default::default(),
                status: CidRecordStatus::Active,
                registered_at: now,
                revoked_at: None,
            },
        );
        CandidateIdentityByCid::<Runtime>::insert(
            &cid_number,
            CandidateIdentity {
                birth_province_code: Default::default(),
                birth_city_code: Default::default(),
                birth_town_code: Default::default(),
                family_name: b"benchmark"
                    .to_vec()
                    .try_into()
                    .unwrap_or_else(|error| panic!("bounded family name: {error:?}")),
                given_name: b"citizen"
                    .to_vec()
                    .try_into()
                    .unwrap_or_else(|error| panic!("bounded given name: {error:?}")),
                citizen_sex: citizen_identity::CitizenSex::Male,
                birth_date: 20000101,
                updated_at: now,
            },
        );
        match scope {
            citizen_identity::PopulationScope::Country => CountryVotingCount::<Runtime>::put(1),
            citizen_identity::PopulationScope::Province(province) => {
                citizen_identity::ProvinceVotingCount::<Runtime>::insert(province, 1)
            }
            citizen_identity::PopulationScope::City(province, city) => {
                citizen_identity::CityVotingCount::<Runtime>::insert((province, city), 1)
            }
            citizen_identity::PopulationScope::Town(province, city, town) => {
                citizen_identity::TownVotingCount::<Runtime>::insert((province, city, town), 1)
            }
        }
    }
}
// pallet_assets 内核接入(ADR-011 第八节)+ OnchainIssuance 外壳配置
//
// pallet_assets 是用户代币的内核 storage / 资产记账实现,
// **所有原生 extrinsic 在 RuntimeCallFilter 中 reject**。
// 业务调用必须经由 OnchainIssuance::propose_* → InternalVote/JointVote callback →
// onchain_issuance 内部以 Root 调用 pallet_assets 的内核 API。
//
// pallet_assets 的 deposit 系列常量统一为 0，仅保留底层资产记账能力。
// 当前 OnchainIssuance 对外调用仍由 RuntimeCallFilter 拒绝；后续业务实装时，
// 费用类型必须进入统一 FeeRoute，实际链上执行费必须复用统一执行收费接口，
// 不得恢复专用创建费、押金收费或其它旁路。

parameter_types! {
    /// 资产 metadata 字符串字段长度上限(name / symbol / description),
    /// 与 onchain_issuance::Config::MaxAssetNameLen 等参数对齐。
    pub const AssetsStringLimit: u32 = 64;
    /// 单批 destroy 时一次清理的账户/审批上限。
    pub const AssetsRemoveItemsLimit: u32 = 1000;
    /// pallet_assets 自身 deposit 系列常量；均设为 0，不承担业务收费职责。
    pub const AssetsDepositZero: Balance = 0;
}

impl pallet_assets::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Balance = Balance;
    type RemoveItemsLimit = AssetsRemoveItemsLimit;
    type AssetId = u32;
    type AssetIdParameter = codec::Compact<u32>;
    type Currency = Balances;
    /// 外部 extrinsic 全部被 RuntimeCallFilter reject,这里 origin 设啥不影响实际入口。
    /// CreateOrigin 接 EnsureSigned 仅为满足 trait Success=AccountId 约束;
    /// ForceOrigin 接 EnsureRoot(Success=())。OnchainIssuance 内部经 fungibles trait
    /// (Create / Mutate)直接调内核 API,不走 extrinsic origin 路径。
    type CreateOrigin =
        frame_support::traits::AsEnsureOriginWithArg<frame_system::EnsureSigned<AccountId>>;
    type ForceOrigin = frame_system::EnsureRoot<AccountId>;
    type AssetDeposit = AssetsDepositZero;
    type AssetAccountDeposit = AssetsDepositZero;
    type MetadataDepositBase = AssetsDepositZero;
    type MetadataDepositPerByte = AssetsDepositZero;
    type ApprovalDeposit = AssetsDepositZero;
    type StringLimit = AssetsStringLimit;
    type Freezer = ();
    type Holder = ();
    type Extra = ();
    type CallbackHandle = ();
    type ReserveData = ();
    type WeightInfo = pallet_assets::weights::SubstrateWeight<Runtime>;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = ();
}

parameter_types! {
    pub const OnchainAssetMaxNameLen: u32 = 64;
    pub const OnchainAssetMaxSymbolLen: u32 = 16;
    pub const OnchainAssetMaxDescriptionLen: u32 = 256;
    pub const OnchainAssetMaxBlacklistWordLen: u32 = 32;
    pub const OnchainAssetMaxBlacklistEntries: u32 = 256;
    pub const OnchainAssetReasonHashLen: u32 = 32;
    pub const OnchainAssetMaxScheduledPerBlock: u32 = 64;
}

impl onchain_issuance::pallet::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    /// pallet_assets 内核类型绑定。onchain_issuance 通过该类型调内核 create / mint_into 等内部 API,
    /// 不走原生 extrinsic(已被 RuntimeCallFilter 拦截)。
    type Assets = Assets;
    type InstitutionQuery = RuntimeInstitutionQuery;
    type InstitutionRoleAuthorization = RuntimeInstitutionRoleAuthorization;
    type MaxAssetNameLen = OnchainAssetMaxNameLen;
    type MaxAssetSymbolLen = OnchainAssetMaxSymbolLen;
    type MaxAssetDescriptionLen = OnchainAssetMaxDescriptionLen;
    type MaxBlacklistWordLen = OnchainAssetMaxBlacklistWordLen;
    type MaxBlacklistEntries = OnchainAssetMaxBlacklistEntries;
    type ReasonHashLen = OnchainAssetReasonHashLen;
    type MaxScheduledPerBlock = OnchainAssetMaxScheduledPerBlock;
    type WeightInfo = onchain_issuance::weights::ZeroWeight;
}
