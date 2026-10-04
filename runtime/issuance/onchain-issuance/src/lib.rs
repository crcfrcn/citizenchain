//! # 链上发行代币模块(onchain-issuance)
//!
//! GMB 链上"发行 GMB 之外的其他人代币"业务 pallet。第一期承载:
//!
//! - **Plain FT(同质化代币,无锚定声明)**:发行人 = CID 注册机构 + personal-admins 个人多签
//! - 当前公开业务调用尚未实现，由 runtime 统一 `Reject`，不得扣费后空成功。
//! - **NRC 强制 monitor**:交易显式携带 NRC `actor_cid_number + actor_role_code`，并由任职管理员账户签名
//! - **业务 InternalVote / 监管 JointVote**:沿用 unified_voting_entry phase 4 铁律,
//!   业务 pallet 不暴露 wrapper extrinsic,前端直调 VotingEngine
//!
//! ## 与 pallet_assets 内核的关系
//!
//! 本模块是 **唯一外壳入口**,内核挂载 `pallet_assets`(Substrate 多资产 pallet),
//! 所有原生 extrinsic 在 runtime `BaseCallFilter` 中 reject。业务调用必须经由
//! `OnchainIssuance::propose_*` → InternalVote/JointVote 通过 → callback 回调 →
//! 内部以 root 调用 `pallet_assets`。
//!
//! ## 协议位
//!
//! 用户代币用 `asset_id` 做资产编号；机构治理只使用 CID，资产账户仅作执行上下文。
//!
//! ## 模块文件
//!
//! - `types.rs`     — 共用数据类型(AssetMeta / AssetClass / AssetState 等)
//! - `proposal.rs`  — ACTION 常量 + 提案体定义
//! - `validation.rs` — 入参校验(decimals 范围 / 发行机构资格 / 黑名单 hit)
//! - `blacklist.rs` — 字符串黑名单 storage + 默认词表
//! - `execution.rs` — 业务路径(issue/mint/burn/close/transfer)桥接 pallet_assets
//! - `monitor.rs`   — NRC 监管 5 动作执行
//! - `weights.rs`   — WeightInfo 占位
//! - `benchmarks.rs` — runtime-benchmarks 占位
//! - `tests/`       — mock runtime + 业务/监管/黑名单测试

#![cfg_attr(not(feature = "std"), no_std)]
// FRAME 宏从既定资产发行 extrinsic 生成多参数分发函数；参数顺序属于链上编码契约。
#![allow(clippy::too_many_arguments)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;
pub mod blacklist;
pub mod execution;
pub mod monitor;
pub mod proposal;
#[cfg(test)]
mod tests;
pub mod types;
pub mod validation;
pub mod weights;

pub use pallet::*;

/// 模块标识前缀,用于在投票引擎 ProposalData 中识别 onchain-issuance 提案。
///
/// 与 ADR-011 第十节固定,跨端识别用户代币业务提案的稳定业务标签。
pub const MODULE_TAG: &[u8] = b"onc-iss";

#[frame_support::pallet]
pub mod pallet {
    use crate::{types::OnchainAssetMeta, weights::WeightInfo};
    use entity_primitives::{
        BusinessActionId, InstitutionMultisigQuery, InstitutionRoleAuthorizationQuery,
        RolePermissionOperation, RoleSubject,
    };
    use frame_support::{pallet_prelude::*, traits::Currency};
    use frame_system::ensure_signed;
    use frame_system::pallet_prelude::{BlockNumberFor, OriginFor};
    use sp_std::vec::Vec;

    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    /// pallet_assets 内核 AssetId(u32)。
    ///
    /// onchain-issuance 对外使用 `asset_id`，它只表示资产编号。
    pub type OnchainAssetId = u32;

    #[pallet::config]
    pub trait Config: frame_system::Config + votingengine::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// GMB 余额类型绑定；未实现业务不得在本 pallet 内另建付款规则。
        type Currency: Currency<Self::AccountId>;

        /// pallet_assets 内核类型绑定。runtime 接线时把 `pallet_assets::Pallet<Runtime>` 接到此处。
        type Assets: frame_support::traits::tokens::fungibles::Create<
                Self::AccountId,
                AssetId = OnchainAssetId,
                Balance = BalanceOf<Self>,
            > + frame_support::traits::tokens::fungibles::Mutate<Self::AccountId>;

        /// 机构账户归属唯一查询；仅用于校验显式 `actor_cid_number + execution_account_id`。
        type InstitutionQuery: entity_primitives::InstitutionMultisigQuery<Self::AccountId>;

        /// 机构业务权限唯一查询；同时校验 CID、岗位码、有效任职账户和动作权限。
        type InstitutionRoleAuthorization: InstitutionRoleAuthorizationQuery<Self::AccountId>;

        /// 资产元数据字符串字段最大长度(name / symbol / description)。
        #[pallet::constant]
        type MaxAssetNameLen: Get<u32>;
        #[pallet::constant]
        type MaxAssetSymbolLen: Get<u32>;
        #[pallet::constant]
        type MaxAssetDescriptionLen: Get<u32>;

        /// 黑名单单词最大长度与黑名单总条目上限。
        #[pallet::constant]
        type MaxBlacklistWordLen: Get<u32>;
        #[pallet::constant]
        type MaxBlacklistEntries: Get<u32>;

        /// 监管动作 reason_hash 字节长度(固定 32B = sha256)。
        #[pallet::constant]
        type ReasonHashLen: Get<u32>;

        /// `ForceCloseSchedule` 单块到期处理上限(防 on_finalize 单块过载)。
        #[pallet::constant]
        type MaxScheduledPerBlock: Get<u32>;

        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    /// asset_id → 资产元数据。
    ///
    /// 用户代币的唯一权威 storage，记录 actor CID、执行账户、class、decimals 和 state。
    #[pallet::storage]
    #[pallet::getter(fn asset_meta)]
    pub type AssetMetas<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        OnchainAssetId,
        OnchainAssetMeta<T::AccountId>,
        OptionQuery,
    >;

    /// 下一个待分配的 AssetId(u32 自增,从 1 开始)。
    ///
    /// 不复用 pallet_assets 自身的 id 池,onchain-issuance 自管单调递增,
    /// 简化迁移与审计。close 后的 asset_id 永久标记为 Closed,不复用。
    #[pallet::storage]
    #[pallet::getter(fn next_asset_id)]
    pub type NextAssetId<T: Config> = StorageValue<_, OnchainAssetId, ValueQuery>;

    /// 字符串黑名单:`name / symbol / description` 写入前过滤。
    ///
    /// GenesisConfig 注入默认词表(法币/锚定/权威/数字货币 4 类),
    /// 后续添词/删词走 RuntimeUpgrade 投票,不可 sudo。
    #[pallet::storage]
    #[pallet::getter(fn blacklist)]
    pub type Blacklist<T: Config> = StorageValue<
        _,
        BoundedVec<BoundedVec<u8, T::MaxBlacklistWordLen>, T::MaxBlacklistEntries>,
        ValueQuery,
    >;

    /// 强制销毁倒计时调度队列:expire_block → 该块到期的 asset_id 列表。
    ///
    /// NRC `monitor_force_close` 入此队列,30 天后由 `on_finalize`
    /// 通过 `ForceCloseSchedule::take(n)` O(1) 取出处理,**避免全表扫描 Assets**。
    #[pallet::storage]
    #[pallet::getter(fn force_close_schedule)]
    pub type ForceCloseSchedule<T: Config> = StorageMap<
        _,
        Twox64Concat,
        BlockNumberFor<T>,
        BoundedVec<OnchainAssetId, T::MaxScheduledPerBlock>,
        ValueQuery,
    >;

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        /// 黑名单初始词表(法币 / 锚定 / 权威 / 数字货币 4 类)。
        pub initial_blacklist: Vec<Vec<u8>>,
        #[doc(hidden)]
        pub _phantom: PhantomData<T>,
    }

    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self {
                initial_blacklist: crate::blacklist::default_blacklist_words(),
                _phantom: PhantomData,
            }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            // 黑名单初始化,超长词或超过条目上限 silently 跳过(应在编译期保证 fixture 正确)。
            let mut bounded: BoundedVec<
                BoundedVec<u8, T::MaxBlacklistWordLen>,
                T::MaxBlacklistEntries,
            > = BoundedVec::new();
            for word in &self.initial_blacklist {
                let bw: BoundedVec<u8, T::MaxBlacklistWordLen> = match word.clone().try_into() {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if bounded.try_push(bw).is_err() {
                    break;
                }
            }
            Blacklist::<T>::put(bounded);
            NextAssetId::<T>::put(1u32);
        }
    }

    #[pallet::event]
    // deposit_event 由 FRAME 宏生成;事件枚举已定稿但尚无 emit 调用点,
    // 待开放任务卡 20260507-onchain-issuance-plain-ft 子任务 C 实装时消费。
    #[allow(dead_code)]
    #[pallet::generate_deposit(pub(crate) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 用户代币创建成功。
        AssetIssued {
            asset_id: OnchainAssetId,
            actor_cid_number: votingengine::types::CidNumber,
            execution_account_id: T::AccountId,
        },
        /// 用户代币增发。
        Minted {
            asset_id: OnchainAssetId,
            to_account_id: T::AccountId,
            amount: BalanceOf<T>,
        },
        /// 用户代币销毁。
        Burned {
            asset_id: OnchainAssetId,
            account_id: T::AccountId,
            amount: BalanceOf<T>,
        },
        /// 用户代币转账。
        Transferred {
            asset_id: OnchainAssetId,
            from_account_id: T::AccountId,
            to_account_id: T::AccountId,
            amount: BalanceOf<T>,
        },
        /// 用户代币关闭(发行方主动)。
        AssetClosed { asset_id: OnchainAssetId },
        /// NRC 监管:冻结特定持仓。
        MonitorFrozen {
            asset_id: OnchainAssetId,
            account_id: T::AccountId,
            reason_hash: [u8; 32],
        },
        /// NRC 监管:解冻。
        MonitorUnfrozen {
            asset_id: OnchainAssetId,
            account_id: T::AccountId,
            reason_hash: [u8; 32],
        },
        /// NRC 监管:强制 burn(扣押)。
        MonitorConfiscated {
            asset_id: OnchainAssetId,
            account_id: T::AccountId,
            amount: BalanceOf<T>,
            reason_hash: [u8; 32],
        },
        /// NRC 监管:强制划转。
        MonitorForceTransferred {
            asset_id: OnchainAssetId,
            from_account_id: T::AccountId,
            to_account_id: T::AccountId,
            amount: BalanceOf<T>,
            reason_hash: [u8; 32],
        },
        /// NRC 监管:整币封禁(已入 ForceCloseSchedule,30 天后销毁)。
        MonitorForceCloseScheduled {
            asset_id: OnchainAssetId,
            expire_block: BlockNumberFor<T>,
            reason_hash: [u8; 32],
        },
        /// NRC 监管:整币封禁到期,持仓销毁完成。
        MonitorForceCloseExecuted { asset_id: OnchainAssetId },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// 发行机构 CID 或资产执行账户不允许。
        InvalidInstitutionContext,
        /// decimals 越界(必须 0..=18)。
        DecimalsOutOfRange,
        /// 字段命中字符串黑名单(法币 / 锚定 / 权威 / 数字货币词)。
        BlacklistedWord,
        /// 资产不存在。
        AssetNotFound,
        /// 资产已关闭/封禁,不可操作。
        AssetClosed,
        /// 提案体解码失败。
        InvalidProposalData,
        /// AssetId 溢出(u32 自增达到上限)。
        AssetIdOverflow,
        /// pallet_assets 内核错误。
        AssetsInternal,
        /// 字符串字段超长。
        FieldTooLong,
        /// 黑名单条目数已达上限。
        BlacklistFull,
        /// 资产 class 暂不支持(第一期仅 Plain)。
        UnsupportedAssetClass,
        /// propose origin 校验未通过(业务 ACTION:proposer_account_id 不在 actor CID 的 admins;
        /// 监管 ACTION:proposer_account_id 不在 NRC admins)。
        ProposeOriginNotAllowed,
        /// metadata 不可修改（ADR-011 第 5.7 节铁律）。
        MetadataImmutable,
        /// 单块强制销毁队列已满。
        ScheduleFull,
        /// 业务尚未实装(ADR-011 任务卡 A/B)。执行入口一律拒绝,
        /// 禁止形成"投票通过但无任何链上副作用"的假成功。
        NotImplemented,
    }

    /// 业务 pallet 暴露 10 个 propose_X extrinsic(call_index 0..=4 业务 / 10..=14 监管)。
    ///
    /// 不暴露 execute/cancel wrapper(走 VotingEngine::retry_passed_proposal 9.4 / cancel_passed_proposal 9.5)。
    /// 框架阶段先统一执行 CID 管理员授权；创建资产还强制校验 execution_account_id 属于同一 CID。
    /// 资产业务执行、协议费用与投票创建仍由后续发行任务卡实装，但不得绕过本授权入口。
    ///
    /// call_index 5..=9 / 15+ 留洞不复用(永久 ABI)。
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        // ---------- 业务 propose_X(InternalVote)----------

        /// 创建用户代币提案占位；runtime 在完整业务与费用执行落地前统一拒绝本调用。
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::issue())]
        pub fn propose_issue(
            origin: OriginFor<T>,
            actor_cid_number: votingengine::types::CidNumber,
            actor_role_code: votingengine::types::RoleCode,
            execution_account_id: T::AccountId,
            class: crate::types::AssetClass,
            name: BoundedVec<u8, T::MaxAssetNameLen>,
            symbol: BoundedVec<u8, T::MaxAssetSymbolLen>,
            description: BoundedVec<u8, T::MaxAssetDescriptionLen>,
            decimals: u8,
            initial_supply: BalanceOf<T>,
        ) -> DispatchResult {
            let account_id = ensure_signed(origin)?;
            Self::ensure_actor_role(
                &account_id,
                &actor_cid_number,
                actor_role_code.as_slice(),
                entity_primitives::business_action::ACTION_ONCHAIN_ASSET_ISSUE,
                false,
            )?;
            ensure!(
                T::InstitutionQuery::lookup_cid(&execution_account_id).as_deref()
                    == Some(actor_cid_number.as_slice()),
                Error::<T>::InvalidInstitutionContext
            );
            // 业务逻辑参数防 unused 警告(框架阶段)
            let _ = (
                actor_cid_number,
                execution_account_id,
                class,
                name,
                symbol,
                description,
                decimals,
                initial_supply,
            );
            // 当前仅建立授权入口；资产发行与投票执行由任务卡 A 统一实装。
            //   1. validation::ensure_institution_context / ensure_decimals_in_range / ensure_class_supported
            //   2. 按业务动作校验 proposer_account_id 对目标 RoleSubject 的 Propose 权限
            //   3. 字段过黑名单
            //   4. 构造绑定业务对象与岗位选民主体的 VotePlan，再调用指定的内部投票引擎
            Ok(())
        }

        /// 增发提案。
        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::mint())]
        pub fn propose_mint(
            origin: OriginFor<T>,
            actor_cid_number: votingengine::types::CidNumber,
            actor_role_code: votingengine::types::RoleCode,
            asset_id: OnchainAssetId,
            to_account_id: T::AccountId,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            let account_id = ensure_signed(origin)?;
            Self::ensure_actor_role(
                &account_id,
                &actor_cid_number,
                actor_role_code.as_slice(),
                entity_primitives::business_action::ACTION_ONCHAIN_ASSET_MINT,
                false,
            )?;
            let _ = (actor_cid_number, asset_id, to_account_id, amount);
            // 当前仅建立授权入口；增发提案与投票执行由任务卡 A 统一实装。
            Ok(())
        }

        /// 销毁提案。
        #[pallet::call_index(2)]
        #[pallet::weight(<T as Config>::WeightInfo::burn())]
        pub fn propose_burn(
            origin: OriginFor<T>,
            actor_cid_number: votingengine::types::CidNumber,
            actor_role_code: votingengine::types::RoleCode,
            asset_id: OnchainAssetId,
            from_account_id: T::AccountId,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            let account_id = ensure_signed(origin)?;
            Self::ensure_actor_role(
                &account_id,
                &actor_cid_number,
                actor_role_code.as_slice(),
                entity_primitives::business_action::ACTION_ONCHAIN_ASSET_BURN,
                false,
            )?;
            let _ = (actor_cid_number, asset_id, from_account_id, amount);
            // 当前仅建立授权入口；销毁提案与投票执行由任务卡 A 统一实装。
            Ok(())
        }

        /// 关闭代币提案(发行方主动)。
        #[pallet::call_index(3)]
        #[pallet::weight(<T as Config>::WeightInfo::close())]
        pub fn propose_close(
            origin: OriginFor<T>,
            actor_cid_number: votingengine::types::CidNumber,
            actor_role_code: votingengine::types::RoleCode,
            asset_id: OnchainAssetId,
        ) -> DispatchResult {
            let account_id = ensure_signed(origin)?;
            Self::ensure_actor_role(
                &account_id,
                &actor_cid_number,
                actor_role_code.as_slice(),
                entity_primitives::business_action::ACTION_ONCHAIN_ASSET_CLOSE,
                false,
            )?;
            let _ = (actor_cid_number, asset_id);
            // 当前仅建立授权入口；关闭提案与投票执行由任务卡 A 统一实装。
            Ok(())
        }

        /// 转账提案。
        #[pallet::call_index(4)]
        #[pallet::weight(<T as Config>::WeightInfo::transfer())]
        pub fn propose_transfer(
            origin: OriginFor<T>,
            actor_cid_number: votingengine::types::CidNumber,
            actor_role_code: votingengine::types::RoleCode,
            asset_id: OnchainAssetId,
            from_account_id: T::AccountId,
            to_account_id: T::AccountId,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            let account_id = ensure_signed(origin)?;
            Self::ensure_actor_role(
                &account_id,
                &actor_cid_number,
                actor_role_code.as_slice(),
                entity_primitives::business_action::ACTION_ONCHAIN_ASSET_TRANSFER,
                false,
            )?;
            let _ = (
                actor_cid_number,
                asset_id,
                from_account_id,
                to_account_id,
                amount,
            );
            // 当前仅建立授权入口；转账提案与投票执行由任务卡 A 统一实装。
            Ok(())
        }

        // ---------- 监管 propose_monitor_X(JointVote)----------

        /// NRC 监管:冻结持仓提案。
        #[pallet::call_index(10)]
        #[pallet::weight(<T as Config>::WeightInfo::monitor_freeze())]
        pub fn propose_monitor_freeze(
            origin: OriginFor<T>,
            actor_cid_number: votingengine::types::CidNumber,
            actor_role_code: votingengine::types::RoleCode,
            asset_id: OnchainAssetId,
            account_id: T::AccountId,
            reason_hash: [u8; 32],
        ) -> DispatchResult {
            let proposer_account_id = ensure_signed(origin)?;
            Self::ensure_actor_role(
                &proposer_account_id,
                &actor_cid_number,
                actor_role_code.as_slice(),
                entity_primitives::business_action::ACTION_MONITOR_FREEZE,
                true,
            )?;
            let _ = (actor_cid_number, asset_id, account_id, reason_hash);
            // 当前仅建立 NRC 授权入口；冻结提案与联合投票执行由任务卡 B 统一实装。
            //   校验 proposer_account_id 对 NRC 委员 RoleSubject 的监管冻结 Propose 权限
            //   构造含 NRC 委员投票主体的固定 VotePlan，再调用指定的联合投票引擎
            Ok(())
        }

        /// NRC 监管:解冻持仓提案。
        #[pallet::call_index(11)]
        #[pallet::weight(<T as Config>::WeightInfo::monitor_unfreeze())]
        pub fn propose_monitor_unfreeze(
            origin: OriginFor<T>,
            actor_cid_number: votingengine::types::CidNumber,
            actor_role_code: votingengine::types::RoleCode,
            asset_id: OnchainAssetId,
            account_id: T::AccountId,
            reason_hash: [u8; 32],
        ) -> DispatchResult {
            let proposer_account_id = ensure_signed(origin)?;
            Self::ensure_actor_role(
                &proposer_account_id,
                &actor_cid_number,
                actor_role_code.as_slice(),
                entity_primitives::business_action::ACTION_MONITOR_UNFREEZE,
                true,
            )?;
            let _ = (actor_cid_number, asset_id, account_id, reason_hash);
            // 当前仅建立 NRC 授权入口；解冻提案与联合投票执行由任务卡 B 统一实装。
            Ok(())
        }

        /// NRC 监管:强制 burn(扣押)提案。
        #[pallet::call_index(12)]
        #[pallet::weight(<T as Config>::WeightInfo::monitor_confiscate())]
        pub fn propose_monitor_confiscate(
            origin: OriginFor<T>,
            actor_cid_number: votingengine::types::CidNumber,
            actor_role_code: votingengine::types::RoleCode,
            asset_id: OnchainAssetId,
            account_id: T::AccountId,
            amount: BalanceOf<T>,
            reason_hash: [u8; 32],
        ) -> DispatchResult {
            let proposer_account_id = ensure_signed(origin)?;
            Self::ensure_actor_role(
                &proposer_account_id,
                &actor_cid_number,
                actor_role_code.as_slice(),
                entity_primitives::business_action::ACTION_MONITOR_CONFISCATE,
                true,
            )?;
            let _ = (actor_cid_number, asset_id, account_id, amount, reason_hash);
            // 当前仅建立 NRC 授权入口；扣押提案与联合投票执行由任务卡 B 统一实装。
            Ok(())
        }

        /// NRC 监管:强制划转(追赃)提案。
        #[pallet::call_index(13)]
        #[pallet::weight(<T as Config>::WeightInfo::monitor_force_transfer())]
        pub fn propose_monitor_force_transfer(
            origin: OriginFor<T>,
            actor_cid_number: votingengine::types::CidNumber,
            actor_role_code: votingengine::types::RoleCode,
            asset_id: OnchainAssetId,
            from_account_id: T::AccountId,
            to_account_id: T::AccountId,
            amount: BalanceOf<T>,
            reason_hash: [u8; 32],
        ) -> DispatchResult {
            let proposer_account_id = ensure_signed(origin)?;
            Self::ensure_actor_role(
                &proposer_account_id,
                &actor_cid_number,
                actor_role_code.as_slice(),
                entity_primitives::business_action::ACTION_MONITOR_FORCE_TRANSFER,
                true,
            )?;
            let _ = (
                actor_cid_number,
                asset_id,
                from_account_id,
                to_account_id,
                amount,
                reason_hash,
            );
            // 当前仅建立 NRC 授权入口；追赃提案与联合投票执行由任务卡 B 统一实装。
            Ok(())
        }

        /// NRC 监管:整币封禁(30 天后销毁)提案。
        #[pallet::call_index(14)]
        #[pallet::weight(<T as Config>::WeightInfo::monitor_force_close())]
        pub fn propose_monitor_force_close(
            origin: OriginFor<T>,
            actor_cid_number: votingengine::types::CidNumber,
            actor_role_code: votingengine::types::RoleCode,
            asset_id: OnchainAssetId,
            reason_hash: [u8; 32],
        ) -> DispatchResult {
            let proposer_account_id = ensure_signed(origin)?;
            Self::ensure_actor_role(
                &proposer_account_id,
                &actor_cid_number,
                actor_role_code.as_slice(),
                entity_primitives::business_action::ACTION_MONITOR_FORCE_CLOSE,
                true,
            )?;
            let _ = (actor_cid_number, asset_id, reason_hash);
            // 当前仅建立 NRC 授权入口；整币封禁提案与联合投票执行由任务卡 B 统一实装。
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        /// 所有机构调用共用授权入口：CID、岗位码和签名账户必须同时匹配动作权限。
        fn ensure_actor_role(
            account_id: &T::AccountId,
            actor_cid_number: &votingengine::types::CidNumber,
            actor_role_code: &[u8],
            action_code: u32,
            nrc_only: bool,
        ) -> DispatchResult {
            let actor_text = core::str::from_utf8(actor_cid_number.as_slice())
                .map_err(|_| Error::<T>::InvalidInstitutionContext)?;
            let institution_code =
                votingengine::types::institution_code_from_cid_number(actor_text)
                    .ok_or(Error::<T>::InvalidInstitutionContext)?;
            ensure!(
                !nrc_only || institution_code == votingengine::types::NRC,
                Error::<T>::InvalidInstitutionContext
            );
            ensure!(
                T::InstitutionRoleAuthorization::is_authorized(
                    account_id,
                    &RoleSubject {
                        cid_number: actor_cid_number.to_vec(),
                        role_code: actor_role_code.to_vec(),
                    },
                    &BusinessActionId {
                        module_tag: crate::MODULE_TAG.to_vec(),
                        action_code,
                    },
                    RolePermissionOperation::Propose,
                ),
                Error::<T>::ProposeOriginNotAllowed
            );
            Ok(())
        }
    }
}
