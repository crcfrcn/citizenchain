//! 装配主Runtime模块、区块和交易类型及原生版本。

#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "256"]

#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

pub mod apis;
#[cfg(feature = "runtime-benchmarks")]
#[path = "../tests/benchmarks.rs"]
mod benchmarks;
pub mod configs;
mod evm_accounts;
mod migrations;

extern crate alloc;
use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::weights::Weight;
use frame_support::{dispatch::DispatchInfo, pallet_prelude::TransactionSource};
use scale_info::TypeInfo;
use sp_runtime::{
    generic,
    traits::{
        AsSystemOriginSigner, BlakeTwo256, DispatchInfoOf, Dispatchable, PostDispatchInfoOf,
        TransactionExtension, ValidateResult,
    },
    transaction_validity::{InvalidTransaction, TransactionValidityError},
    DispatchResult, MultiAddress,
};
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

pub use frame_system::Call as SystemCall;
pub use pallet_balances::Call as BalancesCall;
pub use pallet_timestamp::Call as TimestampCall;
#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;

/// Opaque types. These are used by the CLI to instantiate machinery that don't need to know
/// the specifics of the runtime. They can then be made to be agnostic over specific formats
/// of data like extrinsics, allowing for them to continue syncing the network through upgrades
/// to even the core data structures.
pub mod opaque {
    use super::*;
    use sp_consensus_grandpa::AuthorityId as GrandpaId;
    use sp_runtime::impl_opaque_keys;
    use sp_runtime::{
        generic,
        traits::{BlakeTwo256, Hash as HashT},
    };

    pub use sp_runtime::OpaqueExtrinsic as UncheckedExtrinsic;

    /// Opaque block header type.
    pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
    /// Opaque block type.
    pub type Block = generic::Block<Header, UncheckedExtrinsic>;
    /// Opaque block identifier type.
    pub type BlockId = generic::BlockId<Block>;
    /// Opaque block hash type.
    pub type Hash = <BlakeTwo256 as HashT>::Output;

    impl_opaque_keys! {
        pub struct SessionKeys {
            pub grandpa: GrandpaId,
        }
    }
}

// To learn more about runtime versioning, see:
// https://docs.substrate.io/main-docs/build/upgrade#runtime-versioning
#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
    spec_name: alloc::borrow::Cow::Borrowed("citizenchain"),
    impl_name: alloc::borrow::Cow::Borrowed("citizenchain"),
    authoring_version: 0,
    // The version of the runtime specification. A full node will not attempt to use its native
    //   runtime in substitute for the on-chain Wasm runtime unless all of `spec_name`,
    //   `spec_version`, and `authoring_version` are the same between Wasm and native.
    // 当前 runtime 采用统一模块命名：
    // genesis /
    // publicentity / privateentity / personal / engingcore /
    // multisig / offchain / onchain /
    // square。
    // square 订阅采用 runtime 真实公历时间戳协议：只执行扣款、最小状态和时间戳比较；
    // 平台三档价由创世播种，平台机构 CID 为创世固定常量，开发期零用户不设迁移。
    // 公民链版本政策：每次正式 Runtime 升级，spec_version 与 transaction_version
    // 分别在此前实际部署值上加1，钱包同步支持该次交易版本。
    // 编辑、测试或重复编译不递增；源码值不能替代升级前核对的实际链上版本。
    spec_version: 0,
    impl_version: 0,
    apis: apis::RUNTIME_API_VERSIONS,
    transaction_version: 0,
    system_version: 0,
};

pub const BLOCK_HASH_COUNT: BlockNumber = primitives::core_const::BLOCK_HASH_COUNT;

// 货币单位统一为“分”体系：1 表示 1 分，100 分 = 1 元。
pub const FEN: Balance = 1;
pub const YUAN: Balance = 100 * FEN;

// UNIT 别名指向 1 元（100 分）。
pub const UNIT: Balance = YUAN;

/// 账户存在最小余额（单位：分），统一采用 primitives 制度常量。
pub const EXISTENTIAL_DEPOSIT: Balance = primitives::core_const::ACCOUNT_EXISTENTIAL_DEPOSIT;

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
    NativeVersion {
        runtime_version: VERSION,
        can_author_with: Default::default(),
    }
}

/// SDK 外层类型参数；原生交易的认证只由 V5 认证扩展执行。
pub type Signature = sp_runtime::MultiSignature;

/// 唯一链上账户标识：sr 公钥、PQ 公钥域隔离哈希或协议派生标识均为 32 字节。
pub type AccountId = sp_runtime::AccountId32;

/// Balance of an account.
pub type Balance = u128;

/// Index of a transaction in the chain.
pub type Nonce = u32;

/// A hash of some data used by the chain.
pub type Hash = sp_core::H256;

/// An index to a block.
pub type BlockNumber = u32;

/// The address format for describing accounts.
pub type TransactionAddress = MultiAddress<AccountId, ()>;

/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;

/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;

/// A Block signed with a Justification
pub type SignedBlock = generic::SignedBlock<Block>;

/// BlockId type as expected by this runtime.
pub type BlockId = generic::BlockId<Block>;

/// The `TransactionExtension` to the basic transaction logic.
pub type TxExtension = (
    frame_system::AuthorizeCall<Runtime>,
    frame_system::CheckNonZeroSender<Runtime>,
    CheckNonStakeSender,
    frame_system::CheckSpecVersion<Runtime>,
    frame_system::CheckTxVersion<Runtime>,
    frame_system::CheckGenesis<Runtime>,
    frame_system::CheckEra<Runtime>,
    frame_system::CheckNonce<Runtime>,
    frame_system::CheckWeight<Runtime>,
    pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
    frame_metadata_hash_extension::CheckMetadataHash<Runtime>,
    // 默认来源标记不增加原生编码字节；嵌套末两项避免超过Rust元组标准trait的12项上限。
    // 执行顺序仍为CheckMetadataHash、SetOrigin、WeightReclaim。
    (
        pallet_revive::evm::tx_extension::SetOrigin<Runtime>,
        frame_system::WeightReclaim<Runtime>,
    ),
);

/// 签名账户必须有原生币余额，质押账户继续禁止直接签名。沿用现有扩展编码。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, TypeInfo, Debug)]
pub struct CheckNonStakeSender;

impl TransactionExtension<RuntimeCall> for CheckNonStakeSender
where
    RuntimeCall: Dispatchable<Info = DispatchInfo>,
    <RuntimeCall as Dispatchable>::RuntimeOrigin: AsSystemOriginSigner<AccountId> + Clone,
{
    const IDENTIFIER: &'static str = "CheckNonStakeSender";
    type Implicit = ();
    type Val = ();
    type Pre = ();

    fn weight(&self, _call: &RuntimeCall) -> Weight {
        <Runtime as frame_system::Config>::DbWeight::get()
            .reads(1)
            .saturating_add(Weight::from_parts(2_000_000, 3_593))
    }

    fn validate(
        &self,
        origin: RuntimeOrigin,
        _call: &RuntimeCall,
        _info: &DispatchInfoOf<RuntimeCall>,
        _len: usize,
        _self_implicit: Self::Implicit,
        _inherited_implication: &impl Encode,
        _source: TransactionSource,
    ) -> ValidateResult<Self::Val, RuntimeCall> {
        if let Some(who) = origin.as_system_origin_signer() {
            if configs::is_stake_account(who) {
                return Err(InvalidTransaction::Call.into());
            }
            if Balances::free_balance(who) == 0 {
                return Err(InvalidTransaction::Payment.into());
            }
        }
        Ok((Default::default(), (), origin))
    }

    fn prepare(
        self,
        _val: Self::Val,
        _origin: &RuntimeOrigin,
        _call: &RuntimeCall,
        _info: &DispatchInfoOf<RuntimeCall>,
        _len: usize,
    ) -> Result<Self::Pre, TransactionValidityError> {
        Ok(())
    }

    fn post_dispatch_details(
        _pre: Self::Pre,
        _info: &DispatchInfo,
        _post_info: &PostDispatchInfoOf<RuntimeCall>,
        _len: usize,
        _result: &DispatchResult,
    ) -> Result<Weight, TransactionValidityError> {
        Ok(Weight::zero())
    }
}

/// 原生交易统一采用 V5；SDK 的 Ethereum 检查与固有交易保持原执行路径。
#[derive(
    codec::Encode,
    codec::Decode,
    codec::DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    Debug,
    sp_runtime::serde::Serialize,
    sp_runtime::serde::Deserialize,
)]
#[serde(crate = "sp_runtime::serde", transparent)]
pub struct UncheckedExtrinsic(
    pub  generic::UncheckedExtrinsic<
        TransactionAddress,
        RuntimeCall,
        Signature,
        TxExtension,
        sp_runtime::traits::PipelineAtVers<1, NativeTxExtension>,
    >,
);

type SdkExtrinsic =
    pallet_revive::evm::runtime::UncheckedExtrinsic<TransactionAddress, Signature, EthExtraImpl>;

impl
    From<
        generic::UncheckedExtrinsic<
            TransactionAddress,
            RuntimeCall,
            Signature,
            TxExtension,
            sp_runtime::traits::PipelineAtVers<1, NativeTxExtension>,
        >,
    > for UncheckedExtrinsic
{
    fn from(
        value: generic::UncheckedExtrinsic<
            TransactionAddress,
            RuntimeCall,
            Signature,
            TxExtension,
            sp_runtime::traits::PipelineAtVers<1, NativeTxExtension>,
        >,
    ) -> Self {
        Self(value)
    }
}
impl From<UncheckedExtrinsic> for sp_runtime::OpaqueExtrinsic {
    fn from(value: UncheckedExtrinsic) -> Self {
        value.0.into()
    }
}
impl TypeInfo for UncheckedExtrinsic {
    type Identity = <SdkExtrinsic as TypeInfo>::Identity;
    fn type_info() -> scale_info::Type {
        SdkExtrinsic::type_info()
    }
}
impl sp_runtime::traits::ExtrinsicMetadata for UncheckedExtrinsic {
    const VERSIONS: &'static [u8] = &[5];
    type TransactionExtensionPipelines =
        <SdkExtrinsic as sp_runtime::traits::ExtrinsicMetadata>::TransactionExtensionPipelines;
}
impl sp_runtime::traits::ExtrinsicLike for UncheckedExtrinsic {
    fn is_bare(&self) -> bool {
        sp_runtime::traits::ExtrinsicLike::is_bare(&self.0)
    }
}
impl sp_runtime::traits::ExtrinsicCall for UncheckedExtrinsic {
    type Call = RuntimeCall;
    fn call(&self) -> &RuntimeCall {
        &self.0.function
    }
    fn into_call(self) -> RuntimeCall {
        self.0.function
    }
}
impl sp_runtime::traits::LazyExtrinsic for UncheckedExtrinsic {
    fn decode_unprefixed(data: &[u8]) -> Result<Self, codec::Error> {
        sp_runtime::traits::LazyExtrinsic::decode_unprefixed(data).map(Self)
    }
}
impl frame_support::dispatch::GetDispatchInfo for UncheckedExtrinsic {
    fn get_dispatch_info(&self) -> frame_support::dispatch::DispatchInfo {
        frame_support::dispatch::GetDispatchInfo::get_dispatch_info(&SdkExtrinsic::from(
            self.0.clone(),
        ))
    }
}
impl frame_support::traits::InherentBuilder for UncheckedExtrinsic {
    fn new_inherent(call: RuntimeCall) -> Self {
        generic::UncheckedExtrinsic::new_bare(call).into()
    }
}
/// FRAME 的交易构造合同同样只生成 V5，认证账户包含在唯一证明中。
impl frame_support::traits::SignedTransactionBuilder for UncheckedExtrinsic {
    type Address = ();
    type Signature = mldsa::Proof;
    type Extension = TxExtension;
    fn new_signed_transaction(
        call: RuntimeCall,
        _: (),
        proof: mldsa::Proof,
        checks: TxExtension,
    ) -> Self {
        generic::UncheckedExtrinsic::from_parts(
            call,
            generic::Preamble::General(sp_runtime::traits::ExtensionVariant::Other(
                sp_runtime::traits::PipelineAtVers::<1, _>::new((
                    mldsa::Authenticate::<Runtime>::from(proof),
                    checks,
                )),
            )),
        )
        .into()
    }
}
impl<Lookup> sp_runtime::traits::Checkable<Lookup> for UncheckedExtrinsic
where
    SdkExtrinsic: sp_runtime::traits::Checkable<Lookup>,
{
    type Checked = <SdkExtrinsic as sp_runtime::traits::Checkable<Lookup>>::Checked;
    fn check(self, lookup: &Lookup) -> Result<Self::Checked, TransactionValidityError> {
        use sp_runtime::generic::Preamble;
        use sp_runtime::traits::ExtensionVariant;
        match &self.0.preamble {
            Preamble::Bare(5) | Preamble::General(ExtensionVariant::Other(_)) => {}
            Preamble::General(ExtensionVariant::V0(_))
                if matches!(
                    &self.0.function,
                    RuntimeCall::Revive(pallet_revive::Call::eth_transact { .. })
                ) => {}
            _ => return Err(InvalidTransaction::BadSigner.into()),
        }
        SdkExtrinsic::from(self.0).check(lookup)
    }
    #[cfg(feature = "try-runtime")]
    fn unchecked_into_checked_i_know_what_i_am_doing(
        self,
        lookup: &Lookup,
    ) -> Result<Self::Checked, TransactionValidityError> {
        self.check(lookup)
    }
}

/// 两种账户共用同一认证管线，业务检查的顺序与参数保持。
pub type NativeTxExtension = (mldsa::Authenticate<Runtime>, TxExtension);

/// Ethereum 验签转换复用全部既有交易扩展，不引入另一套收费规则。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EthExtraImpl;

impl pallet_revive::evm::runtime::EthExtra for EthExtraImpl {
    type Config = Runtime;
    type ExtensionV0 = TxExtension;
    type ExtensionOtherVersions = sp_runtime::traits::PipelineAtVers<1, NativeTxExtension>;

    fn get_eth_extension(nonce: Nonce, tip: Balance) -> TxExtension {
        (
            frame_system::AuthorizeCall::<Runtime>::new(),
            frame_system::CheckNonZeroSender::<Runtime>::new(),
            CheckNonStakeSender,
            frame_system::CheckSpecVersion::<Runtime>::new(),
            frame_system::CheckTxVersion::<Runtime>::new(),
            frame_system::CheckGenesis::<Runtime>::new(),
            frame_system::CheckEra::<Runtime>::from(generic::Era::Immortal),
            frame_system::CheckNonce::<Runtime>::from(nonce),
            frame_system::CheckWeight::<Runtime>::new(),
            // SDK 严格原生模式在调用本函数前拒绝非零 tip。
            pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(tip),
            frame_metadata_hash_extension::CheckMetadataHash::<Runtime>::new(false),
            (
                pallet_revive::evm::tx_extension::SetOrigin::<Runtime>::new_from_eth_transaction(),
                frame_system::WeightReclaim::<Runtime>::new(),
            ),
        )
    }
}

/// Runtime upgrade migrations 集合；元组按执行顺序排列。
pub type Migrations = ();

/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
    Runtime,
    Block,
    frame_system::ChainContext<Runtime>,
    Runtime,
    AllPalletsWithSystem,
    Migrations,
>;

// Create the runtime by composing the FRAME pallets that were previously configured.
#[frame_support::runtime]
mod runtime {
    #[runtime::runtime]
    #[runtime::derive(
        RuntimeCall,
        RuntimeEvent,
        RuntimeError,
        RuntimeOrigin,
        RuntimeFreezeReason,
        RuntimeHoldReason,
        RuntimeSlashReason,
        RuntimeLockId,
        RuntimeTask,
        RuntimeViewFunction
    )]
    pub struct Runtime;

    #[runtime::pallet_index(0)]
    pub type System = frame_system;

    #[runtime::pallet_index(1)]
    pub type Timestamp = pallet_timestamp;

    // 纯 PoW 出块 + GRANDPA 最终性。
    #[runtime::pallet_index(2)]
    pub type Balances = pallet_balances;

    #[runtime::pallet_index(3)]
    pub type TransactionPayment = pallet_transaction_payment;

    #[runtime::pallet_index(14)]
    pub type Grandpa = pallet_grandpa;

    // 链上交易手续费模块：发出 FeePaid 事件，供客户端读取真实手续费
    #[runtime::pallet_index(4)]
    pub type Onchain = onchain::pallet;

    // 省储行利息模块：按年度给固定省储行账户发放质押利息
    #[runtime::pallet_index(5)]
    pub type PrbBankIssue = prbbankissue;

    // 全节点发行模块：出块成功后发放固定铸块奖励
    #[runtime::pallet_index(6)]
    pub type FullnodeIssue = fullnodeissue;

    // 决议发行模块：完整承载提案、联合投票回调、发行执行与维护审计
    #[runtime::pallet_index(8)]
    pub type ResolutionIssue = resolutionissue;

    // 投票引擎核心:Proposals/反向索引/状态机/快照/锁/清理共用基础设施
    #[runtime::pallet_index(9)]
    pub type VotingEngine = engingcore;

    // 内部投票 sub-pallet：个人多签按管理员账户投票；机构按 VotePlan 岗位快照投票。
    #[runtime::pallet_index(20)]
    pub type InternalVote = internalvote;

    // 联合投票 sub-pallet：机构岗位投票和联合公投两个阶段。
    #[runtime::pallet_index(21)]
    pub type JointVote = jointvote;

    // 选举投票 sub-pallet:选举公职人员(普选 + 机构成员互选)
    #[runtime::pallet_index(22)]
    pub type ElectionVote = electionvote;

    // 链上公民身份：登记投票身份、参选身份并提供人口快照读取
    #[runtime::pallet_index(10)]
    pub type Identity = identity;

    // 公民发行：仅负责认证奖励发放
    #[runtime::pallet_index(11)]
    pub type CitizenIssue = citizenissue;

    // 运行时升级治理模块：提案与联合投票通过后触发 set_code。
    #[runtime::pallet_index(12)]
    pub type Upgrade = upgrade;

    // 决议销毁治理模块：本机构内部投票通过后销毁本机构交易地址余额
    #[runtime::pallet_index(13)]
    pub type Destroy = destroy;

    // GRANDPA 密钥治理模块：国家储委会/省储委会内部投票通过后替换 GRANDPA 投票公钥
    #[runtime::pallet_index(15)]
    pub type GrandpaKey = grandpakey;

    // 个人多签模块:创建/关闭及成员治理，用户自定义多签账户(无 CID 归属,creator_account_id+account_name 派生)。
    #[runtime::pallet_index(7)]
    pub type Personal = personal;

    // pallet_index(29) 原个人成员模块已并入7，永久保留空位，不复用。

    // PoW 动态难度调整模块：每 600 块根据实际出块速度自动调整挖矿难度
    #[runtime::pallet_index(16)]
    pub type Difficulty = difficulty;

    // 机构/个人多签账户转账模块：机构交易以 CID 为主体并显式携带 institution_account_id；个人交易以 personal_account_id 为主体。
    #[runtime::pallet_index(17)]
    pub type Multisig = multisig;

    // 创世模块：存储创世期/运行期阶段、出块目标时间、开发者直升开关、创世常量
    #[runtime::pallet_index(18)]
    pub type Genesis = genesis;

    // 链下交易清算模块：省储行即时清算、批量上链、绑定清算行、费率治理
    #[runtime::pallet_index(19)]
    pub type Offchain = offchain::pallet;

    // 链上发行代币(Plain FT, ADR-011):用户(CID 机构 + personal 个人多签账户)发行 GMB 之外的代币。
    // 唯一外壳入口,内核挂 pallet_assets;pallet_assets 原生 extrinsic 由 BaseCallFilter 屏蔽。
    // 当前为空壳(任务卡 A/B 未实装),OnchainIssue 自身 propose_* 也在 RuntimeCallFilter 中 reject。
    #[runtime::pallet_index(23)]
    pub type OnchainIssue = onchainissue;

    // pallet_assets 内核:多资产基础设施,所有原生 extrinsic 在 RuntimeCallFilter 中 reject。
    // 业务调用一律经由 OnchainIssue::propose_* → InternalVote/JointVote callback → 内部 root 调用。
    #[runtime::pallet_index(24)]
    pub type Assets = pallet_assets;

    // 立法院模块:法律结构化上链 + 修法走立法投票(ADR-027)。业务壳,只承载法律数据与提案入口;
    // 表决/计票/公投归属 legislationvote sub-pallet。
    #[runtime::pallet_index(25)]
    pub type Legislation = legislation;

    // 立法投票 sub-pallet:立法机构专属投票(单院/两院/特别案强制公投,ADR-027)。
    // 投票引擎「头等模式」PROPOSAL_KIND_LEGISLATION,共享核心共享基础,只本地存计票账本。
    #[runtime::pallet_index(26)]
    pub type LegislationVote = legislationvote;

    // 公民选举模块：用于公民选举公职人员，当前仅占位。
    #[runtime::pallet_index(32)]
    pub type Election = election;

    // pallet index 27、28 原机构管理员模块已退役。

    // 公权机构生命周期模块：只负责公权机构 CID 登记、创建、关闭。
    #[runtime::pallet_index(30)]
    pub type PublicEntity = publicentity;

    // 私权机构生命周期模块：只负责私权机构 CID 登记、创建、关闭。
    #[runtime::pallet_index(31)]
    pub type PrivateEntity = privateentity;

    // 地址变更上链模块：只保存地址目录版本、单条地址当前哈希和变更事件。
    #[runtime::pallet_index(33)]
    pub type Address = address;

    // 广场动态发布索引模块：只记录 post_id/content_hash/storage_receipt_id 等链上索引。
    #[runtime::pallet_index(34)]
    pub type Square = square;

    #[runtime::pallet_index(36)]
    pub type Mldsa = mldsa;

    // 官方 Ethereum 执行模块。
    #[runtime::pallet_index(35)]
    pub type Revive = pallet_revive;
}

#[cfg(test)]
#[path = "../tests/mod.rs"]
mod tests;
