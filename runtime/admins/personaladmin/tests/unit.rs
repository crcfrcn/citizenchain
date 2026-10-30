//! 原有行为回归完整迁入，复用所属包测试夹具。
use super::*;

#[test]
fn propose_admin_set_change_updates_personal_admins_and_threshold() {
    new_test_ext().execute_with(|| {
        let account = personal_account_id();
        let old_admins = admins(&[admin(0), admin(1), admin(2)]);
        seed_active_admin_account(account.clone(), old_admins.clone(), 2);

        let mut next_admins = admins(&[admin(0), admin(3), admin(4), admin(5)]);
        next_admins[1].family_name = Default::default();
        next_admins[1].given_name = Default::default();
        let mut expected_admins = next_admins.clone();
        expected_admins[1].family_name =
            adminbase::FamilyName::truncate_from("管理".as_bytes().to_vec());
        expected_admins[1].given_name =
            adminbase::GivenName::truncate_from("员".as_bytes().to_vec());
        assert_ok!(PersonalAdmins::propose_admin_set_change(
            RuntimeOrigin::signed(admin(0)),
            PMUL,
            account.clone(),
            next_admins.clone(),
            3,
        ));
        let proposal_id = votebase::Pallet::<Test>::next_proposal_id().saturating_sub(1);

        // 提案创建时发起人 admin(0) 已自动赞成，阈值 2 时只需再补一票。
        assert_ok!(internalvote::Pallet::<Test>::do_internal_vote(
            admin(1),
            proposal_id,
            internalvote::InternalVoteTicketClaim::Personal,
            true
        ));
        // 通过判定只入队；管理员集合更新由维护管线异步执行。
        <VotingEngine as Hooks<u64>>::on_initialize(System::block_number());

        let account_id = pallet::AdminAccounts::<Test>::get(account.clone())
            .expect("admin account should exist");
        assert!(account_id.cid_number.is_empty());
        assert_eq!(account_id.admins, expected_admins);
        assert_eq!(
            internalvote::ActivePersonalThresholds::<Test>::get(account),
            Some(3)
        );
    });
}
