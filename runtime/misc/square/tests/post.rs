//! 复用本模块真实测试环境的行为与边界回归。
use super::*;

#[test]
fn post_derives_category_and_storage_until_from_execution_state() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            SquarePost::publish_post(
                RuntimeOrigin::signed(account(4)),
                b"sqp_unbound_001".to_vec(),
                SquarePostType::Document,
                [9u8; 32],
                b"receipt".to_vec(),
            ),
            Error::<Test>::CitizenIdentityUnavailable
        );
        assert_noop!(
            SquarePost::publish_post(
                RuntimeOrigin::signed(visitor_account()),
                b"sqp_no_membership".to_vec(),
                SquarePostType::Document,
                [9u8; 32],
                b"receipt".to_vec(),
            ),
            Error::<Test>::PlatformMembershipRequired
        );
        set_active_platform_member(visitor_account());
        assert_ok!(SquarePost::publish_post(
            RuntimeOrigin::signed(visitor_account()),
            b"sqp_normal_001".to_vec(),
            SquarePostType::Article,
            [9u8; 32],
            b"receipt".to_vec(),
        ));
        let post_id = crate::pallet::PostIdOf::<Test>::try_from(b"sqp_normal_001".to_vec())
            .expect("post id fits");
        let post = crate::pallet::SquarePosts::<Test>::get(post_id).expect("post exists");
        assert_eq!(post.cid_number, bounded_cid_number(VISITOR_CID));
        assert_eq!(post.signer_account_id, visitor_account());
        assert_eq!(post.post_type, SquarePostType::Article);
        assert_eq!(post.post_category, SquarePostCategory::Normal);
        assert_eq!(post.storage_until, 1_800_000_000_000);
        assert_eq!(
            crate::pallet::PublishedPostCountByCid::<Test>::get(bounded_cid_number(VISITOR_CID)),
            1
        );
        set_active_platform_member(verified_account());
        assert_ok!(SquarePost::publish_post(
            RuntimeOrigin::signed(verified_account()),
            b"sqp_campaign_002".to_vec(),
            SquarePostType::Video,
            [9u8; 32],
            b"receipt".to_vec(),
        ));
        let campaign_post_id =
            crate::pallet::PostIdOf::<Test>::try_from(b"sqp_campaign_002".to_vec())
                .expect("post id fits");
        let campaign_post =
            crate::pallet::SquarePosts::<Test>::get(campaign_post_id).expect("post exists");
        assert_eq!(campaign_post.post_type, SquarePostType::Video);
        assert_eq!(campaign_post.post_category, SquarePostCategory::Campaign);
        assert!(matches!(
            System::events().last().map(|record| &record.event),
            Some(RuntimeEvent::SquarePost(Event::SquarePostPublished {
                post_type: SquarePostType::Video,
                post_category: SquarePostCategory::Campaign,
                storage_until: 1_800_000_000_000,
                ..
            }))
        ));
    });
}

#[test]
fn cancelled_platform_membership_keeps_paid_period_publish_rights() {
    new_test_ext().execute_with(|| {
        set_active_platform_member(visitor_account());
        let key = (bounded_cid_number(VISITOR_CID), IssuerKey::Platform);
        Subscriptions::<Test>::mutate(&key, |state| {
            state
                .as_mut()
                .expect("membership exists")
                .subscription_status = SubscriptionStatus::Cancelled;
        });
        assert_ok!(SquarePost::publish_post(
            RuntimeOrigin::signed(visitor_account()),
            b"sqp_cancelled_paid".to_vec(),
            SquarePostType::Document,
            [8u8; 32],
            b"receipt".to_vec(),
        ));

        set_now(1_800_000_000_000);
        assert_noop!(
            SquarePost::publish_post(
                RuntimeOrigin::signed(visitor_account()),
                b"sqp_cancelled_expired".to_vec(),
                SquarePostType::Document,
                [8u8; 32],
                b"receipt".to_vec(),
            ),
            Error::<Test>::PlatformMembershipRequired
        );
    });
}
