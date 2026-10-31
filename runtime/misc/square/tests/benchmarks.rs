//! SquarePost 订阅签名操作的 FRAME benchmark。

#![cfg(feature = "runtime-benchmarks")]

use crate::{
    pallet::{
        Call, CidNumberOf, Config, CreatorPlans, CreatorTierNames, Pallet, PlatformPrice, PostIdOf,
        RenewalIndex, RenewalSchedule, SquarePosts, Subscriptions,
    },
    BillingPeriod, CreatorTier, CreatorTierInput, CreatorTiers, IssuerKey, MembershipLevel,
    PeriodPrice, SquarePostCitizenIdentityProvider, SquarePostType, SubscriptionPlan,
    SubscriptionState, SubscriptionStatus, TierName,
};
use codec::Decode;
use frame_benchmarking::v2::*;
use frame_support::traits::{Currency, Get, Hooks};
use frame_system::RawOrigin;
use sp_runtime::traits::SaturatedConversion;
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
            charge_fen: 1,
            paid_until: 2,
            subscription_status: SubscriptionStatus::Active,
            price_fen: 1,
            suspend_reason: None,
        }
    }

    fn benchmark_cid<T: Config>(account_id: &T::AccountId) -> CidNumberOf<T> {
        T::CitizenIdentity::benchmark_seed_identity(account_id)
            .try_into()
            .expect("benchmark CID must fit SquarePost bounds")
    }

    fn benchmark_tier_input<T: Config>(index: u8) -> CreatorTierInput {
        CreatorTierInput {
            tier_id: vec![b'a' + index; 32].try_into().unwrap(),
            tier_name: "𠮷".repeat(20).into_bytes().try_into().unwrap(),
            prices_fen: [
                BillingPeriod::Monthly,
                BillingPeriod::Quarterly,
                BillingPeriod::Yearly,
            ]
            .into_iter()
            .map(|billing_period| PeriodPrice {
                billing_period,
                price_fen: (u128::from(index) + 1)
                    * T::Currency::minimum_balance()
                        .saturated_into::<u128>()
                        .max(1),
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap(),
        }
    }

    fn creator_fixture<T: Config>(
        caller: &T::AccountId,
    ) -> (CidNumberOf<T>, CidNumberOf<T>, SubscriptionPlan) {
        let cid = benchmark_cid::<T>(caller);
        let creator = account::<T::AccountId>("creator-payee", 0, 0);
        let creator_cid = benchmark_cid::<T>(&creator);
        assert_ne!(cid, creator_cid);
        let mut membership = active_platform_state();
        membership.paid_until = u64::MAX;
        Subscriptions::<T>::insert((creator_cid.clone(), IssuerKey::Platform), membership);
        CreatorPlans::<T>::insert(
            &creator_cid,
            CreatorTiers::try_from(
                (0u8..10)
                    .map(benchmark_tier_input::<T>)
                    .map(|tier| CreatorTier {
                        tier_id: tier.tier_id,
                        prices_fen: tier.prices_fen,
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
        );
        drop(T::Currency::make_free_balance_be(
            caller,
            T::Currency::minimum_balance()
                .saturated_into::<u128>()
                .saturating_mul(1000000)
                .max(1000000000)
                .saturated_into(),
        ));
        (
            cid,
            creator_cid,
            SubscriptionPlan::Creator {
                tier_id: benchmark_tier_input::<T>(9).tier_id,
                billing_period: BillingPeriod::Yearly,
            },
        )
    }

    /// 竞选动态是发布入口最重路径：除 active CID 双向绑定外，还必须读取竞选身份。
    #[benchmark]
    fn publish_post() {
        let caller: T::AccountId = whitelisted_caller();
        let cid_number = benchmark_cid::<T>(&caller);
        let mut membership = active_platform_state();
        membership.paid_until = u64::MAX;
        Subscriptions::<T>::insert((cid_number, IssuerKey::Platform), membership);
        let post_id = vec![b'p'; T::MaxSquarePostIdLen::get() as usize];

        #[extrinsic_call]
        _(
            RawOrigin::Signed(caller),
            post_id.clone(),
            SquarePostType::Video,
            [7u8; 32],
            vec![b'r'; T::MaxSquareStorageReceiptIdLen::get() as usize],
        );

        let post_id: PostIdOf<T> = post_id.try_into().expect("benchmark post id fits");
        assert!(SquarePosts::<T>::contains_key(post_id));
    }

    #[benchmark]
    fn cancel() {
        let caller: T::AccountId = whitelisted_caller();
        let (cid, creator, plan) = creator_fixture::<T>(&caller);
        let issuer = IssuerKey::Creator(creator);
        let key = (cid.clone(), issuer.clone());
        let mut state = active_platform_state();
        state.plan = plan;
        Subscriptions::<T>::insert(&key, state);
        RenewalSchedule::<T>::insert(2u64.to_be_bytes(), &key, ());
        RenewalIndex::<T>::insert(&key, 2u64);
        #[extrinsic_call]
        _(RawOrigin::Signed(caller), issuer);
        assert_eq!(
            Subscriptions::<T>::get(key).unwrap().subscription_status,
            SubscriptionStatus::Cancelled
        );
    }

    /// 覆盖十档是名称删除、名称写入和付款计划写入的最重路径。
    #[benchmark]
    fn set_creator_plans(n: Linear<1, 10>) {
        let caller: T::AccountId = whitelisted_caller();
        let caller_cid_number = benchmark_cid::<T>(&caller);
        let mut membership = active_platform_state();
        membership.paid_until = u64::MAX;
        Subscriptions::<T>::insert((caller_cid_number.clone(), IssuerKey::Platform), membership);
        let old = (0u8..10).map(benchmark_tier_input::<T>).collect::<Vec<_>>();
        for tier in &old {
            CreatorTierNames::<T>::insert(&caller_cid_number, &tier.tier_id, &tier.tier_name);
        }
        CreatorPlans::<T>::insert(
            &caller_cid_number,
            CreatorTiers::try_from(
                old.into_iter()
                    .map(|tier| CreatorTier {
                        tier_id: tier.tier_id,
                        prices_fen: tier.prices_fen,
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
        );
        let expected = n as usize;
        let tiers = (0..n as u8)
            .map(benchmark_tier_input::<T>)
            .collect::<Vec<_>>();

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), tiers);

        assert_eq!(CreatorPlans::<T>::get(caller_cid_number).len(), expected);
    }

    #[benchmark]
    fn update_creator_tier_name() {
        let caller: T::AccountId = whitelisted_caller();
        let caller_cid_number = benchmark_cid::<T>(&caller);
        let input = benchmark_tier_input::<T>(9);
        let tier_id = input.tier_id.clone();
        CreatorPlans::<T>::insert(
            &caller_cid_number,
            CreatorTiers::try_from(
                (0u8..10)
                    .map(benchmark_tier_input::<T>)
                    .map(|tier| CreatorTier {
                        tier_id: tier.tier_id,
                        prices_fen: tier.prices_fen,
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
        );
        let tier_name: TierName = "𠮷".repeat(20).into_bytes().try_into().unwrap();

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
        let subscriber_account_id: T::AccountId = account("renewal-failure", 0, 0);
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
    #[benchmark]
    fn subscribe() {
        let caller: T::AccountId = whitelisted_caller();
        let (cid, creator, plan) = creator_fixture::<T>(&caller);
        let issuer = IssuerKey::Creator(creator);
        let key = (cid.clone(), issuer.clone());
        let now = Pallet::<T>::now_ms();
        let mut old = active_platform_state();
        old.plan = plan.clone();
        old.subscription_status = SubscriptionStatus::Cancelled;
        old.paid_until = now.saturating_sub(1);
        Subscriptions::<T>::insert(&key, &old);
        RenewalIndex::<T>::insert(&key, old.paid_until);
        RenewalSchedule::<T>::insert(old.paid_until.to_be_bytes(), &key, ());
        let price = benchmark_tier_input::<T>(9)
            .prices_fen
            .last()
            .unwrap()
            .price_fen;
        #[extrinsic_call]
        _(RawOrigin::Signed(caller), issuer.clone(), plan, price);
        assert_eq!(
            Subscriptions::<T>::get(key).unwrap().subscription_status,
            SubscriptionStatus::Active
        );
    }

    #[benchmark]
    fn change_subscription_plan() {
        let caller: T::AccountId = whitelisted_caller();
        let (cid, creator, new_plan) = creator_fixture::<T>(&caller);
        let issuer = IssuerKey::Creator(creator);
        let key = (cid, issuer.clone());
        let now = Pallet::<T>::now_ms();
        let mut old = active_platform_state();
        old.plan = SubscriptionPlan::Creator {
            tier_id: benchmark_tier_input::<T>(0).tier_id,
            billing_period: BillingPeriod::Monthly,
        };
        old.price_fen = T::Currency::minimum_balance()
            .saturated_into::<u128>()
            .max(1)
            * 5;
        old.charge_fen = old.price_fen;
        old.started_at = now.saturating_sub(1000);
        old.last_charged_at = old.started_at;
        old.paid_until =
            crate::subscription::add_calendar_period(now, BillingPeriod::Monthly).unwrap();
        Subscriptions::<T>::insert(&key, &old);
        RenewalIndex::<T>::insert(&key, old.paid_until);
        RenewalSchedule::<T>::insert(old.paid_until.to_be_bytes(), &key, ());
        let price = benchmark_tier_input::<T>(9)
            .prices_fen
            .last()
            .unwrap()
            .price_fen;
        #[extrinsic_call]
        _(RawOrigin::Signed(caller), issuer, new_plan.clone(), price);
        assert_eq!(Subscriptions::<T>::get(key).unwrap().plan, new_plan);
    }

    #[benchmark]
    fn propose_set_platform_price() {
        use entitybase::InstitutionRoleAuthorizationQuery;
        let foundation = primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION.cid_number;
        let caller =
            T::AccountId::decode(
                &mut &primitives::cid::china::citizenchain::CITIZENCHAIN_GENESIS_ADMINS[0]
                    .account_id[..],
            )
            .unwrap();
        let action = entitybase::BusinessActionId {
            module_tag: crate::MODULE_TAG.to_vec(),
            action_code: entitybase::business_action::ACTION_PLATFORM_PRICE,
        };
        let role = primitives::cid::china::citizenchain::CITIZENCHAIN_GENESIS_ASSIGNMENTS
            .iter()
            .find(|a| {
                T::InstitutionRoleAuthorization::is_authorized(
                    &caller,
                    &entitybase::RoleSubject {
                        cid_number: foundation.as_bytes().to_vec(),
                        role_code: a.role_code.to_vec(),
                    },
                    &action,
                    entitybase::RolePermissionOperation::Propose,
                )
            })
            .expect("平台调价必须使用真实有效岗位")
            .role_code;
        #[extrinsic_call]
        _(
            RawOrigin::Signed(caller),
            foundation.as_bytes().to_vec().try_into().unwrap(),
            role.to_vec().try_into().unwrap(),
            MembershipLevel::Freedom,
            200000u128,
        );
    }

    #[benchmark]
    fn renewal_success() {
        let caller: T::AccountId = account("renewal-success", 0, 0);
        let cid = benchmark_cid::<T>(&caller);
        let key = (cid, IssuerKey::Platform);
        PlatformPrice::<T>::insert(MembershipLevel::Freedom, 199900u128);
        Subscriptions::<T>::insert(&key, active_platform_state());
        RenewalSchedule::<T>::insert(2u64.to_be_bytes(), &key, ());
        RenewalIndex::<T>::insert(&key, 2u64);
        drop(T::Currency::make_free_balance_be(
            &caller,
            1000000000u128.saturated_into(),
        ));
        #[block]
        {
            Pallet::<T>::process_due_subscriptions(3, 1);
        }
        assert_eq!(
            Subscriptions::<T>::get(key).unwrap().subscription_status,
            SubscriptionStatus::Active
        );
    }

    #[benchmark]
    fn renewal_creator() {
        let caller: T::AccountId = account("renewal-creator", 0, 0);
        let (cid, creator, plan) = creator_fixture::<T>(&caller);
        let key = (cid, IssuerKey::Creator(creator));
        let mut state = active_platform_state();
        state.plan = plan;
        state.price_fen = T::Currency::minimum_balance()
            .saturated_into::<u128>()
            .max(1)
            * 10;
        state.charge_fen = state.price_fen;
        Subscriptions::<T>::insert(&key, state);
        RenewalSchedule::<T>::insert(2u64.to_be_bytes(), &key, ());
        RenewalIndex::<T>::insert(&key, 2u64);
        #[block]
        {
            Pallet::<T>::process_due_subscriptions(3, 1);
        }
        assert_eq!(
            Subscriptions::<T>::get(key).unwrap().subscription_status,
            SubscriptionStatus::Active
        );
    }

    #[benchmark]
    fn on_initialize_base() {
        #[block]
        {
            Pallet::<T>::on_initialize(1u32.into());
        }
    }
}
