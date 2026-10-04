//! # 公民轻节点认证奖励发行模块 (citizen-issuance)
//!
//! 本模块在公民**首次上链**(投票身份登记或竞选身份首建,二者同权)时，通过
//! `OnVotingIdentityRegistered` 回调登记本块待发奖励，并在同一块 `on_finalize` 中自动
//! 发放一次性认证奖励。竞选身份不要求先有投票身份，首次即以竞选身份上链同样发币。
//!
//! ## 核心规则
//! - 双重防重：按规范 `cid_number` + 按账户，防止同一公民或同一账户重复领奖。
//! - 阶梯奖励：前 `CITIZEN_ISSUANCE_HIGH_REWARD_COUNT` 人获高额奖励，之后降为常规奖励。
//! - 总量硬顶：累计发放人数达到 `CITIZEN_ISSUANCE_MAX_COUNT` 后停止发放。
//! - 本模块不暴露任何 extrinsic，所有触发均来自上游回调。

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;
pub mod weights;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use citizen_identity::{OnVotingIdentityRegistered, OnVotingIdentityRegisteredWeight};
    use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
    use frame_support::{
        pallet_prelude::*,
        traits::{Currency, Imbalance},
        Blake2_128Concat, Twox64Concat,
    };
    use frame_system::pallet_prelude::BlockNumberFor;
    use scale_info::TypeInfo;
    use sp_runtime::traits::{SaturatedConversion, Zero};
    use sp_runtime::Debug;

    use crate::weights::WeightInfo;
    use primitives::citizen_const::{
        CITIZEN_ISSUANCE_HIGH_REWARD, CITIZEN_ISSUANCE_HIGH_REWARD_COUNT,
        CITIZEN_ISSUANCE_MAX_COUNT, CITIZEN_ISSUANCE_NORMAL_REWARD, CITIZEN_ISSUANCE_ONE_TIME_ONLY,
    };

    // 链上规则强制“一次性奖励”，禁止通过配置关闭该约束。
    const _: () = assert!(
        CITIZEN_ISSUANCE_ONE_TIME_ONLY,
        "CITIZEN_ISSUANCE_ONE_TIME_ONLY must be true"
    );
    // 奖励金额属于制度常量，必须在编译期保持非零。
    const _: () = assert!(
        CITIZEN_ISSUANCE_HIGH_REWARD > 0,
        "CITIZEN_ISSUANCE_HIGH_REWARD must be greater than zero"
    );
    // 常规奖励同样不允许配置成零，零奖励只保留运行时类型转换兜底。
    const _: () = assert!(
        CITIZEN_ISSUANCE_NORMAL_REWARD > 0,
        "CITIZEN_ISSUANCE_NORMAL_REWARD must be greater than zero"
    );

    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Currency: Currency<Self::AccountId>;
        type WeightInfo: crate::weights::WeightInfo;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn rewarded_count)]
    /// 全局累计已领奖人数，用于控制总发放上限与奖励档位切换。
    pub type RewardedCount<T> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn reward_claimed)]
    /// 按规范 cid_number 防重，确保同一公民身份不会重复领取奖励。
    // unit 是 StorageMap 的存在标记值，不是函数返回类型，不能按 lint 建议删除。
    #[allow(clippy::unused_unit)]
    pub type IdentityRewardClaimed<T: Config> =
        StorageMap<_, Blake2_128Concat, citizen_identity::CidNumberBound, (), ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn account_rewarded)]
    /// 按账户维度再做一次防重，避免同一账户换绑 CID 后再次领奖。
    // unit 是 StorageMap 的存在标记值，不是函数返回类型，不能按 lint 建议删除。
    #[allow(clippy::unused_unit)]
    pub type AccountRewarded<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, (), ValueQuery>;

    /// finalize 前可由节点读取的单笔待发奖励凭据；金额由累计序号和制度常量独立推导。
    #[derive(
        Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
    )]
    pub struct PendingCertificationReward<AccountId> {
        pub account_id: AccountId,
        /// 奖励唯一身份键；账户只是第二防重键和实际收款账户。
        pub cid_number: citizen_identity::CidNumberBound,
    }

    #[pallet::storage]
    /// 本块已排队的奖励数量；`on_finalize` 必须消费后归零。
    pub type PendingRewardCount<T> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    /// 按本块登记顺序保存待发凭据，保证跨奖励档位时顺序可复算。
    pub type PendingRewards<T: Config> =
        StorageMap<_, Twox64Concat, u32, PendingCertificationReward<T::AccountId>, OptionQuery>;

    #[pallet::storage]
    /// 本块规范 CID 临时防重表；finalize 后必须清空，不形成第二套永久真源。
    pub type PendingIdentityRewardClaimed<T: Config> =
        StorageMap<_, Blake2_128Concat, citizen_identity::CidNumberBound, (), ValueQuery>;

    #[pallet::storage]
    /// 本块账户临时防重表；finalize 后必须清空，不形成第二套永久真源。
    pub type PendingAccountRewarded<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, (), ValueQuery>;

    /// 描述奖励被跳过的具体原因，用于链上事件记录和前端展示。
    #[derive(
        Clone,
        Copy,
        Encode,
        Decode,
        DecodeWithMemTracking,
        Eq,
        PartialEq,
        Debug,
        TypeInfo,
        MaxEncodedLen,
    )]
    pub enum SkipReason {
        /// 同一公民身份已经领取过奖励。
        DuplicateCitizenIdentity,
        /// 全局累计发放人数已达上限。
        MaxCountReached,
        /// 该账户已通过其他公民身份领取过奖励，不可再领。
        AccountAlreadyRewarded,
        /// 奖励常量已由编译期断言锁定为非零；该分支只兜底 Balance 转换异常。
        ZeroRewardConfigured,
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 公民投票身份首次登记后，认证发行模块执行一次奖励发放。
        CertificationRewardIssued {
            account_id: T::AccountId,
            cid_number: citizen_identity::CidNumberBound,
            reward: BalanceOf<T>,
        },
        /// 奖励因重复、超限等原因被跳过时触发，reason 字段说明具体原因。
        CertificationRewardSkipped {
            account_id: T::AccountId,
            cid_number: citizen_identity::CidNumberBound,
            reason: SkipReason,
        },
    }

    /// 本模块不暴露 extrinsic，所有逻辑通过回调触发，因此无需定义错误类型。
    #[pallet::error]
    pub enum Error<T> {}

    /// 本模块不暴露 extrinsic，奖励发放由公民身份登记回调驱动，无需用户直接调用。
    #[pallet::call]
    impl<T: Config> Pallet<T> {}

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_finalize(_n: BlockNumberFor<T>) {
            let pending_count = PendingRewardCount::<T>::take();
            for index in 0..pending_count {
                let Some(pending) = PendingRewards::<T>::take(index) else {
                    // 正常 runtime 不会形成空洞；节点守卫会在区块导入前 fail-closed。
                    debug_assert!(false, "公民认证待发队列存在空洞:index={index}");
                    continue;
                };

                let rewarded_count = RewardedCount::<T>::get();
                let reward_amount = Self::reward_amount_at(rewarded_count);
                let reward: BalanceOf<T> = reward_amount.saturated_into();
                debug_assert!(!reward.is_zero(), "citizen reward must remain non-zero");

                let imbalance = T::Currency::deposit_creating(&pending.account_id, reward);
                debug_assert_eq!(
                    imbalance.peek(),
                    reward,
                    "deposit_creating must return full citizen reward"
                );
                // 先结算发行凭证，使 Balances::Issued 事件稳定早于本模块业务事件。
                drop(imbalance);

                RewardedCount::<T>::put(rewarded_count.saturating_add(1));
                IdentityRewardClaimed::<T>::insert(&pending.cid_number, ());
                AccountRewarded::<T>::insert(&pending.account_id, ());
                PendingIdentityRewardClaimed::<T>::remove(&pending.cid_number);
                PendingAccountRewarded::<T>::remove(&pending.account_id);

                Self::deposit_event(Event::<T>::CertificationRewardIssued {
                    account_id: pending.account_id,
                    cid_number: pending.cid_number,
                    reward,
                });
            }
        }
    }

    impl<T: Config> Pallet<T> {
        /// 调用方在 weight 宏中引用此值。
        pub fn on_voting_identity_registered_weight() -> Weight {
            // 上游 citizen-identity 在申报 weight 时会叠加这里的回调预算。
            T::WeightInfo::on_voting_identity_registered()
        }

        fn reward_amount_at(rewarded_count: u64) -> u128 {
            if rewarded_count < CITIZEN_ISSUANCE_HIGH_REWARD_COUNT {
                CITIZEN_ISSUANCE_HIGH_REWARD
            } else {
                CITIZEN_ISSUANCE_NORMAL_REWARD
            }
        }

        fn try_queue_certification_reward(
            account_id: &T::AccountId,
            cid_number: &citizen_identity::CidNumberBound,
        ) -> Result<BalanceOf<T>, SkipReason> {
            // 先查公民身份，再查账户，优先返回更贴近业务语义的跳过原因。
            if IdentityRewardClaimed::<T>::contains_key(cid_number)
                || PendingIdentityRewardClaimed::<T>::contains_key(cid_number)
            {
                return Err(SkipReason::DuplicateCitizenIdentity);
            }

            if AccountRewarded::<T>::contains_key(account_id)
                || PendingAccountRewarded::<T>::contains_key(account_id)
            {
                return Err(SkipReason::AccountAlreadyRewarded);
            }

            let rewarded_count = RewardedCount::<T>::get();
            let pending_count = PendingRewardCount::<T>::get();
            let effective_count = rewarded_count.saturating_add(u64::from(pending_count));
            // 总人数达到上限后直接跳过，不再尝试铸币或写入任何领奖标记。
            if effective_count >= CITIZEN_ISSUANCE_MAX_COUNT {
                return Err(SkipReason::MaxCountReached);
            }

            // 奖励档位完全由全局累计人数决定，避免链下参与者各自推导口径不一致。
            let reward_amount = Self::reward_amount_at(effective_count);

            let reward: BalanceOf<T> = reward_amount.saturated_into();
            debug_assert!(
                !reward.is_zero(),
                "citizen issuance reward constants must stay greater than zero"
            );
            // 制度奖励常量已编译期锁定非零，这里保留为 Balance 类型转换后的防御性兜底。
            if reward.is_zero() {
                return Err(SkipReason::ZeroRewardConfigured);
            }
            let next_pending_count = pending_count
                .checked_add(1)
                .ok_or(SkipReason::MaxCountReached)?;

            PendingRewards::<T>::insert(
                pending_count,
                PendingCertificationReward {
                    account_id: account_id.clone(),
                    cid_number: cid_number.clone(),
                },
            );
            PendingIdentityRewardClaimed::<T>::insert(cid_number, ());
            PendingAccountRewarded::<T>::insert(account_id, ());
            PendingRewardCount::<T>::put(next_pending_count);

            Ok(reward)
        }
    }

    /// 实现 citizen-identity 的登记回调，在公民投票身份首次登记后自动尝试发放认证奖励。
    impl<T: Config> OnVotingIdentityRegistered<T::AccountId> for Pallet<T> {
        fn on_voting_identity_registered(
            account_id: &T::AccountId,
            cid_number: &citizen_identity::CidNumberBound,
        ) {
            match Self::try_queue_certification_reward(account_id, cid_number) {
                Ok(_reward) => {}
                Err(reason) => {
                    Self::deposit_event(Event::<T>::CertificationRewardSkipped {
                        account_id: account_id.clone(),
                        cid_number: cid_number.clone(),
                        reason,
                    });
                }
            }
        }
    }

    /// 向上游提供回调的 weight 预算，供 citizen-identity 在申报交易权重时叠加。
    impl<T: Config> OnVotingIdentityRegisteredWeight for Pallet<T> {
        fn on_voting_identity_registered_weight() -> Weight {
            Pallet::<T>::on_voting_identity_registered_weight()
        }
    }
}

#[cfg(test)]
mod tests;
