#![cfg_attr(not(feature = "std"), no_std)]

//! # offchain-transaction · 清算行(L2)扫码支付清算
//!
//! 链上清算实现。提供:
//!
//! - L3 绑定 / 充值 / 提现 / 切换清算行(call_index 30-33)
//! - 清算行批次上链 + settlement 执行(call_index 34)
//! - L2 费率自治(call_index 40/41)
//! - L3 支付意图签名 / nonce 防重 / 偿付自动保护 / 多签账户登记等配套机制
//!
//! 创世即终态布局，`STORAGE_VERSION` 恒为 0，不做 `on_runtime_upgrade` migration。

pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;
pub mod weights;

// 扫码支付清算体系子模块。
pub mod bank_check;
pub mod batch_item;
pub mod deposit;
pub mod fee_config;
pub mod nonce;
pub mod settlement;
pub mod solvency;

#[cfg(test)]
mod tests;

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
    pallet_prelude::*,
    storage::{with_transaction, TransactionOutcome},
    traits::Currency,
    traits::StorageVersion,
    Blake2_128Concat,
};
use frame_system::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_core::sr25519::{Public as Sr25519Public, Signature as Sr25519Signature};
use sp_io::crypto::sr25519_verify;
use sp_runtime::AccountId32;

/// 清算行节点声明信息。
///
/// 一家清算行机构(cid_number)在链上声明其对外服务的全节点身份 + RPC 接入点。
/// 用于:
/// - citizenapp 通过 cid_number 反查清算行节点的 wss URL
/// - citizenapp 校验对端 PeerId 防 DNS 劫持
/// - node 网络面板统计 clearing_nodes 数量
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
pub struct ClearingBankNodeInfo<AccountId, BlockNumber> {
    /// libp2p PeerId 字符串(以 "12D3KooW" 开头,~52 字节)。
    pub peer_id: BoundedVec<u8, sp_core::ConstU32<64>>,
    /// 节点对外可达的 RPC 域名(不含 scheme/port,如 "l2.cmb.com.cn")。
    pub rpc_domain: BoundedVec<u8, sp_core::ConstU32<128>>,
    /// 节点 RPC 端口(通常 9944)。
    pub rpc_port: u16,
    /// 注册时所在区块高度。
    pub registered_at: BlockNumber,
    /// 提交注册的清算行管理员账户(审计用)。
    pub registrar_account_id: AccountId,
}

/// 全仓统一的机构 CID 上限；机构身份字段不得复用 PeerId 的 64 字节上限。
pub type InstitutionCidNumber =
    BoundedVec<u8, sp_core::ConstU32<{ primitives::core_const::CID_NUMBER_MAX_BYTES }>>;
/// libp2p PeerId 字段上限。
pub type ClearingPeerId = BoundedVec<u8, sp_core::ConstU32<64>>;
/// 机构岗位码上限；岗位码与机构 CID、签名账户共同构成清算业务权限。
pub type ActorRoleCode = BoundedVec<u8, sp_core::ConstU32<64>>;

/// 清算行清算 pallet 的存储版本。全新创世即采用终态布局，不承载历史迁移。
const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

/// 清算 pallet 的 L3 与批次管理员签名账户均由 sr25519 `Public` 形成官方
/// `AccountId32`。使用完整32字节转换，禁止从泛型 SCALE 编码截断或猜测公钥。
fn sr25519_public_from_account_id(account_id: &AccountId32) -> Sr25519Public {
    Sr25519Public::from_raw(account_id.clone().into())
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::bank_check::CidAccountQuery;
    use crate::weights::WeightInfo;

    #[pallet::config]
    /// L3 支付签名固定使用 sr25519，其签名账户必须是 Polkadot SDK 官方 `AccountId32`。
    /// 编译期类型等式禁止未来改成其它账户类型后仍把 SCALE 编码静默解释成公钥。
    pub trait Config: frame_system::Config<AccountId = AccountId32> {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Currency: Currency<Self::AccountId>;

        /// 单批次最大 item 数(SCALE 安全上限)。
        #[pallet::constant]
        type MaxBatchSize: Get<u32>;

        /// 清算行多签批次签名最大字节(BoundedVec 上限)。
        #[pallet::constant]
        type MaxBatchSignatureLength: Get<u32>;

        /// 资金白名单 / 制度保留地址保护,由 runtime 接入 `institution-asset`。
        type InstitutionAsset: primitives::institution_asset::InstitutionAsset<Self::AccountId>;

        /// CID 机构登记表查询抽象。runtime 层应委托给实体生命周期聚合查询;
        /// 测试可用 `()` 的默认空实现(一律返回未登记)。
        type CidAccountQuery: crate::bank_check::CidAccountQuery<Self::AccountId>;

        /// 批次上链对累计手续费收一次链上交易费的执行器(与 multisig 同一条 80/10/10 分账路径)。
        type OnchainFeeCharger: primitives::fee_policy::OnchainFeeCharger<
            Self::AccountId,
            <Self::Currency as Currency<Self::AccountId>>::Balance,
        >;

        type WeightInfo: crate::weights::WeightInfo;
    }

    /// 清算行多签批次签名(BoundedVec)。
    pub type BatchSignatureOf<T> = BoundedVec<u8, <T as Config>::MaxBatchSignatureLength>;

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    // ================== Storage(清算行 L2 体系) ==================

    /// L3 用户绑定的清算行 **CID**(机构唯一永久主键)。
    ///
    /// 一个 L3 同时只能绑定一家清算行;切换清算行需先把 `DepositBalance` 清零。
    #[pallet::storage]
    #[pallet::getter(fn user_bank)]
    pub type UserBank<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, crate::InstitutionCidNumber, OptionQuery>;

    /// `(清算行 CID, L3)` → 该 L3 在该清算行的存款余额(分)。
    ///
    /// 权威账本;清算行节点本地 ledger 只是缓存,最终以链上值为准。
    #[pallet::storage]
    #[pallet::getter(fn deposit_balance)]
    pub type DepositBalance<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        crate::InstitutionCidNumber,
        Blake2_128Concat,
        T::AccountId,
        u128,
        ValueQuery,
    >;

    /// 清算行 CID → 该清算行所有 L3 存款的总额(冗余,偿付对账用)。
    ///
    /// 不变式:`BankTotalDeposits[cid] == Σ DepositBalance[cid][*]`。
    /// 偿付能力要求:`Currency::free_balance(cid 派生账户) >= BankTotalDeposits[cid]`。
    #[pallet::storage]
    #[pallet::getter(fn bank_total_deposits)]
    pub type BankTotalDeposits<T: Config> =
        StorageMap<_, Blake2_128Concat, crate::InstitutionCidNumber, u128, ValueQuery>;

    /// L3 的单调递增支付 nonce(防 L3 签名被重放)。settlement 批次 `execute`
    /// 时通过 `nonce::consume_nonce` 校验并更新。
    #[pallet::storage]
    #[pallet::getter(fn l3_payment_nonce)]
    pub type L3PaymentNonce<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u64, ValueQuery>;

    /// 清算行当前生效费率(bp)。`settlement::execute_clearing_bank_batch` 按
    /// **收款方清算行** 读此值计算手续费。
    #[pallet::storage]
    #[pallet::getter(fn l2_fee_rate_bp)]
    pub type L2FeeRateBp<T: Config> =
        StorageMap<_, Blake2_128Concat, crate::InstitutionCidNumber, u32, ValueQuery>;

    /// 清算行**待生效**的费率提案。`on_initialize` 到达 `effective_at` 后
    /// 把 `(bank, new_rate_bp)` 搬到 `L2FeeRateBp` 并清除本条。
    #[pallet::storage]
    #[pallet::getter(fn l2_fee_rate_proposed)]
    pub type L2FeeRateProposed<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        crate::InstitutionCidNumber,
        (u32, BlockNumberFor<T>),
        OptionQuery,
    >;

    /// 全局费率上限(bp),由联合投票调整。默认 0 → runtime `fee_config` 兜底
    /// 到 `L2_FEE_RATE_BP_MAX`(10 bp = 0.1%)。
    #[pallet::storage]
    #[pallet::getter(fn max_l2_fee_rate_bp)]
    pub type MaxL2FeeRateBp<T: Config> = StorageValue<_, u32, ValueQuery>;

    /// 已处理链下 tx_id 防重放(按省标识 T2 + tx_id 维度)。
    ///
    /// settlement 写入;清算行节点 event_listener 监听 `PaymentSettled` 时
    /// 以此键为索引。
    #[pallet::storage]
    pub type ProcessedOffchainTx<T: Config> =
        StorageDoubleMap<_, Blake2_128Concat, [u8; 2], Blake2_128Concat, T::Hash, bool, ValueQuery>;

    /// 已处理链下 tx_id 的写入高度(用于过期窗口控制)。
    #[pallet::storage]
    pub type ProcessedOffchainTxAt<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        [u8; 2],
        Blake2_128Concat,
        T::Hash,
        BlockNumberFor<T>,
        OptionQuery,
    >;

    /// 清算行主账户 → 已成功落账的最新批次序号。
    ///
    /// node 侧 packer 启动时读取本值续跑,链上入口要求下一批必须等于
    /// `last + 1`,避免节点重启或恶意重复提交造成批次级重放。
    #[pallet::storage]
    #[pallet::getter(fn last_clearing_batch_seq)]
    pub type LastClearingBatchSeq<T: Config> =
        StorageMap<_, Blake2_128Concat, crate::InstitutionCidNumber, u64, ValueQuery>;

    /// 清算行节点声明 storage。
    ///
    /// `cid_number` → 节点信息(peer_id / rpc_domain / rpc_port / 注册管理员)
    ///
    /// 链上自证"哪家机构在哪个全节点上对外提供清算服务"。
    /// 写入时机:`register_clearing_bank` 单签即可,要求调用方是该机构的激活管理员。
    /// 删除/更新:`unregister_clearing_bank` / `update_clearing_bank_endpoint`。
    #[pallet::storage]
    #[pallet::getter(fn clearing_bank_nodes)]
    pub type ClearingBankNodes<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        crate::InstitutionCidNumber,
        crate::ClearingBankNodeInfo<T::AccountId, BlockNumberFor<T>>,
        OptionQuery,
    >;

    /// 节点 PeerId 反向索引(`peer_id → cid_number`),
    /// 防止同一 PeerId 被多个机构占用。
    #[pallet::storage]
    #[pallet::getter(fn node_peer_to_institution)]
    pub type NodePeerToInstitution<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        crate::ClearingPeerId,
        crate::InstitutionCidNumber,
        OptionQuery,
    >;

    // ================== Events ==================

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// L3 绑定清算行(绑定即开户,无预存;身份主键=CID)。
        BankBound {
            user: T::AccountId,
            bank_cid: crate::InstitutionCidNumber,
        },
        /// L3 充值到清算行(资金落 CID 派生主账户)。
        Deposited {
            user: T::AccountId,
            bank_cid: crate::InstitutionCidNumber,
            amount: u128,
        },
        /// L3 从清算行提现。
        Withdrawn {
            user: T::AccountId,
            bank_cid: crate::InstitutionCidNumber,
            amount: u128,
        },
        /// L3 切换清算行(前置:旧清算行余额已清零)。
        BankSwitched {
            user: T::AccountId,
            old_bank_cid: crate::InstitutionCidNumber,
            new_bank_cid: crate::InstitutionCidNumber,
        },
        /// 清算行管理员提交了费率变更提案,延迟到 `effective_at` 生效。
        L2FeeRateProposed {
            bank_cid: crate::InstitutionCidNumber,
            new_rate_bp: u32,
            effective_at: BlockNumberFor<T>,
        },
        /// 费率提案到期自动激活。
        L2FeeRateActivated {
            bank_cid: crate::InstitutionCidNumber,
            rate_bp: u32,
        },
        /// 全局费率上限更新(联合投票)。
        MaxL2FeeRateUpdated { new_max: u32 },
        /// 单笔扫码支付已在链上最终清算。
        PaymentSettled {
            tx_id: T::Hash,
            payer_account_id: T::AccountId,
            payer_bank_cid: crate::InstitutionCidNumber,
            recipient_account_id: T::AccountId,
            recipient_bank_cid: crate::InstitutionCidNumber,
            transfer_amount: u128,
            fee_amount: u128,
        },
        /// 一次清算行批次落账汇总。
        ClearingBankBatchSettled {
            bank_cid: crate::InstitutionCidNumber,
            submitter: T::AccountId,
            item_count: u32,
            total_debit: u128,
        },
        /// 清算行节点声明完成,机构对外提供清算服务。
        ClearingBankRegistered {
            cid_number: crate::InstitutionCidNumber,
            peer_id: crate::ClearingPeerId,
            rpc_domain: BoundedVec<u8, sp_core::ConstU32<128>>,
            rpc_port: u16,
            registrar_account_id: T::AccountId,
        },
        /// 清算行节点 RPC 端点更新(域名 / 端口变更,PeerId 不变)。
        ClearingBankEndpointUpdated {
            cid_number: crate::InstitutionCidNumber,
            new_domain: BoundedVec<u8, sp_core::ConstU32<128>>,
            new_port: u16,
            updated_by: T::AccountId,
        },
        /// 清算行节点声明注销,机构退出清算网络。
        ClearingBankUnregistered {
            cid_number: crate::InstitutionCidNumber,
            unregistered_by: T::AccountId,
        },
    }

    // ================== Errors ==================

    #[pallet::error]
    pub enum Error<T> {
        /// 单批次金额或手续费字段非法。
        InvalidTransferAmount,
        InvalidFeeAmount,
        /// 付款方 = 收款方。
        SelfTransferNotAllowed,
        /// 单笔金额超 u128::MAX 溢出。
        TransferAmountTooLarge,
        /// 批次为空。
        EmptyBatch,
        /// 批次收款行与 `institution_account_id` 不一致。
        InstitutionMismatch,
        /// 清算行 CID、岗位码、有效任职账户或动作权限校验未通过。
        UnauthorizedAdmin,
        /// 签名已过期(`expires_at` 小于当前高度)。
        ExpiredIntent,
        /// 收款方清算行尚未配置 `L2FeeRateBp`。
        L2FeeRateNotConfigured,
        /// L3 sr25519 签名校验失败。
        InvalidL3Signature,
        /// 新费率越界(`< MIN` 或 `> Max`)。
        InvalidL2FeeRate,
        /// 清算行偿付不足,自动拒绝新扣款。
        SolvencyProtected,
        /// `tx_id` 已在链上被清算,拒绝重复提交。
        TxAlreadyProcessed,
        /// 清算行批次级签名无效。
        InvalidBatchSignature,
        /// 清算行批次序号不等于上一成功序号 + 1。
        InvalidBatchSeq,
        /// 批次 item 内用户声明的清算行与链上 `UserBank[user]` 不一致。
        UserBankMismatch,

        // ========== L3 账户相关 ==========
        /// 目标地址未在链上实体生命周期模块注册为清算行机构。
        NotRegisteredClearingBank,
        /// 目标地址的 `name` 不是 "主账户"(只能绑定主账户,不能绑费用账户)。
        NotMainAccount,
        /// 目标地址的 CID K1 不是 S(私法人)或 F(非法人),不属于私权机构。
        NotPrivateInstitution,
        /// 清算行账户未完整登记在机构账户正反索引中。
        ClearingBankAccountNotFound,
        /// 反查费用账户名称过长(CID name BoundedVec 溢出)。
        FeeAccountNameTooLong,
        /// 清算行未创建配套的 "费用账户",无法清算手续费。
        FeeAccountNotFound,
        /// 清算行 CID 未派生"清算账户"(非 SFGF,或注册未同步创建),L2 资金无落点。
        ClearingAccountNotFound,
        /// L3 当前已绑定其他清算行,需先 switch_bank。
        AlreadyHasBank,
        /// L3 尚未绑定任何清算行。
        NoOpenedBank,
        /// switch_bank 时新旧清算行相同。
        NewBankSameAsCurrent,
        /// 切换清算行前旧清算行余额必须为 0。
        MustClearBalanceFirst,
        /// 充值金额必须大于 0。
        DepositAmountTooSmall,
        /// 提现金额必须大于 0。
        WithdrawAmountTooSmall,
        /// 提现金额超过清算行存款余额。
        InsufficientDepositBalance,
        /// 清算行主账户余额不足以兑现提现(偿付异常,应告警并拒绝)。
        InsufficientBankLiquidity,
        /// institution-asset 拒绝了本笔充值动作。
        DepositForbidden,
        /// institution-asset 拒绝了本笔提现动作。
        WithdrawForbidden,
        /// institution-asset 拒绝从清算账户扣款(非扫码清算/提现的动作,如管理员多签转账)。
        ClearingDebitForbidden,
        /// L3 nonce 自增溢出(极小概率,仅防御性错误)。
        L3NonceOverflow,
        /// L3 提交的 nonce 不等于 `链上 nonce + 1`(重放或不同步)。
        InvalidL3Nonce,

        // ========== 清算行节点声明相关 ==========
        /// 清算行节点动作的 actor_cid_number 字段不能为空。
        EmptyActorCidNumber,
        /// PeerId 字段不能为空。
        EmptyPeerId,
        /// PeerId 格式非法(必须 "12D3KooW" 开头 + 长度 ≥ 46 + 纯 ASCII alphanumeric)。
        InvalidPeerIdFormat,
        /// RPC 域名字段不能为空。
        EmptyRpcDomain,
        /// RPC 域名格式非法(仅允许小写字母 / 数字 / 点 / 横杠)。
        InvalidRpcDomainFormat,
        /// RPC 端口非法(必须 1024-65535)。
        InvalidRpcPort,
        /// 该机构(cid_number)不满足清算行资格白名单。
        NotEligibleForClearingBank,
        /// 该 cid_number 已经声明了清算行节点(切换走 unregister + register)。
        ClearingBankAlreadyRegistered,
        /// 该 cid_number 尚未声明清算行节点(无法 update / unregister)。
        ClearingBankNodeNotFound,
        /// PeerId 已被另一家机构占用(防 PeerId 冒名)。
        PeerIdAlreadyRegistered,
        /// bank_check:该机构未声明清算行节点(尚未加入清算网络)。
        ClearingBankNotRegisteredAsNode,
        /// 批次累计手续费的链上交易费从费用账户扣款失败(余额不足),整批拒绝(fail-closed)。
        ClearingBatchOnchainFeeUnpaid,
    }

    // ================== Calls ==================

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// L3 绑定清算行 = 开户。无预存、无业务开户费。
        ///
        /// 约束:
        /// - 未绑定其他清算行
        /// - `bank_cid` 必须是 K1=S/F 私权机构 + 资格 + 已声明清算行节点
        #[pallet::call_index(30)]
        #[pallet::weight(T::WeightInfo::bind_clearing_bank())]
        pub fn bind_clearing_bank(
            origin: OriginFor<T>,
            bank_cid: crate::InstitutionCidNumber,
        ) -> DispatchResult {
            let user = ensure_signed(origin)?;
            crate::deposit::do_bind_clearing_bank::<T>(user, bank_cid)
        }

        /// L3 从自持链上账户充值到绑定的清算行主账户。`amount` 单位分。
        #[pallet::call_index(31)]
        #[pallet::weight(T::WeightInfo::deposit())]
        pub fn deposit(origin: OriginFor<T>, amount: u128) -> DispatchResult {
            let user = ensure_signed(origin)?;
            crate::deposit::do_deposit::<T>(user, amount)
        }

        /// L3 从清算行主账户提现到自持链上账户。
        #[pallet::call_index(32)]
        #[pallet::weight(T::WeightInfo::withdraw())]
        pub fn withdraw(origin: OriginFor<T>, amount: u128) -> DispatchResult {
            let user = ensure_signed(origin)?;
            crate::deposit::do_withdraw::<T>(user, amount)
        }

        /// L3 切换清算行。前置:当前清算行余额必须为 0。
        #[pallet::call_index(33)]
        #[pallet::weight(T::WeightInfo::switch_bank())]
        pub fn switch_bank(
            origin: OriginFor<T>,
            new_bank_cid: crate::InstitutionCidNumber,
        ) -> DispatchResult {
            let user = ensure_signed(origin)?;
            crate::deposit::do_switch_bank::<T>(user, new_bank_cid)
        }

        /// 清算行批次上链(清算行 L2 体系唯一上链路径)。
        ///
        /// **收款方主导清算**模型。
        /// - `actor_cid_number` = 收款方清算行的唯一机构主键
        /// - `institution_account_id` = **收款方清算行主账户**
        /// - 提交者 = `actor_cid_number + actor_role_code` 的有效岗位任职账户
        /// - 批次内所有 item 的 `recipient_bank_cid` 必须等于 `actor_cid_number`
        ///   (`payer_bank_cid` 可不同,即同一收款方清算行可一次代收来自不同付款方清算行的多笔)
        /// - 本调用属于链下清算费类别，不另收链上 gas；每个 item 的付款公民
        ///   从其 L2 存款支付 `fee_amount`，手续费进入收款方清算行费用账户
        ///
        /// 安全模型:链上验签的核心是 L3 用户对 PaymentIntent 的 sr25519 签名,
        /// PaymentIntent 内含 payer_bank_cid 字段;链上凭 L3 签名授权 mutate
        /// 付款方清算账户 Currency,与谁提交批次无关。
        ///
        /// [`actor_cid_number`] 批次归属的机构 CID
        /// [`actor_role_code`] 提交清算批次的机构岗位码
        /// [`institution_account_id`] 批次归属的清算行主账户(= **收款方**清算行)
        /// [`batch_seq`] 清算行内单调递增的批次序号(冗余审计字段)
        /// [`batch`] `OffchainBatchItem` 列表(每条带 L3 sr25519 签名 / nonce / 费率)
        /// [`batch_signature`] 清算行多签批次级签名
        #[pallet::call_index(34)]
        #[pallet::weight(T::WeightInfo::submit_offchain_batch(batch.len() as u32))]
        pub fn submit_offchain_batch(
            origin: OriginFor<T>,
            actor_cid_number: crate::InstitutionCidNumber,
            actor_role_code: crate::ActorRoleCode,
            institution_account_id: T::AccountId,
            batch_seq: u64,
            batch: BoundedVec<
                crate::batch_item::OffchainBatchItem<T::AccountId, BlockNumberFor<T>>,
                T::MaxBatchSize,
            >,
            batch_signature: BatchSignatureOf<T>,
        ) -> DispatchResult {
            let submitter = ensure_signed(origin)?;
            ensure!(!batch.is_empty(), Error::<T>::EmptyBatch);
            crate::bank_check::ensure_institution_account::<T>(
                actor_cid_number.as_slice(),
                &institution_account_id,
                crate::bank_check::ACCOUNT_NAME_MAIN,
            )?;
            ensure!(
                T::CidAccountQuery::is_institution_role_authorized(
                    actor_cid_number.as_slice(),
                    actor_role_code.as_slice(),
                    &submitter,
                    entity_primitives::business_action::ACTION_OFFCHAIN_SUBMIT_BATCH,
                ),
                Error::<T>::UnauthorizedAdmin
            );
            Self::verify_batch_signature(
                &submitter,
                actor_cid_number.as_slice(),
                actor_role_code.as_slice(),
                &institution_account_id,
                batch_seq,
                batch.as_slice(),
                &batch_signature,
            )?;
            ensure!(
                batch_seq == LastClearingBatchSeq::<T>::get(&actor_cid_number).saturating_add(1),
                Error::<T>::InvalidBatchSeq
            );

            with_transaction(|| {
                match crate::settlement::execute_clearing_bank_batch::<T>(
                    &submitter,
                    &actor_cid_number,
                    actor_role_code.as_slice(),
                    &institution_account_id,
                    batch.as_slice(),
                ) {
                    Ok(()) => {
                        LastClearingBatchSeq::<T>::insert(&actor_cid_number, batch_seq);
                        TransactionOutcome::Commit(Ok(()))
                    }
                    Err(e) => TransactionOutcome::Rollback(Err(e)),
                }
            })?;
            Ok(())
        }

        /// 清算行管理员提案新费率,延迟 7 天生效。
        #[pallet::call_index(40)]
        #[pallet::weight(T::WeightInfo::propose_l2_fee_rate())]
        pub fn propose_l2_fee_rate(
            origin: OriginFor<T>,
            actor_cid_number: crate::InstitutionCidNumber,
            actor_role_code: crate::ActorRoleCode,
            institution_account_id: T::AccountId,
            new_rate_bp: u32,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            crate::fee_config::do_propose_l2_fee_rate::<T>(
                who,
                &actor_cid_number,
                actor_role_code.as_slice(),
                institution_account_id,
                new_rate_bp,
            )
        }

        /// 设置全局费率上限(Root Origin,联合投票回调)。
        #[pallet::call_index(41)]
        #[pallet::weight(T::WeightInfo::set_max_l2_fee_rate())]
        pub fn set_max_l2_fee_rate(origin: OriginFor<T>, new_max: u32) -> DispatchResult {
            ensure_root(origin)?;
            crate::fee_config::do_set_max_l2_fee_rate::<T>(new_max)
        }

        /// 声明本节点为某清算行的清算节点。
        ///
        /// 校验链(任一失败立即拒绝):
        /// 1. origin 是签名账户
        /// 2. actor_cid_number / peer_id / rpc_domain 非空,rpc_port ∈ [1024, 65535]
        /// 3. peer_id 格式合法("12D3KooW" 开头 + 长度 ≥ 46 + 纯 ASCII alphanumeric)
        /// 4. rpc_domain 字符集合法(仅小写字母/数字/点/横杠)
        /// 5. actor_cid_number 反查得到已登记的主账户
        /// 6. 调用方同时匹配该机构 CID、岗位码、有效任职和本动作权限
        /// 7. 资格白名单:机构必须 (K1=S ∧ JOINT_STOCK) ∨ (K1=F ∧ parent.K1=S.JOINT_STOCK)
        /// 8. actor_cid_number 未已注册节点(切换走 unregister + register)
        /// 9. peer_id 未被另一机构占用
        ///
        /// 当前 pallet 整体保持禁用；未来启用前必须由业务方案明确指定投票引擎。
        #[pallet::call_index(50)]
        #[pallet::weight(T::WeightInfo::register_clearing_bank())]
        pub fn register_clearing_bank(
            origin: OriginFor<T>,
            actor_cid_number: crate::InstitutionCidNumber,
            actor_role_code: crate::ActorRoleCode,
            peer_id: crate::ClearingPeerId,
            rpc_domain: BoundedVec<u8, sp_core::ConstU32<128>>,
            rpc_port: u16,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_register_clearing_bank(
                who,
                actor_cid_number,
                actor_role_code,
                peer_id,
                rpc_domain,
                rpc_port,
            )
        }

        /// 更新清算行节点的 RPC 端点(域名 / 端口),PeerId 不变。
        ///
        /// 校验:
        /// 1. origin 是签名账户
        /// 2. actor_cid_number 已注册清算行节点
        /// 3. 调用方同时匹配机构 CID、岗位码、有效任职和本动作权限
        /// 4. new_domain / new_port 字段合法
        ///
        /// 不重新校验资格白名单(注册时已校验,后续无需重复)。
        #[pallet::call_index(51)]
        #[pallet::weight(T::WeightInfo::update_clearing_bank_endpoint())]
        pub fn update_clearing_bank_endpoint(
            origin: OriginFor<T>,
            actor_cid_number: crate::InstitutionCidNumber,
            actor_role_code: crate::ActorRoleCode,
            new_domain: BoundedVec<u8, sp_core::ConstU32<128>>,
            new_port: u16,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_update_clearing_bank_endpoint(
                who,
                actor_cid_number,
                actor_role_code,
                new_domain,
                new_port,
            )
        }

        /// 注销清算行节点声明,机构退出清算网络。
        ///
        /// 校验:
        /// 1. origin 是签名账户
        /// 2. actor_cid_number 已注册清算行节点
        /// 3. 调用方同时匹配机构 CID、岗位码、有效任职和本动作权限
        ///
        /// 注销后该机构 cid_number 不再被 citizenapp 显示为可绑定清算行(CID 后端
        /// `app_search_clearing_banks` 过滤会去掉该 cid_number)。
        /// 已绑定到该机构的用户需要主动 switch_bank 切换或继续使用直到迁移完成。
        #[pallet::call_index(52)]
        #[pallet::weight(T::WeightInfo::unregister_clearing_bank())]
        pub fn unregister_clearing_bank(
            origin: OriginFor<T>,
            actor_cid_number: crate::InstitutionCidNumber,
            actor_role_code: crate::ActorRoleCode,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_unregister_clearing_bank(who, actor_cid_number, actor_role_code)
        }
    }

    // ================== Hooks ==================

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        #[cfg(feature = "std")]
        fn integrity_test() {
            assert!(T::MaxBatchSize::get() > 0);
            assert!(T::MaxBatchSignatureLength::get() > 0);
        }

        /// 每块扫描激活到期的 `L2FeeRateProposed` 提案,搬到 `L2FeeRateBp`。
        /// 清算行规模较小时成本低;若达万级,可优化为 cursor/分批。
        fn on_initialize(now: BlockNumberFor<T>) -> Weight {
            crate::fee_config::activate_pending_rates::<T>(now)
        }
    }

    impl<T: Config> Pallet<T> {
        /// 验证清算行管理员对整批 item 的批次级签名。
        ///
        /// L3 签名仍是资金授权的核心,本签名用于约束“哪个清算行管理员提交了
        /// 哪个 institution + role + batch_seq + batch_bytes”,与 `LastClearingBatchSeq`
        /// 一起防止节点重启后的批次级重放。
        fn verify_batch_signature(
            submitter: &T::AccountId,
            actor_cid_number: &[u8],
            actor_role_code: &[u8],
            institution_account_id: &T::AccountId,
            batch_seq: u64,
            batch: &[crate::batch_item::OffchainBatchItem<T::AccountId, BlockNumberFor<T>>],
            batch_signature: &BatchSignatureOf<T>,
        ) -> DispatchResult {
            let sig = Sr25519Signature::try_from(batch_signature.as_slice())
                .map_err(|_| Error::<T>::InvalidBatchSignature)?;
            let public_key = crate::sr25519_public_from_account_id(submitter);
            let batch_bytes = batch.encode();
            let message = crate::batch_item::batch_signing_hash(
                actor_cid_number,
                actor_role_code,
                institution_account_id,
                batch_seq,
                &batch_bytes,
            );
            ensure!(
                sr25519_verify(&sig, &message, &public_key),
                Error::<T>::InvalidBatchSignature
            );
            Ok(())
        }
    }
}

impl<T: pallet::Config> pallet::Pallet<T> {
    /// 反查清算行 CID 对应的费用账户(辅助 ops / off-chain ledger)。
    pub fn fee_account_of(
        bank_cid: &crate::InstitutionCidNumber,
    ) -> Result<T::AccountId, pallet::Error<T>> {
        crate::bank_check::fee_account_of::<T>(bank_cid.as_slice())
    }

    // ============= 清算行节点声明实现 =============

    /// PeerId 字节串校验:必须以 "12D3KooW" 开头 + 长度 ≥ 46 + 纯 ASCII alphanumeric。
    /// 与 [citizenchain/node/src/ui/network/network-overview/mod.rs::normalize_peer_id]
    /// 保持一致的语义。链上仅做字节级格式校验,不解析 libp2p 协议。
    fn validate_peer_id_bytes(peer_id: &[u8]) -> Result<(), pallet::Error<T>> {
        if peer_id.len() < 46 {
            return Err(pallet::Error::<T>::InvalidPeerIdFormat);
        }
        if !peer_id.starts_with(b"12D3KooW") {
            return Err(pallet::Error::<T>::InvalidPeerIdFormat);
        }
        if !peer_id.iter().all(|c| c.is_ascii_alphanumeric()) {
            return Err(pallet::Error::<T>::InvalidPeerIdFormat);
        }
        Ok(())
    }

    /// RPC 域名字节串校验:仅允许小写字母 / 数字 / 点 / 横杠;
    /// 不解析 DNS,真实可达性由桌面节点提交前自测保证。
    fn validate_rpc_domain_bytes(domain: &[u8]) -> Result<(), pallet::Error<T>> {
        if domain.is_empty() {
            return Err(pallet::Error::<T>::EmptyRpcDomain);
        }
        let valid = domain
            .iter()
            .all(|&c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'.' || c == b'-');
        if !valid {
            return Err(pallet::Error::<T>::InvalidRpcDomainFormat);
        }
        Ok(())
    }

    /// 反查 cid_number 对应的清算行主账户(用于校验机构合法性)。
    fn lookup_main_account_by_cid(cid_number: &[u8]) -> Result<T::AccountId, pallet::Error<T>> {
        use crate::bank_check::{CidAccountQuery, ACCOUNT_NAME_MAIN};
        T::CidAccountQuery::find_account(cid_number, ACCOUNT_NAME_MAIN)
            .ok_or(pallet::Error::<T>::NotRegisteredClearingBank)
    }

    /// `register_clearing_bank` 完整业务逻辑(供 extrinsic 调用)。
    pub(crate) fn do_register_clearing_bank(
        who: T::AccountId,
        actor_cid_number: crate::InstitutionCidNumber,
        actor_role_code: crate::ActorRoleCode,
        peer_id: crate::ClearingPeerId,
        rpc_domain: BoundedVec<u8, sp_core::ConstU32<128>>,
        rpc_port: u16,
    ) -> DispatchResult {
        use crate::bank_check::CidAccountQuery;

        // 1-2. 非空 + 端口范围
        ensure!(
            !actor_cid_number.is_empty(),
            pallet::Error::<T>::EmptyActorCidNumber
        );
        ensure!(!peer_id.is_empty(), pallet::Error::<T>::EmptyPeerId);
        ensure!(rpc_port >= 1024, pallet::Error::<T>::InvalidRpcPort);

        // 3. PeerId 格式
        Self::validate_peer_id_bytes(peer_id.as_slice())?;

        // 4. 域名字符集
        Self::validate_rpc_domain_bytes(rpc_domain.as_slice())?;

        // 5. actor_cid_number → 已登记主账户
        let bank_main = Self::lookup_main_account_by_cid(actor_cid_number.as_slice())?;
        ensure!(
            T::CidAccountQuery::account_exists(&bank_main),
            pallet::Error::<T>::ClearingBankAccountNotFound
        );

        // 6. 调用方必须同时匹配该 CID、岗位码、签名账户和业务动作权限。
        ensure!(
            T::CidAccountQuery::is_institution_role_authorized(
                actor_cid_number.as_slice(),
                actor_role_code.as_slice(),
                &who,
                entity_primitives::business_action::ACTION_OFFCHAIN_REGISTER_BANK,
            ),
            pallet::Error::<T>::UnauthorizedAdmin
        );

        // 7. 资格白名单:CID 负责候选资格,链上实现层确认该机构主账户已登记。
        ensure!(
            T::CidAccountQuery::is_clearing_bank_eligible(&bank_main),
            pallet::Error::<T>::NotEligibleForClearingBank
        );

        // 8. cid_number 未已注册
        ensure!(
            !pallet::ClearingBankNodes::<T>::contains_key(&actor_cid_number),
            pallet::Error::<T>::ClearingBankAlreadyRegistered
        );

        // 9. peer_id 未被另一机构占用
        ensure!(
            !pallet::NodePeerToInstitution::<T>::contains_key(&peer_id),
            pallet::Error::<T>::PeerIdAlreadyRegistered
        );

        let now = frame_system::Pallet::<T>::block_number();
        let info = crate::ClearingBankNodeInfo {
            peer_id: peer_id.clone(),
            rpc_domain: rpc_domain.clone(),
            rpc_port,
            registered_at: now,
            registrar_account_id: who.clone(),
        };

        pallet::ClearingBankNodes::<T>::insert(&actor_cid_number, &info);
        pallet::NodePeerToInstitution::<T>::insert(&peer_id, &actor_cid_number);

        Self::deposit_event(pallet::Event::ClearingBankRegistered {
            cid_number: actor_cid_number,
            peer_id,
            rpc_domain,
            rpc_port,
            registrar_account_id: who,
        });
        Ok(())
    }

    /// `update_clearing_bank_endpoint` 完整业务逻辑。
    pub(crate) fn do_update_clearing_bank_endpoint(
        who: T::AccountId,
        actor_cid_number: crate::InstitutionCidNumber,
        actor_role_code: crate::ActorRoleCode,
        new_domain: BoundedVec<u8, sp_core::ConstU32<128>>,
        new_port: u16,
    ) -> DispatchResult {
        use crate::bank_check::CidAccountQuery;

        ensure!(
            !actor_cid_number.is_empty(),
            pallet::Error::<T>::EmptyActorCidNumber
        );
        ensure!(new_port >= 1024, pallet::Error::<T>::InvalidRpcPort);
        Self::validate_rpc_domain_bytes(new_domain.as_slice())?;

        let mut info = pallet::ClearingBankNodes::<T>::get(&actor_cid_number)
            .ok_or(pallet::Error::<T>::ClearingBankNodeNotFound)?;

        ensure!(
            T::CidAccountQuery::is_institution_role_authorized(
                actor_cid_number.as_slice(),
                actor_role_code.as_slice(),
                &who,
                entity_primitives::business_action::ACTION_OFFCHAIN_UPDATE_BANK_ENDPOINT,
            ),
            pallet::Error::<T>::UnauthorizedAdmin
        );

        info.rpc_domain = new_domain.clone();
        info.rpc_port = new_port;
        pallet::ClearingBankNodes::<T>::insert(&actor_cid_number, &info);

        Self::deposit_event(pallet::Event::ClearingBankEndpointUpdated {
            cid_number: actor_cid_number,
            new_domain,
            new_port,
            updated_by: who,
        });
        Ok(())
    }

    /// `unregister_clearing_bank` 完整业务逻辑。
    pub(crate) fn do_unregister_clearing_bank(
        who: T::AccountId,
        actor_cid_number: crate::InstitutionCidNumber,
        actor_role_code: crate::ActorRoleCode,
    ) -> DispatchResult {
        use crate::bank_check::CidAccountQuery;

        ensure!(
            !actor_cid_number.is_empty(),
            pallet::Error::<T>::EmptyActorCidNumber
        );

        let info = pallet::ClearingBankNodes::<T>::get(&actor_cid_number)
            .ok_or(pallet::Error::<T>::ClearingBankNodeNotFound)?;

        ensure!(
            T::CidAccountQuery::is_institution_role_authorized(
                actor_cid_number.as_slice(),
                actor_role_code.as_slice(),
                &who,
                entity_primitives::business_action::ACTION_OFFCHAIN_UNREGISTER_BANK,
            ),
            pallet::Error::<T>::UnauthorizedAdmin
        );

        // 删除主索引 + 反向索引
        pallet::ClearingBankNodes::<T>::remove(&actor_cid_number);
        pallet::NodePeerToInstitution::<T>::remove(&info.peer_id);

        Self::deposit_event(pallet::Event::ClearingBankUnregistered {
            cid_number: actor_cid_number,
            unregistered_by: who,
        });
        Ok(())
    }
}
