//! SquarePost 订阅签名操作的 FRAME benchmark。

#![cfg(feature = "runtime-benchmarks")]

use crate::{
    pallet::{
        Call, CidNumberOf, Config, CreatorPlans, CreatorTierNames, Pallet, PlatformPrice, PostIdOf,
        RenewalIndex, RenewalSchedule, SquarePosts, Subscriptions,
    },
    BillingPeriod, CreatorTier, CreatorTierInput, CreatorTiers, IssuerKey, MembershipLevel,
    PeriodPrice, PeriodPrices, SquarePostCitizenIdentityProvider, SquarePostType, SubscriptionPlan,
    SubscriptionState, SubscriptionStatus, TierId, TierName,
};
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
// 基准也用于 no_std WASM，沿用本模块的 sp_std 显式导入 Vec 与 vec! 宏。
use sp_std::{vec, vec::Vec};

#[benchmarks]
mod benchmarks {
    use super::*;

    fn active_platform_state() -> SubscriptionState {
        SubscriptionState {
            plan: SubscriptionPlan::Platform {
                membership_level: MembershipLevel::Freedom,
            },
            started_at: 1,
            last_charged_at: 1,
            last_charged_price_fen: 1,
            paid_until: 2,
            subscription_status: SubscriptionStatus::Active,
            authorized_price_fen: 1,
            suspend_reason: None,
        }
    }

    fn benchmark_cid<T: Config>(account_id: &T::AccountId) -> CidNumberOf<T> {
        T::CitizenIdentity::benchmark_seed_identity(account_id)
            .try_into()
            .expect("benchmark CID must fit SquarePost bounds")
    }

    fn benchmark_tier_input(index: u8) -> CreatorTierInput {
        CreatorTierInput {
            tier_id: TierId::try_from(vec![b'a' + index]).expect("benchmark tier id fits"),
            tier_name: TierName::try_from(vec![b'n', b'0' + index])
                .expect("benchmark tier name fits"),
            prices_fen: PeriodPrices::try_from(vec![PeriodPrice {
                billing_period: BillingPeriod::Monthly,
                price_fen: u128::from(index) + 1,
            }])
            .expect("benchmark prices fit"),
        }
    }

    /// 竞选动态是发布入口最重路径：除 active CID 双向绑定外，还必须读取竞选身份。
    #[benchmark]
    fn publish_post() {
        let caller: T::AccountId = whitelisted_caller();
        let cid_number = benchmark_cid::<T>(&caller);
        let mut membership = active_platform_state();
        membership.paid_until = u64::MAX;
        Subscriptions::<T>::insert((cid_number, IssuerKey::Platform), membership);
        let post_id = b"benchmark-campaign-post".to_vec();

        #[extrinsic_call]
        _(
            RawOrigin::Signed(caller),
            post_id.clone(),
            SquarePostType::Video,
            [7u8; 32],
            b"benchmark-storage-receipt".to_vec(),
        );

        let post_id: PostIdOf<T> = post_id.try_into().expect("benchmark post id fits");
        assert!(SquarePosts::<T>::contains_key(post_id));
    }

    #[benchmark]
    fn cancel() {
        let caller: T::AccountId = whitelisted_caller();
        let caller_cid_number = benchmark_cid::<T>(&caller);
        let key = (caller_cid_number.clone(), IssuerKey::Platform);
        Subscriptions::<T>::insert(&key, active_platform_state());
        RenewalSchedule::<T>::insert(2u64.to_be_bytes(), &key, ());
        RenewalIndex::<T>::insert(&key, 2u64);

        #[extrinsic_call]
        _(RawOrigin::Signed(caller.clone()), IssuerKey::Platform);

        assert_eq!(
            Subscriptions::<T>::get((caller_cid_number, IssuerKey::Platform))
                .expect("benchmark state exists")
                .subscription_status,
            SubscriptionStatus::Cancelled
        );
    }

    /// 覆盖十档是名称删除、名称写入和付款计划写入的最重路径。
    #[benchmark]
    fn set_creator_plans() {
        let caller: T::AccountId = whitelisted_caller();
        let caller_cid_number = benchmark_cid::<T>(&caller);
        let mut membership = active_platform_state();
        membership.paid_until = u64::MAX;
        Subscriptions::<T>::insert((caller_cid_number.clone(), IssuerKey::Platform), membership);
        let tiers = (0u8..10).map(benchmark_tier_input).collect::<Vec<_>>();

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), tiers);

        assert_eq!(CreatorPlans::<T>::get(caller_cid_number).len(), 10);
    }

    #[benchmark]
    fn update_creator_tier_name() {
        let caller: T::AccountId = whitelisted_caller();
        let caller_cid_number = benchmark_cid::<T>(&caller);
        let input = benchmark_tier_input(0);
        let tier_id = input.tier_id.clone();
        CreatorPlans::<T>::insert(
            &caller_cid_number,
            CreatorTiers::try_from(vec![CreatorTier {
                tier_id: input.tier_id,
                prices_fen: input.prices_fen,
            }])
            .expect("benchmark tiers fit"),
        );
        let tier_name = TierName::try_from(b"renamed".to_vec()).expect("name fits");

        #[extrinsic_call]
        _(
            RawOrigin::Signed(caller),
            tier_id.clone(),
            tier_name.clone(),
        );

        assert_eq!(
            CreatorTierNames::<T>::get(caller_cid_number, tier_id),
            Some(tier_name)
        );
    }

    /// 单笔到期续费处理路径（on_initialize 按实际处理笔数记账）。
    #[benchmark]
    fn process_one_due() {
        let subscriber_account_id: T::AccountId = whitelisted_caller();
        let subscriber_cid_number = benchmark_cid::<T>(&subscriber_account_id);
        let key = (subscriber_cid_number, IssuerKey::Platform);
        PlatformPrice::<T>::insert(MembershipLevel::Freedom, 199_900u128);
        Subscriptions::<T>::insert(&key, active_platform_state());
        RenewalSchedule::<T>::insert(2u64.to_be_bytes(), &key, ());
        RenewalIndex::<T>::insert(&key, 2u64);

        #[block]
        {
            Pallet::<T>::process_due_subscriptions(3u64, 1);
        }

        assert!(!RenewalSchedule::<T>::contains_key(
            2u64.to_be_bytes(),
            &key
        ));
    }

    impl_benchmark_test_suite!(Pallet, crate::tests::new_test_ext(), crate::tests::Test,);
}
