//! 真实内部投票通过只排队，管理员与阈值由后续引擎执行管线统一更新。
use super::*;

#[test]
fn approved_change_waits_for_execution_and_cannot_execute_twice() {
    new_test_ext().execute_with(|| {
        let account = personal_account_id();
        let before = admins(&[admin(0), admin(1), admin(2)]);
        seed_active_admin_account(account.clone(), before.clone(), 2);
        let next = admins(&[admin(0), admin(3), admin(4), admin(5)]);
        assert_ok!(PersonalAdmins::propose_admin_set_change(
            RuntimeOrigin::signed(admin(0)),
            PMUL,
            account.clone(),
            next.clone(),
            3
        ));
        let id = VotingEngine::next_proposal_id() - 1;
        assert_ok!(internalvote::Pallet::<Test>::do_internal_vote(
            admin(1),
            id,
            internalvote::InternalVoteTicketClaim::Personal,
            true
        ));
        assert_eq!(AdminAccounts::<Test>::get(&account).unwrap().admins, before);
        assert_eq!(
            internalvote::ActivePersonalThresholds::<Test>::get(&account),
            Some(2)
        );
        <VotingEngine as Hooks<u64>>::on_initialize(System::block_number());
        assert_eq!(AdminAccounts::<Test>::get(&account).unwrap().admins, next);
        assert_eq!(
            internalvote::ActivePersonalThresholds::<Test>::get(&account),
            Some(3)
        );
        let state = sp_io::storage::root(sp_runtime::StateVersion::V1);
        <VotingEngine as Hooks<u64>>::on_initialize(System::block_number());
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), state);
    });
}
