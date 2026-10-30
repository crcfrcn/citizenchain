//! 个人管理员变更的拒绝场景必须在提案、阈值、名册和事件上无副作用。
use super::*;
use frame_support::assert_noop;

#[test]
fn invalid_thresholds_duplicate_admins_and_small_sets_are_atomic() {
    new_test_ext().execute_with(|| {
        let account = personal_account_id();
        seed_active_admin_account(account.clone(), admins(&[admin(0), admin(1), admin(2)]), 2);
        for threshold in [0, 1, 4] {
            assert_noop!(
                PersonalAdmins::propose_admin_set_change(
                    RuntimeOrigin::signed(admin(0)),
                    PMUL,
                    account.clone(),
                    admins(&[admin(0), admin(3), admin(4)]),
                    threshold
                ),
                Error::<Test>::InvalidThreshold
            );
        }
        assert_noop!(
            PersonalAdmins::propose_admin_set_change(
                RuntimeOrigin::signed(admin(0)),
                PMUL,
                account.clone(),
                admins(&[admin(0), admin(0), admin(3)]),
                2
            ),
            Error::<Test>::DuplicateAdmin
        );
        for list in [admins(&[]), admins(&[admin(0)])] {
            assert_noop!(
                PersonalAdmins::propose_admin_set_change(
                    RuntimeOrigin::signed(admin(0)),
                    PMUL,
                    account.clone(),
                    list,
                    1
                ),
                Error::<Test>::InvalidAdminsLen
            );
        }
    });
}

#[test]
fn outsiders_wrong_kind_missing_account_and_reordered_same_set_are_rejected() {
    new_test_ext().execute_with(|| {
        let account = personal_account_id();
        seed_active_admin_account(account.clone(), admins(&[admin(0), admin(1), admin(2)]), 2);
        let next = admins(&[admin(0), admin(3), admin(4)]);
        assert_noop!(
            PersonalAdmins::propose_admin_set_change(
                RuntimeOrigin::signed(admin(9)),
                PMUL,
                account.clone(),
                next.clone(),
                2
            ),
            Error::<Test>::PermissionDenied
        );
        assert_noop!(
            PersonalAdmins::propose_admin_set_change(
                RuntimeOrigin::signed(admin(0)),
                *b"NRCG",
                account.clone(),
                next.clone(),
                2
            ),
            Error::<Test>::NotPersonalAccount
        );
        assert_noop!(
            PersonalAdmins::propose_admin_set_change(
                RuntimeOrigin::signed(admin(0)),
                PMUL,
                admin(30),
                next,
                2
            ),
            Error::<Test>::PersonalNotFound
        );
        assert_noop!(
            PersonalAdmins::propose_admin_set_change(
                RuntimeOrigin::signed(admin(0)),
                PMUL,
                account,
                admins(&[admin(2), admin(0), admin(1)]),
                2
            ),
            Error::<Test>::AdminSetUnchanged
        );
    });
}
