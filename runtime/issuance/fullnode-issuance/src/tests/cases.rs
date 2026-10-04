use super::*;

#[test]
fn bind_reward_account_only_once() {
    new_test_ext().execute_with(|| {
        let miner = account(1);
        let reward_account_id = account(2);
        let reward_account_id_2 = account(3);

        assert_ok!(FullnodeIssuance::bind_reward_account(
            RuntimeOrigin::signed(miner.clone()),
            reward_account_id.clone()
        ));
        assert_eq!(
            RewardAccountIdByMiner::<Test>::get(&miner),
            Some(reward_account_id)
        );

        assert_noop!(
            FullnodeIssuance::bind_reward_account(
                RuntimeOrigin::signed(miner),
                reward_account_id_2
            ),
            Error::<Test>::RewardAccountAlreadyBound
        );
    });
}

#[test]
fn bind_rejects_miner_reward_account() {
    new_test_ext().execute_with(|| {
        let miner = account(4);
        assert_noop!(
            FullnodeIssuance::bind_reward_account(RuntimeOrigin::signed(miner.clone()), miner),
            Error::<Test>::RewardAccountCannotBeMiner
        );
    });
}

/// 预绑定:未出过块的矿工账户也能绑定奖励接收账户。
///
/// 绑定表只在出块时被 `get(&author)` 读取,未出块账户的登记读不到、不产生任何效果,
/// 故不以出块记录作为门槛;否则新装节点在抢到首个 PoW 区块之前无法配置收款账户,
/// 且首块的全节点手续费分成会按 `RewardAccountUnbound` 销毁。
#[test]
fn bind_succeeds_for_miner_that_never_authored() {
    new_test_ext().execute_with(|| {
        let miner = account(5);
        let reward_account_id = account(6);

        assert_ok!(FullnodeIssuance::bind_reward_account(
            RuntimeOrigin::signed(miner.clone()),
            reward_account_id.clone()
        ));
        assert_eq!(
            RewardAccountIdByMiner::<Test>::get(&miner),
            Some(reward_account_id),
            "未出块也应完成绑定"
        );
    });
}

#[test]
fn reward_issued_within_range_when_bound() {
    new_test_ext().execute_with(|| {
        let miner = account(11);
        let reward_account_id = account(22);
        assert_ok!(FullnodeIssuance::bind_reward_account(
            RuntimeOrigin::signed(miner.clone()),
            reward_account_id.clone()
        ));
        MOCK_AUTHOR.with(|v| *v.borrow_mut() = Some(miner.clone()));

        // 起始边界块 1 应发放奖励
        <FullnodeIssuance as Hooks<u32>>::on_finalize(1);
        assert_eq!(
            Balances::free_balance(reward_account_id.clone()),
            primitives::pow_const::FULLNODE_BLOCK_REWARD
        );
        assert_eq!(RewardedBlockCount::<Test>::get(), 1);
        assert_eq!(
            TotalFullnodeIssued::<Test>::get(),
            primitives::pow_const::FULLNODE_BLOCK_REWARD
        );
        assert_eq!(
            LastRewardAudit::<Test>::get(),
            Some((
                1,
                miner.clone(),
                reward_account_id.clone(),
                primitives::pow_const::FULLNODE_BLOCK_REWARD,
            ))
        );

        let has_event = System::events().iter().any(|r| {
            matches!(
                r.event,
                RuntimeEvent::FullnodeIssuance(Event::FullnodeIssuanceIssued { block: 1, .. })
            )
        });
        assert!(has_event);
    });
}

#[test]
fn reward_to_miner_when_not_bound() {
    new_test_ext().execute_with(|| {
        let miner = account(33);
        MOCK_AUTHOR.with(|v| *v.borrow_mut() = Some(miner.clone()));

        <FullnodeIssuance as Hooks<u32>>::on_finalize(1);
        // 未绑定账户时，奖励默认发到矿工自身账户
        assert_eq!(
            Balances::free_balance(miner),
            primitives::pow_const::FULLNODE_BLOCK_REWARD
        );
    });
}

#[test]
fn no_reward_outside_reward_range() {
    new_test_ext().execute_with(|| {
        let miner = account(55);
        let reward_account_id = account(66);
        assert_ok!(FullnodeIssuance::bind_reward_account(
            RuntimeOrigin::signed(miner.clone()),
            reward_account_id.clone()
        ));
        MOCK_AUTHOR.with(|v| *v.borrow_mut() = Some(miner));

        // 区块 0 不发放
        <FullnodeIssuance as Hooks<u32>>::on_finalize(0);
        assert_eq!(Balances::free_balance(reward_account_id.clone()), 0);
        assert_eq!(RewardedBlockCount::<Test>::get(), 0);
        assert_eq!(TotalFullnodeIssued::<Test>::get(), 0);
        assert_eq!(LastRewardAudit::<Test>::get(), None);
        let has_event_block_0 = System::events().iter().any(|r| {
            matches!(
                r.event,
                RuntimeEvent::FullnodeIssuance(Event::FullnodeIssuanceSkippedNoAuthor { block: 0 })
            )
        });
        assert!(!has_event_block_0);

        // 超出结束高度不发放
        <FullnodeIssuance as Hooks<u32>>::on_finalize(
            primitives::pow_const::FULLNODE_REWARD_END_BLOCK + 1,
        );
        assert_eq!(Balances::free_balance(reward_account_id), 0);
        assert_eq!(RewardedBlockCount::<Test>::get(), 0);
        assert_eq!(TotalFullnodeIssued::<Test>::get(), 0);
    });
}

#[test]
fn reward_issued_on_end_boundary_block() {
    new_test_ext().execute_with(|| {
        let miner = account(77);
        let reward_account_id = account(88);
        assert_ok!(FullnodeIssuance::bind_reward_account(
            RuntimeOrigin::signed(miner.clone()),
            reward_account_id.clone()
        ));
        MOCK_AUTHOR.with(|v| *v.borrow_mut() = Some(miner));

        let end = primitives::pow_const::FULLNODE_REWARD_END_BLOCK;
        <FullnodeIssuance as Hooks<u32>>::on_finalize(end);

        assert_eq!(
            Balances::free_balance(reward_account_id.clone()),
            primitives::pow_const::FULLNODE_BLOCK_REWARD
        );
        let has_event = System::events().iter().any(|r| {
            matches!(
                r.event,
                RuntimeEvent::FullnodeIssuance(Event::FullnodeIssuanceIssued { block, .. })
                if block == primitives::pow_const::FULLNODE_REWARD_END_BLOCK
            )
        });
        assert!(has_event);
    });
}

#[test]
fn reward_accumulates_across_multiple_blocks() {
    new_test_ext().execute_with(|| {
        let miner = account(91);
        let reward_account_id = account(92);
        assert_ok!(FullnodeIssuance::bind_reward_account(
            RuntimeOrigin::signed(miner.clone()),
            reward_account_id.clone()
        ));
        MOCK_AUTHOR.with(|v| *v.borrow_mut() = Some(miner));

        <FullnodeIssuance as Hooks<u32>>::on_finalize(1);
        <FullnodeIssuance as Hooks<u32>>::on_finalize(2);
        <FullnodeIssuance as Hooks<u32>>::on_finalize(3);

        assert_eq!(
            Balances::free_balance(reward_account_id),
            primitives::pow_const::FULLNODE_BLOCK_REWARD * 3
        );
        assert_eq!(RewardedBlockCount::<Test>::get(), 3);
        assert_eq!(
            TotalFullnodeIssued::<Test>::get(),
            primitives::pow_const::FULLNODE_BLOCK_REWARD * 3
        );
        assert_eq!(LastRewardAudit::<Test>::get().map(|audit| audit.0), Some(3));
    });
}

#[test]
fn skip_event_emitted_when_author_not_found() {
    new_test_ext().execute_with(|| {
        MOCK_AUTHOR.with(|v| *v.borrow_mut() = None);

        <FullnodeIssuance as Hooks<u32>>::on_finalize(1);

        assert_eq!(RewardedBlockCount::<Test>::get(), 0);
        assert_eq!(TotalFullnodeIssued::<Test>::get(), 0);
        assert_eq!(LastRewardAudit::<Test>::get(), None);

        let has_event = System::events().iter().any(|r| {
            matches!(
                r.event,
                RuntimeEvent::FullnodeIssuance(Event::FullnodeIssuanceSkippedNoAuthor { block: 1 })
            )
        });
        assert!(has_event);
    });
}

#[test]
fn reward_issued_to_miner_when_reward_account_not_bound() {
    new_test_ext().execute_with(|| {
        let miner = account(101);
        MOCK_AUTHOR.with(|v| *v.borrow_mut() = Some(miner.clone()));

        <FullnodeIssuance as Hooks<u32>>::on_finalize(1);

        // 未绑定时奖励发到矿工，并 emit FullnodeIssuanceIssued（reward_account_id = miner）
        assert_eq!(
            Balances::free_balance(miner.clone()),
            primitives::pow_const::FULLNODE_BLOCK_REWARD
        );
        assert_eq!(LastAuthoredBlockByMiner::<Test>::get(&miner), Some(1));
        let has_event = System::events().iter().any(|r| {
            matches!(
                r.event,
                RuntimeEvent::FullnodeIssuance(Event::FullnodeIssuanceIssued {
                    block: 1,
                    miner_account_id: ref m,
                    reward_account_id: ref w,
                    ..
                }) if m == &miner && w == &miner
            )
        });
        assert!(has_event);
    });
}

#[test]
fn prebound_reward_account_receives_first_authored_block_reward() {
    new_test_ext().execute_with(|| {
        let miner = account(104);
        let reward_account_id = account(105);

        // 矿工可在首次出块前预绑定收款账户，首块奖励直接进入该账户。
        assert_ok!(FullnodeIssuance::bind_reward_account(
            RuntimeOrigin::signed(miner.clone()),
            reward_account_id.clone()
        ));
        MOCK_AUTHOR.with(|v| *v.borrow_mut() = Some(miner.clone()));

        <FullnodeIssuance as Hooks<u32>>::on_finalize(1);
        assert_eq!(Balances::free_balance(miner.clone()), 0);
        assert_eq!(
            Balances::free_balance(reward_account_id),
            primitives::pow_const::FULLNODE_BLOCK_REWARD
        );
        assert_eq!(LastAuthoredBlockByMiner::<Test>::get(&miner), Some(1));
    });
}

#[test]
fn reward_account_can_be_rebound_by_miner() {
    new_test_ext().execute_with(|| {
        let miner = account(111);
        let reward_account_id_1 = account(112);
        let reward_account_id_2 = account(113);
        assert_ok!(FullnodeIssuance::bind_reward_account(
            RuntimeOrigin::signed(miner.clone()),
            reward_account_id_1
        ));

        assert_ok!(FullnodeIssuance::rebind_reward_account(
            RuntimeOrigin::signed(miner.clone()),
            reward_account_id_2.clone()
        ));
        assert_eq!(
            RewardAccountIdByMiner::<Test>::get(&miner),
            Some(reward_account_id_2.clone())
        );

        let has_event = System::events().iter().any(|r| {
            matches!(
                r.event,
                RuntimeEvent::FullnodeIssuance(Event::RewardAccountRebound {
                    miner_account_id: ref m,
                    new_reward_account_id: ref w
                }) if m == &miner && w == &reward_account_id_2
            )
        });
        assert!(has_event);
    });
}

#[test]
fn rebind_rejects_miner_reward_account() {
    new_test_ext().execute_with(|| {
        let miner = account(114);
        let reward_account_id = account(115);
        assert_ok!(FullnodeIssuance::bind_reward_account(
            RuntimeOrigin::signed(miner.clone()),
            reward_account_id.clone()
        ));

        assert_noop!(
            FullnodeIssuance::rebind_reward_account(
                RuntimeOrigin::signed(miner.clone()),
                miner.clone()
            ),
            Error::<Test>::RewardAccountCannotBeMiner
        );
        assert_eq!(
            RewardAccountIdByMiner::<Test>::get(&miner),
            Some(reward_account_id)
        );
    });
}

#[test]
fn rebind_rejects_unchanged_reward_account() {
    new_test_ext().execute_with(|| {
        let miner = account(116);
        let reward_account_id = account(117);
        assert_ok!(FullnodeIssuance::bind_reward_account(
            RuntimeOrigin::signed(miner.clone()),
            reward_account_id.clone()
        ));

        assert_noop!(
            FullnodeIssuance::rebind_reward_account(
                RuntimeOrigin::signed(miner.clone()),
                reward_account_id.clone()
            ),
            Error::<Test>::RewardAccountUnchanged
        );
        assert_eq!(
            RewardAccountIdByMiner::<Test>::get(&miner),
            Some(reward_account_id)
        );
    });
}

#[test]
fn rebind_requires_existing_binding() {
    new_test_ext().execute_with(|| {
        let miner = account(121);
        let reward_account_id = account(122);
        assert_noop!(
            FullnodeIssuance::rebind_reward_account(
                RuntimeOrigin::signed(miner),
                reward_account_id
            ),
            Error::<Test>::RewardAccountNotBound
        );
    });
}

#[test]
fn reward_goes_to_new_reward_account_id_after_rebind() {
    new_test_ext().execute_with(|| {
        let miner = account(131);
        let reward_account_id_1 = account(132);
        let reward_account_id_2 = account(133);

        assert_ok!(FullnodeIssuance::bind_reward_account(
            RuntimeOrigin::signed(miner.clone()),
            reward_account_id_1.clone()
        ));
        MOCK_AUTHOR.with(|v| *v.borrow_mut() = Some(miner.clone()));

        // 第 1 块奖励 -> reward_account_id_1
        <FullnodeIssuance as Hooks<u32>>::on_finalize(1);
        assert_eq!(
            Balances::free_balance(reward_account_id_1.clone()),
            primitives::pow_const::FULLNODE_BLOCK_REWARD
        );

        // 重绑到 reward_account_id_2
        assert_ok!(FullnodeIssuance::rebind_reward_account(
            RuntimeOrigin::signed(miner),
            reward_account_id_2.clone()
        ));

        // 第 2 块奖励 -> reward_account_id_2，reward_account_id_1 不再增长
        <FullnodeIssuance as Hooks<u32>>::on_finalize(2);
        assert_eq!(
            Balances::free_balance(reward_account_id_1),
            primitives::pow_const::FULLNODE_BLOCK_REWARD
        );
        assert_eq!(
            Balances::free_balance(reward_account_id_2),
            primitives::pow_const::FULLNODE_BLOCK_REWARD
        );
    });
}

#[test]
fn on_initialize_declares_weight_only_within_reward_range() {
    new_test_ext().execute_with(|| {
        let w0 = <FullnodeIssuance as Hooks<u32>>::on_initialize(0);
        assert_eq!(w0, Weight::zero());

        let w1 = <FullnodeIssuance as Hooks<u32>>::on_initialize(1);
        assert_ne!(w1, Weight::zero());

        let w_after = <FullnodeIssuance as Hooks<u32>>::on_initialize(
            primitives::pow_const::FULLNODE_REWARD_END_BLOCK + 1,
        );
        assert_eq!(w_after, Weight::zero());
    });
}

#[test]
fn reward_audit_scale_contract_matches_node_guard() {
    use codec::{Decode, Encode};

    let miner = account(141);
    let reward_account_id = account(142);
    let value = (
        7u32,
        miner.clone(),
        reward_account_id.clone(),
        primitives::pow_const::FULLNODE_BLOCK_REWARD,
    );
    let encoded = value.encode();
    let decoded = <(u32, AccountId32, AccountId32, u128)>::decode(&mut &encoded[..])
        .expect("node mirror tuple must decode runtime audit");
    assert_eq!(decoded, value);
}
