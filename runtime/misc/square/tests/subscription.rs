//! 复用本模块真实测试环境的行为与边界回归。
use super::*;

#[test]
fn real_calendar_month_keeps_calendar_date_and_time() {
    let source = 1_608_467_696_789u64;
    let expected = 1_611_146_096_789u64;
    assert_eq!(
        crate::subscription::add_calendar_period(source, BillingPeriod::Monthly),
        Some(expected)
    );
}

#[test]
fn real_calendar_handles_leap_year_and_missing_target_date() {
    assert_eq!(
        crate::subscription::add_calendar_period(1_582_938_123_456, BillingPeriod::Yearly),
        Some(1_614_474_123_456)
    );
    assert_eq!(
        crate::subscription::add_calendar_period(1_612_065_906_321, BillingPeriod::Monthly),
        Some(1_614_485_106_321)
    );
}

#[test]
fn subscribe_charges_immediately_and_schedules_real_calendar_due_time() {
    new_test_ext().execute_with(|| {
        setup_platform();
        let before = Balances::free_balance(subscriber_account_id());
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Platform,
            platform_plan(),
            PLATFORM_PRICE,
        ));
        let state = Subscriptions::<Test>::get((subscriber_cid_number(), IssuerKey::Platform))
            .expect("state exists");
        let expected_due =
            crate::subscription::add_calendar_period(1_700_000_000_000, BillingPeriod::Monthly)
                .expect("calendar fits");
        assert_eq!(
            Balances::free_balance(subscriber_account_id()),
            before - PLATFORM_PRICE
        );
        assert_eq!(state.paid_until, expected_due);
        assert_eq!(state.subscription_status, SubscriptionStatus::Active);
        // 5.6a：首扣后已授权价=扣款价，无挂起原因。
        assert_eq!(state.price_fen, PLATFORM_PRICE);
        assert_eq!(state.suspend_reason, None);
        assert_eq!(
            RenewalIndex::<Test>::get((subscriber_cid_number(), IssuerKey::Platform)),
            Some(expected_due)
        );
    });
}

#[test]
fn runtime_renews_at_due_time_without_external_call_and_uses_latest_price() {
    new_test_ext().execute_with(|| {
        setup_platform();
        let key = (subscriber_cid_number(), IssuerKey::Platform);
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Platform,
            platform_plan(),
            PLATFORM_PRICE,
        ));
        let before_price_change = Subscriptions::<Test>::get(&key).expect("state exists");
        assert_eq!(before_price_change.price_fen, PLATFORM_PRICE);
        assert_eq!(
            before_price_change.subscription_status,
            SubscriptionStatus::Active
        );
        assert_eq!(before_price_change.suspend_reason, None);
        let due = RenewalIndex::<Test>::get(&key).expect("scheduled");
        let next_price = PLATFORM_PRICE + 10;
        // 单元测试直接写入投票引擎通过后才会更新的治理价格，聚焦验证续费授权边界。
        PlatformPrice::<Test>::insert(MembershipLevel::Spark, next_price);
        let before = Balances::free_balance(subscriber_account_id());
        initialize_at(due - 1);
        assert_eq!(Balances::free_balance(subscriber_account_id()), before);
        assert_eq!(
            Subscriptions::<Test>::get(&key)
                .expect("state exists")
                .price_fen,
            PLATFORM_PRICE
        );
        initialize_at(due);
        assert_eq!(
            Balances::free_balance(subscriber_account_id()),
            before - next_price
        );
        let state = Subscriptions::<Test>::get(&key).expect("state exists");
        assert_eq!(state.last_charged_at, due);
        assert_eq!(state.charge_fen, next_price);
        assert_eq!(state.price_fen, next_price);
        assert_eq!(state.subscription_status, SubscriptionStatus::Active);
        assert_eq!(state.suspend_reason, None);
        assert!(state.paid_until > due);
        assert_eq!(RenewalIndex::<Test>::get(&key), Some(state.paid_until));
    });
}

#[test]
fn subscriber_rebind_keeps_subscription_and_renews_from_current_account() {
    new_test_ext().execute_with(|| {
        setup_platform();
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Platform,
            platform_plan(),
            PLATFORM_PRICE,
        ));
        let key = (subscriber_cid_number(), IssuerKey::Platform);
        let due = RenewalIndex::<Test>::get(&key).expect("scheduled");
        let old_before = Balances::free_balance(subscriber_account_id());
        let new_before = Balances::free_balance(rebound_subscriber_account_id());

        // CID 换绑只替换当前扣款凭证；订阅 storage 不搬迁、不改键。
        bind_identity(SUBSCRIBER_CID, rebound_subscriber_account_id());
        initialize_at(due);

        assert_eq!(Balances::free_balance(subscriber_account_id()), old_before);
        assert_eq!(
            Balances::free_balance(rebound_subscriber_account_id()),
            new_before - PLATFORM_PRICE
        );
        assert_eq!(
            Subscriptions::<Test>::get(&key)
                .expect("same CID subscription remains")
                .subscription_status,
            SubscriptionStatus::Active
        );
    });
}

#[test]
fn renewal_without_current_identity_binding_suspends_without_charging_old_account() {
    new_test_ext().execute_with(|| {
        setup_platform();
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Platform,
            platform_plan(),
            PLATFORM_PRICE,
        ));
        let key = (subscriber_cid_number(), IssuerKey::Platform);
        let due = RenewalIndex::<Test>::get(&key).expect("scheduled");
        let old_before = Balances::free_balance(subscriber_account_id());

        unbind_identity(SUBSCRIBER_CID);
        initialize_at(due);

        assert_eq!(Balances::free_balance(subscriber_account_id()), old_before);
        let state = Subscriptions::<Test>::get(&key).expect("subscription remains");
        assert_eq!(state.subscription_status, SubscriptionStatus::Suspended);
        assert_eq!(
            state.suspend_reason,
            Some(crate::SuspendReason::IdentityBindingUnavailable)
        );
        assert!(!RenewalIndex::<Test>::contains_key(&key));
    });
}

#[test]
fn runtime_catches_up_every_due_period_after_blocks_resume() {
    new_test_ext().execute_with(|| {
        setup_platform();
        set_now(1_608_467_696_789);
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Platform,
            platform_plan(),
            PLATFORM_PRICE,
        ));
        let before = Balances::free_balance(subscriber_account_id());
        initialize_at(1_616_243_696_789);
        assert_eq!(
            Balances::free_balance(subscriber_account_id()),
            before - PLATFORM_PRICE * 3
        );
        let state = Subscriptions::<Test>::get((subscriber_cid_number(), IssuerKey::Platform))
            .expect("state exists");
        assert!(state.paid_until > 1_616_243_696_789);
    });
}

#[test]
fn payment_failure_suspends_and_removes_schedule() {
    new_test_ext().execute_with(|| {
        setup_platform();
        let key = (bounded_cid_number(POOR_CID), IssuerKey::Platform);
        Subscriptions::<Test>::insert(
            &key,
            SubscriptionState {
                plan: platform_plan(),
                started_at: 1,
                last_charged_at: 1,
                charge_fen: PLATFORM_PRICE,
                paid_until: 2,
                subscription_status: SubscriptionStatus::Active,
                price_fen: PLATFORM_PRICE,
                suspend_reason: None,
            },
        );
        SquarePost::schedule_renewal(&key, 2);
        initialize_at(2);
        // 余额不足 → 挂起（不再终止），退出续费调度，保留订阅。
        let state = Subscriptions::<Test>::get(&key).expect("state exists");
        assert_eq!(state.subscription_status, SubscriptionStatus::Suspended);
        assert_eq!(
            state.suspend_reason,
            Some(crate::SuspendReason::InsufficientBalance)
        );
        assert!(!RenewalIndex::<Test>::contains_key(&key));
    });
}

#[test]
fn cancellation_preserves_paid_rights_and_revokes_future_charges() {
    new_test_ext().execute_with(|| {
        setup_platform();
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Platform,
            platform_plan(),
            PLATFORM_PRICE,
        ));
        let key = (subscriber_cid_number(), IssuerKey::Platform);
        let paid_until = Subscriptions::<Test>::get(&key)
            .expect("state exists")
            .paid_until;
        assert_ok!(SquarePost::cancel(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Platform,
        ));
        let state = Subscriptions::<Test>::get(&key).expect("state exists");
        assert_eq!(state.paid_until, paid_until);
        assert_eq!(state.subscription_status, SubscriptionStatus::Cancelled);
        assert!(!RenewalIndex::<Test>::contains_key(&key));
        let before = Balances::free_balance(subscriber_account_id());
        initialize_at(paid_until);
        assert_eq!(Balances::free_balance(subscriber_account_id()), before);
    });
}

#[test]
fn change_plan_upgrade_charges_difference_immediately() {
    new_test_ext().execute_with(|| {
        setup_platform();
        let now = 1_700_000_000_000u64;
        // 先订自由档（199_900）。
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Platform,
            platform_plan_for(MembershipLevel::Freedom),
            199_900,
        ));
        let balance_after_subscribe = Balances::free_balance(subscriber_account_id());
        let base_end = crate::subscription::add_calendar_period(now, BillingPeriod::Monthly)
            .expect("calendar fits");
        // 立即升档到薪火（5_999_900）；刚扣款故剩余权益=全额 199_900。
        assert_ok!(SquarePost::change_subscription_plan(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Platform,
            platform_plan(),
            PLATFORM_PRICE,
        ));
        // 升档补扣「新价 − 剩余权益」= 5_999_900 − 199_900。
        assert_eq!(
            Balances::free_balance(subscriber_account_id()),
            balance_after_subscribe - (PLATFORM_PRICE - 199_900)
        );
        let state = Subscriptions::<Test>::get((subscriber_cid_number(), IssuerKey::Platform))
            .expect("state exists");
        assert_eq!(state.plan, platform_plan());
        assert_eq!(state.price_fen, PLATFORM_PRICE);
        assert_eq!(state.subscription_status, SubscriptionStatus::Active);
        // 新周期从现在起算。
        assert_eq!(state.paid_until, base_end);
    });
}

#[test]
fn change_plan_downgrade_extends_duration() {
    new_test_ext().execute_with(|| {
        setup_platform();
        let now = 1_700_000_000_000u64;
        // 先订薪火（5_999_900）。
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Platform,
            platform_plan(),
            PLATFORM_PRICE,
        ));
        let balance_after_subscribe = Balances::free_balance(subscriber_account_id());
        let base_end = crate::subscription::add_calendar_period(now, BillingPeriod::Monthly)
            .expect("calendar fits");
        let new_price = PlatformPrice::<Test>::get(MembershipLevel::Freedom).expect("price");
        // 立即降档到自由档；剩余权益=全额 5_999_900 ＞ 新价 199_900。
        assert_ok!(SquarePost::change_subscription_plan(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Platform,
            platform_plan_for(MembershipLevel::Freedom),
            new_price,
        ));
        // 降档不扣款。
        assert_eq!(
            Balances::free_balance(subscriber_account_id()),
            balance_after_subscribe
        );
        let state = Subscriptions::<Test>::get((subscriber_cid_number(), IssuerKey::Platform))
            .expect("state exists");
        assert_eq!(state.plan, platform_plan_for(MembershipLevel::Freedom));
        assert_eq!(state.price_fen, new_price);
        // 剩余信用按新档单价折算成额外时长叠加到 base_end。
        let period_ms = u128::from(base_end - now);
        let extra_ms = (PLATFORM_PRICE - new_price) * period_ms / new_price;
        assert_eq!(state.paid_until, base_end + extra_ms as u64);
    });
}

#[test]
fn change_plan_prorates_partial_remaining_credit() {
    new_test_ext().execute_with(|| {
        setup_platform();
        let now = 1_700_000_000_000u64;
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Platform,
            platform_plan_for(MembershipLevel::Freedom),
            199_900,
        ));
        let te = crate::subscription::add_calendar_period(now, BillingPeriod::Monthly)
            .expect("calendar");
        // 推进到本期中段，剩余权益按时间比例折算。
        let mid = now + (te - now) / 3;
        set_now(mid);
        let balance_before_change = Balances::free_balance(subscriber_account_id());
        let credit = 199_900u128 * u128::from(te - mid) / u128::from(te - now);
        assert_ok!(SquarePost::change_subscription_plan(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Platform,
            platform_plan(),
            PLATFORM_PRICE,
        ));
        assert_eq!(
            Balances::free_balance(subscriber_account_id()),
            balance_before_change - (PLATFORM_PRICE - credit)
        );
        let base_end = crate::subscription::add_calendar_period(mid, BillingPeriod::Monthly)
            .expect("calendar");
        let state = Subscriptions::<Test>::get((subscriber_cid_number(), IssuerKey::Platform))
            .expect("state exists");
        assert_eq!(state.paid_until, base_end);
    });
}

#[test]
fn creator_subscription_renews_at_price_fen_when_unchanged() {
    new_test_ext().execute_with(|| {
        set_active_platform_member(creator_account_id());
        CreatorPlans::<Test>::insert(
            creator_cid_number(),
            CreatorTiers::try_from(vec![creator_tier(50)]).expect("tiers fit"),
        );
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Creator(creator_cid_number()),
            creator_plan(),
            50,
        ));
        let due = RenewalIndex::<Test>::get((
            subscriber_cid_number(),
            IssuerKey::Creator(creator_cid_number()),
        ))
        .expect("scheduled");
        let creator_before = Balances::free_balance(creator_account_id());
        // 价格未变 → 自动续扣当前授权价。
        initialize_at(due);
        assert_eq!(
            Balances::free_balance(creator_account_id()),
            creator_before + 50
        );
    });
}

#[test]
fn creator_rebind_keeps_plans_and_routes_renewal_to_current_account() {
    new_test_ext().execute_with(|| {
        set_active_platform_member(creator_account_id());
        CreatorPlans::<Test>::insert(
            creator_cid_number(),
            CreatorTiers::try_from(vec![creator_tier(50)]).expect("tiers fit"),
        );
        let key = (
            subscriber_cid_number(),
            IssuerKey::Creator(creator_cid_number()),
        );
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Creator(creator_cid_number()),
            creator_plan(),
            50,
        ));
        let due = RenewalIndex::<Test>::get(&key).expect("scheduled");
        let old_before = Balances::free_balance(creator_account_id());
        let new_before = Balances::free_balance(rebound_creator_account_id());

        bind_identity(CREATOR_CID, rebound_creator_account_id());
        initialize_at(due);

        assert_eq!(Balances::free_balance(creator_account_id()), old_before);
        assert_eq!(
            Balances::free_balance(rebound_creator_account_id()),
            new_before + 50
        );
        assert_eq!(CreatorPlans::<Test>::get(creator_cid_number()).len(), 1);
    });
}

#[test]
fn creator_cannot_subscribe_to_same_cid_after_account_rebind() {
    new_test_ext().execute_with(|| {
        setup_platform();
        bind_identity(CREATOR_CID, rebound_creator_account_id());
        assert_noop!(
            SquarePost::subscribe(
                RuntimeOrigin::signed(rebound_creator_account_id()),
                IssuerKey::Creator(creator_cid_number()),
                creator_plan(),
                50,
            ),
            Error::<Test>::CannotSubscribeSelf
        );
    });
}

#[test]
fn creator_price_change_suspends_renewal_until_reconsent() {
    new_test_ext().execute_with(|| {
        set_active_platform_member(creator_account_id());
        let ck = (
            subscriber_cid_number(),
            IssuerKey::Creator(creator_cid_number()),
        );
        CreatorPlans::<Test>::insert(
            creator_cid_number(),
            CreatorTiers::try_from(vec![creator_tier(50)]).expect("tiers fit"),
        );
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Creator(creator_cid_number()),
            creator_plan(),
            50,
        ));
        assert_eq!(
            Subscriptions::<Test>::get(&ck)
                .expect("state exists")
                .price_fen,
            50
        );
        // 创作者升价 50 → 75。
        CreatorPlans::<Test>::insert(
            creator_cid_number(),
            CreatorTiers::try_from(vec![creator_tier(75)]).expect("tiers fit"),
        );
        let due = RenewalIndex::<Test>::get(&ck).expect("scheduled");
        let creator_before = Balances::free_balance(creator_account_id());
        initialize_at(due);
        // 续费挂起、创作者未收款、离调度。
        let state = Subscriptions::<Test>::get(&ck).expect("state exists");
        assert_eq!(state.subscription_status, SubscriptionStatus::Suspended);
        assert_eq!(
            state.suspend_reason,
            Some(crate::SuspendReason::NeedReconsent)
        );
        assert_eq!(state.price_fen, 50);
        assert_eq!(Balances::free_balance(creator_account_id()), creator_before);
        assert!(!RenewalIndex::<Test>::contains_key(&ck));
        // 订阅者按新价再签名 → 恢复 Active 并扣新价。
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Creator(creator_cid_number()),
            creator_plan(),
            75,
        ));
        let resumed = Subscriptions::<Test>::get(&ck).expect("state exists");
        assert_eq!(resumed.subscription_status, SubscriptionStatus::Active);
        assert_eq!(resumed.price_fen, 75);
        assert_eq!(
            Balances::free_balance(creator_account_id()),
            creator_before + 75
        );
        assert!(RenewalIndex::<Test>::contains_key(&ck));
    });
}

#[test]
fn creator_price_change_reconsent_before_lapse_keeps_active_without_charge() {
    new_test_ext().execute_with(|| {
        set_active_platform_member(creator_account_id());
        let ck = (
            subscriber_cid_number(),
            IssuerKey::Creator(creator_cid_number()),
        );
        CreatorPlans::<Test>::insert(
            creator_cid_number(),
            CreatorTiers::try_from(vec![creator_tier(50)]).expect("tiers fit"),
        );
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Creator(creator_cid_number()),
            creator_plan(),
            50,
        ));
        let due = RenewalIndex::<Test>::get(&ck).expect("scheduled");
        // 创作者升价 50 → 75；订阅者到期前再签名。
        CreatorPlans::<Test>::insert(
            creator_cid_number(),
            CreatorTiers::try_from(vec![creator_tier(75)]).expect("tiers fit"),
        );
        let creator_before = Balances::free_balance(creator_account_id());
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Creator(creator_cid_number()),
            creator_plan(),
            75,
        ));
        // 保持 Active、不即时扣款、已授权价更新。
        let state = Subscriptions::<Test>::get(&ck).expect("state exists");
        assert_eq!(state.subscription_status, SubscriptionStatus::Active);
        assert_eq!(state.price_fen, 75);
        assert_eq!(Balances::free_balance(creator_account_id()), creator_before);
        // 下期按新价自动扣。
        initialize_at(due);
        assert_eq!(
            Balances::free_balance(creator_account_id()),
            creator_before + 75
        );
    });
}

#[test]
fn creator_loses_membership_pauses_fans_and_resumes() {
    new_test_ext().execute_with(|| {
        set_active_platform_member(creator_account_id());
        let ck = (
            subscriber_cid_number(),
            IssuerKey::Creator(creator_cid_number()),
        );
        CreatorPlans::<Test>::insert(
            creator_cid_number(),
            CreatorTiers::try_from(vec![creator_tier(50)]).expect("tiers fit"),
        );
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Creator(creator_cid_number()),
            creator_plan(),
            50,
        ));
        let due = RenewalIndex::<Test>::get(&ck).expect("scheduled");
        // 创作者掉平台会员。
        Subscriptions::<Test>::remove((creator_cid_number(), IssuerKey::Platform));
        let creator_before = Balances::free_balance(creator_account_id());
        initialize_at(due);
        // 粉丝暂停：IssuerPaused、未扣、未终止、仍在调度、下周期重试。
        let state = Subscriptions::<Test>::get(&ck).expect("state exists");
        assert_eq!(state.subscription_status, SubscriptionStatus::IssuerPaused);
        assert_eq!(Balances::free_balance(creator_account_id()), creator_before);
        let retry = RenewalIndex::<Test>::get(&ck).expect("still scheduled");
        assert!(retry > due);
        // 创作者恢复平台会员 → 下个重试自动续扣、回 Active。
        set_active_platform_member(creator_account_id());
        initialize_at(retry);
        let resumed = Subscriptions::<Test>::get(&ck).expect("state exists");
        assert_eq!(resumed.subscription_status, SubscriptionStatus::Active);
        assert_eq!(
            Balances::free_balance(creator_account_id()),
            creator_before + 50
        );
    });
}

#[test]
fn creator_paused_and_repriced_suspends_for_reconsent_on_return() {
    new_test_ext().execute_with(|| {
        set_active_platform_member(creator_account_id());
        let ck = (
            subscriber_cid_number(),
            IssuerKey::Creator(creator_cid_number()),
        );
        CreatorPlans::<Test>::insert(
            creator_cid_number(),
            CreatorTiers::try_from(vec![creator_tier(50)]).expect("tiers fit"),
        );
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Creator(creator_cid_number()),
            creator_plan(),
            50,
        ));
        let due = RenewalIndex::<Test>::get(&ck).expect("scheduled");
        Subscriptions::<Test>::remove((creator_cid_number(), IssuerKey::Platform));
        initialize_at(due);
        let retry = RenewalIndex::<Test>::get(&ck).expect("still scheduled");
        // 创作者恢复会员但同时改了价 → 恢复时先挂起待再签名。
        set_active_platform_member(creator_account_id());
        CreatorPlans::<Test>::insert(
            creator_cid_number(),
            CreatorTiers::try_from(vec![creator_tier(75)]).expect("tiers fit"),
        );
        initialize_at(retry);
        let state = Subscriptions::<Test>::get(&ck).expect("state exists");
        assert_eq!(state.subscription_status, SubscriptionStatus::Suspended);
        assert_eq!(
            state.suspend_reason,
            Some(crate::SuspendReason::NeedReconsent)
        );
        assert!(!RenewalIndex::<Test>::contains_key(&ck));
    });
}

#[test]
fn only_effective_platform_member_can_publish_creator_plans() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            SquarePost::set_creator_plans(
                RuntimeOrigin::signed(creator_account_id()),
                vec![creator_tier_input(50, "支持者")],
            ),
            Error::<Test>::CreatorNotPlatformMember
        );
        set_active_platform_member(creator_account_id());
        assert_ok!(SquarePost::set_creator_plans(
            RuntimeOrigin::signed(creator_account_id()),
            vec![creator_tier_input(50, "支持者")],
        ));
        assert_eq!(CreatorPlans::<Test>::get(creator_cid_number()).len(), 1);
        assert_eq!(
            CreatorTierNames::<Test>::get(
                creator_cid_number(),
                TierId::try_from(b"supporter".to_vec()).expect("tier id fits"),
            ),
            Some(tier_name("支持者")),
        );
    });
}

#[test]
fn tier_name_is_chain_truth_and_rename_does_not_change_financial_plan() {
    new_test_ext().execute_with(|| {
        set_active_platform_member(creator_account_id());
        assert_ok!(SquarePost::set_creator_plans(
            RuntimeOrigin::signed(creator_account_id()),
            vec![creator_tier_input(50, "支持者")],
        ));
        let before = CreatorPlans::<Test>::get(creator_cid_number());
        let tier_id = TierId::try_from(b"supporter".to_vec()).expect("tier id fits");

        assert_ok!(SquarePost::update_creator_tier_name(
            RuntimeOrigin::signed(creator_account_id()),
            tier_id.clone(),
            tier_name("核心支持者"),
        ));

        assert_eq!(CreatorPlans::<Test>::get(creator_cid_number()), before);
        assert_eq!(
            CreatorTierNames::<Test>::get(creator_cid_number(), tier_id),
            Some(tier_name("核心支持者")),
        );
        assert!(matches!(
            System::events().last().map(|record| &record.event),
            Some(RuntimeEvent::SquarePost(
                Event::CreatorTierNameUpdated { .. }
            ))
        ));
    });
}

#[test]
fn tier_name_rejects_invalid_utf8_whitespace_controls_and_more_than_twenty_scalars() {
    new_test_ext().execute_with(|| {
        set_active_platform_member(creator_account_id());
        let invalid_names = [
            TierName::try_from(vec![0xff]).expect("one byte fits"),
            tier_name(" 前导空格"),
            tier_name("尾随空格 "),
            tier_name("换\n行"),
        ];
        for invalid_name in invalid_names {
            let mut input = creator_tier_input(50, "支持者");
            input.tier_name = invalid_name;
            assert_noop!(
                SquarePost::set_creator_plans(
                    RuntimeOrigin::signed(creator_account_id()),
                    vec![input],
                ),
                Error::<Test>::InvalidTierName
            );
        }
        let mut too_long = creator_tier_input(50, "支持者");
        too_long.tier_name = tier_name("二十一二十一二十一二十一二十一二十一二十一");
        assert_noop!(
            SquarePost::set_creator_plans(
                RuntimeOrigin::signed(creator_account_id()),
                vec![too_long],
            ),
            Error::<Test>::TierNameTooLong
        );
    });
}

#[test]
fn tier_name_accepts_exactly_twenty_four_byte_unicode_scalars() {
    new_test_ext().execute_with(|| {
        set_active_platform_member(creator_account_id());
        let boundary = "🙂".repeat(20);
        assert_eq!(boundary.len(), 80);
        assert_ok!(SquarePost::set_creator_plans(
            RuntimeOrigin::signed(creator_account_id()),
            vec![creator_tier_input(50, &boundary)],
        ));
    });
}

#[test]
fn scale_fixture_matches_target_contract() {
    use codec::Encode;
    use std::collections::BTreeSet;
    let platform: IssuerKeyOf<Test> = IssuerKey::Platform;
    let creator = IssuerKey::Creator(creator_cid_number());
    let state = SubscriptionState {
        plan: platform_plan(),
        started_at: 1_700_000_000_000,
        last_charged_at: 1_700_000_000_000,
        charge_fen: PLATFORM_PRICE,
        paid_until: 1_702_000_000_000,
        subscription_status: SubscriptionStatus::Active,
        price_fen: PLATFORM_PRICE,
        suspend_reason: None,
    };
    let actual: Vec<(&str, Vec<u8>)> = vec![
        ("billing_period_monthly", BillingPeriod::Monthly.encode()),
        (
            "billing_period_quarterly",
            BillingPeriod::Quarterly.encode(),
        ),
        ("billing_period_yearly", BillingPeriod::Yearly.encode()),
        ("issuer_platform", platform.encode()),
        ("issuer_creator", creator.encode()),
        ("plan_platform_spark", platform_plan().encode()),
        ("plan_creator_supporter_monthly", creator_plan().encode()),
        ("status_active", SubscriptionStatus::Active.encode()),
        ("status_cancelled", SubscriptionStatus::Cancelled.encode()),
        ("status_terminated", SubscriptionStatus::Terminated.encode()),
        ("status_suspended", SubscriptionStatus::Suspended.encode()),
        (
            "status_issuer_paused",
            SubscriptionStatus::IssuerPaused.encode(),
        ),
        (
            "suspend_reason_need_reconsent",
            SuspendReason::NeedReconsent.encode(),
        ),
        (
            "suspend_reason_insufficient_balance",
            SuspendReason::InsufficientBalance.encode(),
        ),
        (
            "suspend_reason_identity_binding_unavailable",
            SuspendReason::IdentityBindingUnavailable.encode(),
        ),
        ("state_platform_spark_active", state.encode()),
        (
            "call_subscribe_platform_spark",
            crate::Call::<Test>::subscribe {
                issuer: platform.clone(),
                plan: platform_plan(),
                expected_price_fen: PLATFORM_PRICE,
            }
            .encode(),
        ),
        (
            "call_cancel_platform",
            crate::Call::<Test>::cancel { issuer: platform }.encode(),
        ),
    ];
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("subscription_scale_vectors.json"))
            .expect("固定金标JSON");
    let vectors = fixture["vectors"].as_array().expect("固定向量集合");
    let names: BTreeSet<_> = vectors
        .iter()
        .map(|v| v["name"].as_str().expect("向量名称"))
        .collect();
    assert_eq!(vectors.len(), 19);
    assert_eq!(names.len(), vectors.len(), "禁止重复向量名称");
    let covered: BTreeSet<_> = actual.iter().map(|(name, _)| *name).collect();
    assert_eq!(covered.len(), 18);
    assert_eq!(
        names.difference(&covered).copied().collect::<Vec<_>>(),
        vec!["calldata_subscribe_platform_spark"]
    );
    // 含pallet前缀的最后一条由主Runtime实际RuntimeCall测试核验，不能用mock索引代替。
    for (name, bytes) in actual {
        let item = vectors
            .iter()
            .find(|v| v["name"] == name)
            .expect("每项真实类型都有固定金标");
        let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(item["hex"].as_str().expect("固定编码"), hex, "向量 {name}");
    }
}

#[test]
fn subscription_prices_preserve_distinct_full_u128_scale_values() {
    use codec::{Decode, Encode};
    let mut state = SubscriptionState {
        plan: platform_plan(),
        started_at: 1,
        last_charged_at: 2,
        charge_fen: 127,
        paid_until: 3,
        subscription_status: SubscriptionStatus::Suspended,
        price_fen: 211,
        suspend_reason: Some(SuspendReason::NeedReconsent),
    };
    // 独立拼接原有顺序，确保字段改名不改变已有状态的 SCALE 字节。
    for (charge_fen, price_fen) in [(127, 211), (0, u128::MAX), (u128::MAX, 0)] {
        state.charge_fen = charge_fen;
        state.price_fen = price_fen;
        let expected = (
            state.plan.clone(),
            state.started_at,
            state.last_charged_at,
            charge_fen,
            state.paid_until,
            state.subscription_status,
            price_fen,
            state.suspend_reason,
        )
            .encode();
        assert_eq!(state.encode(), expected);
        let mut input = expected.as_slice();
        assert_eq!(
            SubscriptionState::decode(&mut input).expect("完整 u128 状态"),
            state
        );
        assert!(input.is_empty());
        assert!(SubscriptionState::decode(&mut &expected[..expected.len() - 1]).is_err());
    }
}

#[test]
fn genesis_build_seeds_default_platform_prices() {
    use frame_support::traits::BuildGenesisConfig;
    new_test_ext().execute_with(|| {
        crate::pallet::GenesisConfig::<Test>::default().build();
        assert_eq!(
            PlatformPrice::<Test>::get(MembershipLevel::Freedom),
            Some(crate::pallet::FREEDOM_PRICE_FEN)
        );
        assert_eq!(
            PlatformPrice::<Test>::get(MembershipLevel::Democracy),
            Some(crate::pallet::DEMOCRACY_PRICE_FEN)
        );
        assert_eq!(
            PlatformPrice::<Test>::get(MembershipLevel::Spark),
            Some(crate::pallet::SPARK_PRICE_FEN)
        );
    });
}

// 平台调价治理：可测的前置校验（零价 / 非基金会）。完整「投票通过→执行→改价」
// 路径需 votebase 集成夹具，列为跟进项。
#[test]
fn propose_platform_price_rejects_zero_price() {
    new_test_ext().execute_with(|| {
        let cid = votebase::types::CidNumber::try_from(
            primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION
                .cid_number
                .as_bytes()
                .to_vec(),
        )
        .expect("cid fits");
        assert_noop!(
            SquarePost::propose_set_platform_price(
                RuntimeOrigin::signed(verified_account()),
                cid,
                primitives::cid::china::citizenchain::ROLE_CODE_GENESIS_PRODUCT_MANAGER
                    .to_vec()
                    .try_into()
                    .expect("product manager role fits"),
                MembershipLevel::Spark,
                0,
            ),
            Error::<Test>::InvalidPlatformPrice
        );
    });
}

#[test]
fn propose_platform_price_rejects_non_foundation_institution() {
    new_test_ext().execute_with(|| {
        let cid = votebase::types::CidNumber::try_from(b"GD001-CTZN1-000000001-2026".to_vec())
            .expect("cid fits");
        assert_noop!(
            SquarePost::propose_set_platform_price(
                RuntimeOrigin::signed(verified_account()),
                cid,
                primitives::cid::china::citizenchain::ROLE_CODE_GENESIS_PRODUCT_MANAGER
                    .to_vec()
                    .try_into()
                    .expect("product manager role fits"),
                MembershipLevel::Spark,
                599_900,
            ),
            Error::<Test>::NotPlatformInstitution
        );
    });
}

/// 平台侧价格暂时缺失只暂停不终止，平台恢复后自动续费，不需订阅者重新签名。
#[test]
fn missing_platform_price_pauses_instead_of_terminating() {
    new_test_ext().execute_with(|| {
        setup_platform();
        let key = (subscriber_cid_number(), IssuerKey::Platform);
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Platform,
            platform_plan(),
            PLATFORM_PRICE,
        ));
        let due = RenewalIndex::<Test>::get(&key).expect("scheduled");

        // 平台三档价被清空（模拟播种缺失/治理写入异常）。
        PlatformPrice::<Test>::remove(MembershipLevel::Spark);
        initialize_at(due);

        // 必须是可恢复的暂停，且仍留在调度里等待重试。
        let state = Subscriptions::<Test>::get(&key).expect("state exists");
        assert_eq!(state.subscription_status, SubscriptionStatus::IssuerPaused);
        assert_ne!(state.subscription_status, SubscriptionStatus::Terminated);
        let retry = RenewalIndex::<Test>::get(&key).expect("still scheduled");
        assert!(retry > due);

        // 平台价恢复 → 下个重试自动续扣回 Active，订阅者全程无需操作。
        PlatformPrice::<Test>::insert(MembershipLevel::Spark, PLATFORM_PRICE);
        initialize_at(retry);
        let resumed = Subscriptions::<Test>::get(&key).expect("state exists");
        assert_eq!(resumed.subscription_status, SubscriptionStatus::Active);
    });
}

/// 换档折算按本期计价，不按已授权价：创作者涨价 + 到期前再签名的窗口内两者不等。
#[test]
fn plan_change_credit_uses_charge_fen() {
    new_test_ext().execute_with(|| {
        set_active_platform_member(creator_account_id());
        let key = (
            subscriber_cid_number(),
            IssuerKey::Creator(creator_cid_number()),
        );
        CreatorPlans::<Test>::insert(
            creator_cid_number(),
            CreatorTiers::try_from(vec![creator_tier(50)]).expect("tiers fit"),
        );
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Creator(creator_cid_number()),
            creator_plan(),
            50,
        ));

        // 创作者涨价到 100，订阅者到期前再签名：只更新授权价，本期计价仍是 50。
        CreatorPlans::<Test>::insert(
            creator_cid_number(),
            CreatorTiers::try_from(vec![creator_tier(100)]).expect("tiers fit"),
        );
        assert_ok!(SquarePost::subscribe(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Creator(creator_cid_number()),
            creator_plan(),
            100,
        ));
        let state = Subscriptions::<Test>::get(&key).expect("state exists");
        assert_eq!(state.price_fen, 100, "已授权价已是新价");
        assert_eq!(state.charge_fen, 50, "本期计价仍是旧价");

        // 立即换档：整周期刚开始，剩余信用应≈本期计价 50，而非授权价 100。
        // 目标档 100 分，补扣差额应接近 50；若误用授权价则信用 100、补扣为 0。
        let before = Balances::free_balance(subscriber_account_id());
        assert_ok!(SquarePost::change_subscription_plan(
            RuntimeOrigin::signed(subscriber_account_id()),
            IssuerKey::Creator(creator_cid_number()),
            creator_plan(),
            100,
        ));
        let charged = before - Balances::free_balance(subscriber_account_id());
        assert!(
            charged > 0,
            "必须按本期计价折算并补扣差额，不得因误用授权价而零扣款"
        );
    });
}
