#![cfg_attr(not(feature = "std"), no_std)]
//! 全节点发行：全节点PoW铸块奖励发行制度说明（不可治理）
//! ============================================================================
//! 一、制度定位
//! ---------------------------------------------------------------------------
//! 1. 本模块 `fullnode-issuance` 是【系统级、制度性】的货币发行模块；
//! 2. 用于在 Substrate PoW 共识下，对成功铸造新区块的【全节点】发放铸块奖励；
//! 3. 本模块不属于治理参数范畴，不接受链上治理修改；
//! 4. 本模块只保留发奖资格、奖励账户绑定与节点守卫审计所需的最小 Runtime Storage；发行金额与高度规则完全由编译期常量决定。
//!
//! 二、发行规则（写死于 primitives::pow_const）
//! ---------------------------------------------------------------------------
//! 1. 单块奖励金额：每成功铸造 1 个新区块，系统铸造并发放：9999.00 元数字公民币（即 999,900 分）；
//! 2. 奖励区块高度区间（含首尾）：起始区块高度：1 ，结束区块高度：9,999,999；
//! 3. 发行终止规则（永久）：当区块高度 > 9,999,999 时：系统永久停止全节点 PoW 铸块奖励发行，后续新区块不再产生任何全节点铸块奖励；
//! 4. 发行总量（仅用于审计）：总发行区块数：9,999,999 个，全节点铸块奖励总发行量：999,900 × 9,999,999 = 99,989,990,001.00 元。
//!
//! 三、技术实现原则
//! ---------------------------------------------------------------------------
//! 1. 本模块不参与PoW共识过程，仅消费共识结果，PoW共识由Substrate框架原生实现，通过PreRuntime Digest + FindAuthor获取区块作者；
//! 2. 本模块不保存逐块奖励列表，只保存累计奖励块数、累计发行额和最近一次奖励审计元组，供节点原生守卫独立复算；
//! 3. 奖励发放时机：奖励在区块执行完成后的 on_finalize 阶段发放，属于对“已完成铸块行为”的结算，而非预测性激励；
//! 4. 区块高度作为唯一时间与次数约束，区块高度全网一致、不可篡改的事实状态，不依赖任何人为或治理输入。
//!
//! 四、不可改动声明（对以下内容的任何修改，都会构成【货币发行制度的根本性变更】）
//! ---------------------------------------------------------------------------
//! 1. 单块奖励金额；
//! 2. 奖励起止区块高度；
//! 3. 永久停止发行的规则；
//! 4. 发行触发条件（PoW 铸块）。
//!
//! 上述内容不得修改。
//! ============================================================================

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;
pub mod weights;

pub use pallet::*;
pub use weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
    use crate::weights::WeightInfo;
    use frame_support::{
        pallet_prelude::*,
        traits::{Currency, FindAuthor, Imbalance},
    };
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::{SaturatedConversion, Saturating};
    // 全节点 PoW 发行制度常量（来自 primitives）
    use primitives::pow_const::{
        FULLNODE_BLOCK_REWARD, FULLNODE_REWARD_END_BLOCK, FULLNODE_REWARD_START_BLOCK,
    };

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// 公民币货币系统（通常对接 pallet-balances 或自定义币模块）
        type Currency: Currency<Self::AccountId>;

        /// PoW 区块作者查找接口（来自 Substrate 共识层）
        type FindAuthor: FindAuthor<Self::AccountId>;

        /// 权重信息（由 benchmark 自动生成或手动估算）
        type WeightInfo: crate::weights::WeightInfo;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    /// 余额类型别名
    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
    /// 最近一次奖励审计记录的固定字段布局。
    pub type RewardAudit<AccountId, Balance> = (u32, AccountId, AccountId, Balance);

    /// 矿工身份账户（powr）到奖励接收账户的绑定表。
    #[pallet::storage]
    #[pallet::getter(fn reward_account_id_by_miner)]
    pub type RewardAccountIdByMiner<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, T::AccountId, OptionQuery>;

    /// 矿工身份账户（powr）最近一次真实出块的区块高度。
    #[pallet::storage]
    #[pallet::getter(fn last_authored_block_by_miner)]
    pub type LastAuthoredBlockByMiner<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, OptionQuery>;

    /// 已成功发放全节点 PoW 奖励的区块数量。
    ///
    /// 本值只提供可被节点原生守卫复算的审计落点，制度真源仍是节点二进制中的固定高度区间。
    #[pallet::storage]
    #[pallet::getter(fn rewarded_block_count)]
    pub type RewardedBlockCount<T: Config> = StorageValue<_, u32, ValueQuery>;

    /// 全节点 PoW 奖励累计发行额，单位与 Balances 一致。
    #[pallet::storage]
    #[pallet::getter(fn total_fullnode_issued)]
    pub type TotalFullnodeIssued<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    /// 最近一次全节点奖励审计记录：`(区块高度, 矿工账户, 奖励接收账户, 金额)`。
    #[pallet::storage]
    #[pallet::getter(fn last_reward_audit)]
    pub type LastRewardAudit<T: Config> =
        StorageValue<_, RewardAudit<T::AccountId, BalanceOf<T>>, OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// powr 矿工身份完成一次性奖励接收账户绑定。
        RewardAccountBound {
            miner_account_id: T::AccountId,
            reward_account_id: T::AccountId,
        },
        /// 本区块 PoW 奖励已发放到绑定账户。
        FullnodeIssuanceIssued {
            block: u32,
            miner_account_id: T::AccountId,
            reward_account_id: T::AccountId,
            amount: BalanceOf<T>,
        },
        /// 本区块奖励跳过：未能从 digest 识别出作者。
        FullnodeIssuanceSkippedNoAuthor { block: u32 },
        /// 矿工身份奖励接收账户重新绑定。
        RewardAccountRebound {
            miner_account_id: T::AccountId,
            new_reward_account_id: T::AccountId,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// 同一个矿工身份只允许绑定一次奖励接收账户。
        RewardAccountAlreadyBound,
        /// 矿工身份未绑定奖励接收账户。
        RewardAccountNotBound,
        /// 奖励接收账户不得与矿工身份账户相同。
        RewardAccountCannotBeMiner,
        /// 新奖励接收账户必须不同于当前绑定账户。
        RewardAccountUnchanged,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 由矿工身份账户（powr 对应账户）发起一次性绑定。
        ///
        /// 语义为「预绑定」：不要求矿工已经出过块。新装节点的 powr 账户由首启自动生成，
        /// 而能否抢到 PoW 出块是概率事件（本链交易池为空时还会跳过挖矿），若以出块记录
        /// 作为绑定门槛，算力弱的节点将长期甚至永远无法配置收款账户，且其首个区块的
        /// 全节点手续费分成必然按 `RewardAccountUnbound` 销毁。绑定表只在出块时被
        /// `get(&author)` 读取，未出块账户登记的记录读不到、不产生任何效果，故该门槛
        /// 收益为零而代价实在。防垃圾登记由交易手续费本身承担。
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::bind_reward_account())]
        pub fn bind_reward_account(
            origin: OriginFor<T>,
            reward_account_id: T::AccountId,
        ) -> DispatchResult {
            let miner_account_id = ensure_signed(origin)?;
            ensure!(
                !RewardAccountIdByMiner::<T>::contains_key(&miner_account_id),
                Error::<T>::RewardAccountAlreadyBound
            );
            ensure!(
                reward_account_id != miner_account_id,
                Error::<T>::RewardAccountCannotBeMiner
            );

            // 绑定表只决定奖励接收账户，不改变出块作者身份本身。
            RewardAccountIdByMiner::<T>::insert(&miner_account_id, &reward_account_id);
            Self::deposit_event(Event::<T>::RewardAccountBound {
                miner_account_id,
                reward_account_id,
            });
            Ok(())
        }

        /// 允许矿工身份账户主动重绑奖励接收账户（无需治理权限）。
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::rebind_reward_account())]
        pub fn rebind_reward_account(
            origin: OriginFor<T>,
            new_reward_account_id: T::AccountId,
        ) -> DispatchResult {
            let miner_account_id = ensure_signed(origin)?;
            let current_reward_account_id = RewardAccountIdByMiner::<T>::get(&miner_account_id)
                .ok_or(Error::<T>::RewardAccountNotBound)?;
            ensure!(
                new_reward_account_id != miner_account_id,
                Error::<T>::RewardAccountCannotBeMiner
            );
            ensure!(
                new_reward_account_id != current_reward_account_id,
                Error::<T>::RewardAccountUnchanged
            );
            // 重绑后仅影响后续区块奖励，历史已经发放的奖励不会被追溯重定向。
            RewardAccountIdByMiner::<T>::insert(&miner_account_id, &new_reward_account_id);
            Self::deposit_event(Event::<T>::RewardAccountRebound {
                miner_account_id,
                new_reward_account_id,
            });
            Ok(())
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        #[cfg(feature = "std")]
        fn integrity_test() {
            let reward: BalanceOf<T> = FULLNODE_BLOCK_REWARD.saturated_into();
            let reward_back: u128 = reward.saturated_into();
            assert_eq!(
                reward_back, FULLNODE_BLOCK_REWARD,
                "FULLNODE_BLOCK_REWARD must fit into runtime Balance"
            );
        }

        fn on_initialize(n: BlockNumberFor<T>) -> Weight {
            let block_number = n.saturated_into::<u64>();
            if block_number >= u64::from(FULLNODE_REWARD_START_BLOCK)
                && block_number <= u64::from(FULLNODE_REWARD_END_BLOCK)
            {
                // 预申报 on_finalize 最坏路径预算：
                // digest + 真实出块记录 + 奖励账户映射 + balances/issuance + event 相关读写
                T::DbWeight::get().reads_writes(5, 7)
            } else {
                Weight::zero()
            }
        }

        fn on_finalize(n: BlockNumberFor<T>) {
            // 区间判断使用 u64，避免把运行时 BlockNumber 强绑定为 u32。
            let block_number_u64 = n.saturated_into::<u64>();

            // 是否处于全节点 PoW 奖励区间 [1, 9,999,999]
            if block_number_u64 < u64::from(FULLNODE_REWARD_START_BLOCK)
                || block_number_u64 > u64::from(FULLNODE_REWARD_END_BLOCK)
            {
                return;
            }
            // 固定奖励区间本身写死在 u32 范围内，进入区间后再转为存储和事件字段。
            let block_number: u32 = block_number_u64.saturated_into();

            // 从共识 PreRuntime Digest 中获取 PoW 出块作者
            let digest = <frame_system::Pallet<T>>::digest();
            let pre_runtime_digests = digest.logs().iter().filter_map(|d| d.as_pre_runtime());

            let author = match T::FindAuthor::find_author(pre_runtime_digests) {
                Some(a) => a,
                None => {
                    Self::deposit_event(Event::<T>::FullnodeIssuanceSkippedNoAuthor {
                        block: block_number,
                    });
                    return;
                } // 理论上不应发生，发生则不发奖励
            };

            // 只记录共识 digest 证明的真实出块高度；绑定奖励接收账户无需等待首次出块。
            LastAuthoredBlockByMiner::<T>::insert(&author, block_number);

            // 已绑定奖励接收账户则发到该账户，未绑定则默认发到矿工自身账户。
            let recipient_account_id =
                RewardAccountIdByMiner::<T>::get(&author).unwrap_or_else(|| author.clone());

            // 发放固定的全节点 PoW 铸块奖励
            // 奖励金额完全由制度常量决定，绑定表只决定”发给谁”，不影响”发多少”。
            let reward: BalanceOf<T> = FULLNODE_BLOCK_REWARD.saturated_into();
            // deposit_creating 会在奖励账户尚未建户时自动建户，并同步增加总发行量。
            let imbalance = T::Currency::deposit_creating(&recipient_account_id, reward);
            debug_assert_eq!(
                imbalance.peek(),
                reward,
                "deposit_creating must return full reward"
            );

            // 三个审计状态由节点守卫按高度、PoW 作者和固定奖励独立复算；runtime 升级即使改写
            // 本模块逻辑，只要产出与固定公式不一致，诚实节点仍会在 BlockImport 层拒绝该块。
            RewardedBlockCount::<T>::mutate(|count| *count = count.saturating_add(1));
            TotalFullnodeIssued::<T>::mutate(|total| *total = total.saturating_add(reward));
            LastRewardAudit::<T>::put((
                block_number,
                author.clone(),
                recipient_account_id.clone(),
                reward,
            ));
            Self::deposit_event(Event::<T>::FullnodeIssuanceIssued {
                block: block_number,
                miner_account_id: author,
                reward_account_id: recipient_account_id,
                amount: reward,
            });
        }
    }
}

#[cfg(test)]
mod tests;
