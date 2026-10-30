//! 损坏或未登记的执行请求不得绕过投票终态修改管理员。
use super::*;
use frame_support::assert_noop;

#[test]
fn unregistered_execution_action_cannot_mutate_roster_or_threshold() {
    new_test_ext().execute_with(|| {
        let account = personal_account_id();
        seed_active_admin_account(account.clone(), admins(&[admin(0), admin(1), admin(2)]), 2);
        let action = adminbase::AdminSetChangeAction {
            personal_account_id: account.clone(),
            admins: admins(&[admin(0), admin(3), admin(4)]),
            new_threshold: 2,
        };
        assert_noop!(
            PersonalAdmins::try_execute_set_change_from_action(999, action),
            Error::<Test>::ProposalActionNotFound
        );
        assert_eq!(
            internalvote::ActivePersonalThresholds::<Test>::get(account),
            Some(2)
        );
    });
}
