#![cfg(test)]

use super::*;

#[test]
fn transfer_with_remark_moves_balance_and_emits_bound_remark() {
    new_test_ext().execute_with(|| {
        let from = account(1);
        let beneficiary_account_id = account(2);
        let remark = crate::pallet::TransferRemarkOf::<Test>::try_from(
            "美西已深夜，美东在庆国庆，中华联邦创世！"
                .as_bytes()
                .to_vec(),
        )
        .expect("remark should fit 99 bytes");

        assert_ok!(OnchainTransaction::transfer_with_remark(
            RuntimeOrigin::signed(from.clone()),
            beneficiary_account_id.clone(),
            123,
            remark.clone(),
        ));

        assert_eq!(Balances::free_balance(&from), 877);
        assert_eq!(Balances::free_balance(&beneficiary_account_id), 1_123);
        assert!(System::events().iter().any(|record| matches!(
            &record.event,
            RuntimeEvent::OnchainTransaction(pallet::Event::TransferWithRemark {
                from_account_id: event_from,
                beneficiary_account_id: event_beneficiary,
                amount,
                remark: event_remark,
            }) if event_from == &from
                && event_beneficiary == &beneficiary_account_id
                && *amount == 123
                && event_remark == &remark
        )));
    });
}

#[test]
fn transfer_with_remark_rejects_invalid_transfer_and_caps_remark_bytes() {
    new_test_ext().execute_with(|| {
        let remark = crate::pallet::TransferRemarkOf::<Test>::try_from(b"ok".to_vec())
            .expect("short remark should fit");

        assert_noop!(
            OnchainTransaction::transfer_with_remark(
                RuntimeOrigin::signed(account(1)),
                account(2),
                0,
                remark.clone(),
            ),
            pallet::Error::<Test>::ZeroAmount
        );
        assert_noop!(
            OnchainTransaction::transfer_with_remark(
                RuntimeOrigin::signed(account(1)),
                account(1),
                1,
                remark,
            ),
            pallet::Error::<Test>::SelfTransferNotAllowed
        );
        assert!(
            crate::pallet::TransferRemarkOf::<Test>::try_from(vec![b'a'; 100]).is_err(),
            "ordinary transfer remark must be capped at 99 bytes",
        );
    });
}

#[test]
fn charge_details_handles_all_fee_routes() {
    new_test_ext().execute_with(|| {
        let who = account(1);
        let call = sample_call();
        let info = call.get_dispatch_info();

        let (payer_account_id, fee) =
            charge_details::<Test, Balances, FeeRouteOnchain>(&who, &call, &info, 0)
                .expect("onchain route")
                .expect("onchain route must charge");
        assert_eq!(payer_account_id, who);
        assert_eq!(fee, 50);

        let (_, vote_fee) =
            charge_details::<Test, Balances, FeeRouteVote>(&account(1), &call, &info, 0)
                .expect("vote route")
                .expect("vote route must charge");
        assert_eq!(vote_fee, 100);

        assert!(
            charge_details::<Test, Balances, FeeRouteOffchain>(&account(1), &call, &info, 0,)
                .expect("offchain route")
                .is_none()
        );
        assert!(
            charge_details::<Test, Balances, FeeRouteFree>(&account(1), &call, &info, 0,)
                .expect("free route")
                .is_none()
        );

        let reject_err =
            charge_details::<Test, Balances, FeeRouteReject>(&account(1), &call, &info, 0)
                .expect_err("reject route must fail");
        assert_eq!(reject_err, InvalidTransaction::Call.into());

        let tip_err =
            charge_details::<Test, Balances, FeeRouteOnchain>(&account(1), &call, &info, 1)
                .expect_err("non-zero tip must fail before charging");
        assert_eq!(tip_err, InvalidTransaction::Payment.into());
    });
}

#[test]
fn withdraw_and_can_withdraw_use_explicit_signer_payer_and_min_fee() {
    type Adapter = OnchainChargeAdapter<Balances, (), FeeRouteTinyOnchain>;

    new_test_ext().execute_with(|| {
        let who = account(1);
        let call = sample_call();
        let info = call.get_dispatch_info();

        assert_ok!(<Adapter as OnChargeTransaction<Test>>::can_withdraw_fee(
            &who, &call, &info, 0, 0
        ));

        let liq = <Adapter as OnChargeTransaction<Test>>::withdraw_fee(&who, &call, &info, 0, 0)
            .expect("withdraw should succeed")
            .expect("non-zero fee must return liquidity info");

        assert_eq!(Balances::free_balance(who), 990);
        assert_eq!(liq.peek(), 10);
    });
}

#[test]
fn withdraw_uses_explicit_route_payer() {
    type Adapter = OnchainChargeAdapter<Balances, (), FeeRouteTinyAccount2>;

    new_test_ext().execute_with(|| {
        let who = account(1);
        let payer_account_id = account(2);
        let call = sample_call();
        let info = call.get_dispatch_info();

        let _ = <Adapter as OnChargeTransaction<Test>>::withdraw_fee(&who, &call, &info, 0, 0)
            .expect("withdraw should succeed")
            .expect("non-zero fee must return liquidity info");

        assert_eq!(Balances::free_balance(who), 1_000);
        assert_eq!(Balances::free_balance(payer_account_id), 990);
    });
}

#[test]
fn execution_charger_uses_same_formula_exact_payer_and_ed_rule() {
    use primitives::fee_policy::OnchainFeeCharger;
    type Charger = OnchainExecutionFeeCharger<Test, Balances, ()>;

    new_test_ext().execute_with(|| {
        let payer_account_id = account(1);
        let unrelated_signer = account(2);
        let payer_before = Balances::free_balance(&payer_account_id);
        let signer_before = Balances::free_balance(&unrelated_signer);

        let fee = Charger::charge(&payer_account_id, 50_000)
            .expect("explicit payer_account_id can pay execution fee");
        assert_eq!(fee, 50);
        assert_eq!(
            Balances::free_balance(&payer_account_id),
            payer_before - fee
        );
        assert_eq!(Balances::free_balance(&unrelated_signer), signer_before);
        assert!(System::events().iter().any(|record| matches!(
            &record.event,
            RuntimeEvent::OnchainTransaction(pallet::Event::FeePaid {
                account_id,
                fee: 50
            }) if account_id == &payer_account_id
        )));

        let poor = account(3);
        let poor_before = Balances::free_balance(&poor);
        assert!(Charger::charge(&poor, 1).is_err());
        // 扣款失败必须保持原账户余额不变，也绝不能转向无关签名者代付。
        assert_eq!(Balances::free_balance(&poor), poor_before);
        assert_eq!(Balances::free_balance(&unrelated_signer), signer_before);
    });
}

#[test]
fn withdraw_no_amount_without_tip_returns_none_and_no_fee_paid_event() {
    type Adapter = OnchainChargeAdapter<Balances, (), FeeRouteFree>;

    new_test_ext().execute_with(|| {
        let who = account(1);
        let call = sample_call();
        let info = call.get_dispatch_info();
        let issuance_before = Balances::total_issuance();

        let liquidity =
            <Adapter as OnChargeTransaction<Test>>::withdraw_fee(&who, &call, &info, 0, 0)
                .expect("zero-fee withdraw should succeed");

        assert!(liquidity.is_none());
        assert_eq!(Balances::free_balance(who), 1_000);
        assert_eq!(Balances::total_issuance(), issuance_before);
        assert!(!has_fee_paid_event());
    });
}

#[test]
fn can_withdraw_and_withdraw_fail_when_insufficient_balance() {
    type Adapter = OnchainChargeAdapter<Balances, (), FeeRouteTinyOnchain>;

    new_test_ext().execute_with(|| {
        let poor = account(3);
        let call = sample_call();
        let info = call.get_dispatch_info();

        assert!(<Adapter as OnChargeTransaction<Test>>::can_withdraw_fee(
            &poor, &call, &info, 0, 0
        )
        .is_err());

        assert!(
            <Adapter as OnChargeTransaction<Test>>::withdraw_fee(&poor, &call, &info, 0, 0)
                .is_err()
        );
    });
}

#[test]
fn fee_router_distributes_to_bound_author_reward_account_and_nrc_and_safety_fund() {
    new_test_ext().execute_with(|| {
        let payer_account_id = account(1);
        let miner = account(9);
        let reward_account_id = account(8);
        let nrc = MockNrcAccountProvider::nrc_account().expect("nrc account must exist");
        let safety_fund = AccountId32::new(primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT);
        let issuance_before = Balances::total_issuance();
        let total_fee = 100u128;
        let fullnode_percent = primitives::fee_policy::ONCHAIN_FEE_FULLNODE_PERCENT as u128;
        let nrc_percent = primitives::fee_policy::ONCHAIN_FEE_NRC_PERCENT as u128;
        let safety_fund_percent = primitives::fee_policy::ONCHAIN_FEE_SAFETY_FUND_PERCENT as u128;
        let total_percent = fullnode_percent
            .saturating_add(nrc_percent)
            .saturating_add(safety_fund_percent);
        let expected_fullnode = total_fee.saturating_mul(fullnode_percent) / total_percent;
        let remainder = total_fee.saturating_sub(expected_fullnode);
        let expected_nrc = if nrc_percent.saturating_add(safety_fund_percent) == 0 {
            0
        } else {
            remainder.saturating_mul(nrc_percent) / nrc_percent.saturating_add(safety_fund_percent)
        };
        let expected_safety_fund = remainder.saturating_sub(expected_nrc);

        fullnode_issuance::RewardAccountIdByMiner::<Test>::insert(&miner, &reward_account_id);
        MOCK_AUTHOR.with(|v| *v.borrow_mut() = Some(miner.clone()));

        let credit = <Balances as Balanced<AccountId32>>::withdraw(
            &payer_account_id,
            100,
            Precision::Exact,
            Preservation::Preserve,
            Fortitude::Polite,
        )
        .expect("payer_account_id should have enough balance");

        OnchainFeeRouter::<
            Test,
            Balances,
            MockFindAuthor,
            MockNrcAccountProvider,
            MockSafetyFundAccountProvider,
        >::on_nonzero_unbalanced(credit);

        assert_eq!(Balances::free_balance(payer_account_id), 900);
        assert_eq!(
            Balances::free_balance(&reward_account_id),
            expected_fullnode
        );
        assert_eq!(Balances::free_balance(&nrc), expected_nrc);
        assert_eq!(Balances::free_balance(&safety_fund), expected_safety_fund);
        // 所有手续费都已分配到各账户，无销毁。
        assert_eq!(Balances::total_issuance(), issuance_before);
    });
}

#[test]
fn fee_router_burns_fullnode_share_when_author_not_bound() {
    new_test_ext().execute_with(|| {
        let payer_account_id = account(1);
        let miner = account(7);
        let missing_reward_account_id = account(6);
        let nrc = MockNrcAccountProvider::nrc_account().expect("nrc account must exist");
        let safety_fund = AccountId32::new(primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT);
        let issuance_before = Balances::total_issuance();
        let total_fee = 100u128;
        let fullnode_percent = primitives::fee_policy::ONCHAIN_FEE_FULLNODE_PERCENT as u128;
        let nrc_percent = primitives::fee_policy::ONCHAIN_FEE_NRC_PERCENT as u128;
        let safety_fund_percent = primitives::fee_policy::ONCHAIN_FEE_SAFETY_FUND_PERCENT as u128;
        let total_percent = fullnode_percent
            .saturating_add(nrc_percent)
            .saturating_add(safety_fund_percent);
        let expected_fullnode = total_fee.saturating_mul(fullnode_percent) / total_percent;
        let remainder = total_fee.saturating_sub(expected_fullnode);
        let expected_nrc = if nrc_percent.saturating_add(safety_fund_percent) == 0 {
            0
        } else {
            remainder.saturating_mul(nrc_percent) / nrc_percent.saturating_add(safety_fund_percent)
        };
        let expected_safety_fund = remainder.saturating_sub(expected_nrc);
        // 无作者奖励账户时：全节点分成销毁，NRC 和安全基金正常分配。
        let expected_burn = expected_fullnode;

        MOCK_AUTHOR.with(|v| *v.borrow_mut() = Some(miner.clone()));
        assert_eq!(
            fullnode_issuance::RewardAccountIdByMiner::<Test>::get(&miner),
            None
        );

        let credit = <Balances as Balanced<AccountId32>>::withdraw(
            &payer_account_id,
            100,
            Precision::Exact,
            Preservation::Preserve,
            Fortitude::Polite,
        )
        .expect("payer_account_id should have enough balance");

        OnchainFeeRouter::<
            Test,
            Balances,
            MockFindAuthor,
            MockNrcAccountProvider,
            MockSafetyFundAccountProvider,
        >::on_nonzero_unbalanced(credit);

        assert_eq!(Balances::free_balance(payer_account_id), 900);
        assert_eq!(Balances::free_balance(missing_reward_account_id), 0);
        assert_eq!(Balances::free_balance(&nrc), expected_nrc);
        assert_eq!(Balances::free_balance(&safety_fund), expected_safety_fund);
        assert_eq!(Balances::total_issuance(), issuance_before - expected_burn);
        assert!(has_fee_share_burn_event(
            pallet::BurnReason::RewardAccountUnbound,
            expected_burn
        ));
    });
}

#[test]
fn fee_router_burns_fullnode_share_when_author_not_found() {
    new_test_ext().execute_with(|| {
        let payer_account_id = account(1);
        let nrc = MockNrcAccountProvider::nrc_account().expect("nrc account must exist");
        let safety_fund = AccountId32::new(primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT);
        let issuance_before = Balances::total_issuance();
        let total_fee = 100u128;
        let fullnode_percent = primitives::fee_policy::ONCHAIN_FEE_FULLNODE_PERCENT as u128;
        let nrc_percent = primitives::fee_policy::ONCHAIN_FEE_NRC_PERCENT as u128;
        let safety_fund_percent = primitives::fee_policy::ONCHAIN_FEE_SAFETY_FUND_PERCENT as u128;
        let total_percent = fullnode_percent
            .saturating_add(nrc_percent)
            .saturating_add(safety_fund_percent);
        let expected_fullnode = total_fee.saturating_mul(fullnode_percent) / total_percent;
        let remainder = total_fee.saturating_sub(expected_fullnode);
        let expected_nrc = if nrc_percent.saturating_add(safety_fund_percent) == 0 {
            0
        } else {
            remainder.saturating_mul(nrc_percent) / nrc_percent.saturating_add(safety_fund_percent)
        };
        let expected_safety_fund = remainder.saturating_sub(expected_nrc);
        // 无作者时：全节点分成销毁，NRC 和安全基金正常分配。
        let expected_burn = expected_fullnode;

        MOCK_AUTHOR.with(|v| *v.borrow_mut() = None);
        let credit = <Balances as Balanced<AccountId32>>::withdraw(
            &payer_account_id,
            total_fee,
            Precision::Exact,
            Preservation::Preserve,
            Fortitude::Polite,
        )
        .expect("payer_account_id should have enough balance");

        OnchainFeeRouter::<
            Test,
            Balances,
            MockFindAuthor,
            MockNrcAccountProvider,
            MockSafetyFundAccountProvider,
        >::on_nonzero_unbalanced(credit);

        assert_eq!(Balances::free_balance(payer_account_id), 900);
        assert_eq!(Balances::free_balance(&nrc), expected_nrc);
        assert_eq!(Balances::free_balance(&safety_fund), expected_safety_fund);
        assert_eq!(Balances::total_issuance(), issuance_before - expected_burn);
        assert!(has_fee_share_burn_event(
            pallet::BurnReason::AuthorMissing,
            expected_burn
        ));
    });
}

#[test]
fn fee_router_burns_nrc_share_when_nrc_account_missing() {
    new_test_ext().execute_with(|| {
        let payer_account_id = account(1);
        let miner = account(9);
        let reward_account_id = account(8);
        let safety_fund = AccountId32::new(primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT);
        let issuance_before = Balances::total_issuance();
        let total_fee = 100u128;
        let fullnode_percent = primitives::fee_policy::ONCHAIN_FEE_FULLNODE_PERCENT as u128;
        let nrc_percent = primitives::fee_policy::ONCHAIN_FEE_NRC_PERCENT as u128;
        let safety_fund_percent = primitives::fee_policy::ONCHAIN_FEE_SAFETY_FUND_PERCENT as u128;
        let total_percent = fullnode_percent
            .saturating_add(nrc_percent)
            .saturating_add(safety_fund_percent);
        let expected_fullnode = total_fee.saturating_mul(fullnode_percent) / total_percent;
        let remainder = total_fee.saturating_sub(expected_fullnode);
        // NRC 账户缺失时：NRC 份额的 nrc_credit 被 drop（销毁），安全基金正常分配。
        let expected_nrc_for_split = if nrc_percent.saturating_add(safety_fund_percent) == 0 {
            0
        } else {
            remainder.saturating_mul(nrc_percent) / nrc_percent.saturating_add(safety_fund_percent)
        };
        let expected_safety_fund = remainder.saturating_sub(expected_nrc_for_split);
        let expected_burn = expected_nrc_for_split;

        fullnode_issuance::RewardAccountIdByMiner::<Test>::insert(&miner, &reward_account_id);
        MOCK_AUTHOR.with(|v| *v.borrow_mut() = Some(miner.clone()));
        let credit = <Balances as Balanced<AccountId32>>::withdraw(
            &payer_account_id,
            total_fee,
            Precision::Exact,
            Preservation::Preserve,
            Fortitude::Polite,
        )
        .expect("payer_account_id should have enough balance");

        OnchainFeeRouter::<
            Test,
            Balances,
            MockFindAuthor,
            MockNrcAccountProviderNone,
            MockSafetyFundAccountProvider,
        >::on_nonzero_unbalanced(credit);

        assert_eq!(Balances::free_balance(payer_account_id), 900);
        assert_eq!(
            Balances::free_balance(&reward_account_id),
            expected_fullnode
        );
        assert_eq!(Balances::free_balance(&safety_fund), expected_safety_fund);
        assert_eq!(Balances::total_issuance(), issuance_before - expected_burn);
        assert!(has_fee_share_burn_event(
            pallet::BurnReason::NrcMissing,
            expected_burn
        ));
    });
}

#[test]
fn fee_router_burns_fullnode_share_when_reward_account_id_resolve_fails() {
    new_test_ext().execute_with(|| {
        let payer_account_id = account(1);
        let miner = account(9);
        let reward_account_id = account(8);
        let nrc = MockNrcAccountProvider::nrc_account().expect("nrc account must exist");
        let safety_fund = AccountId32::new(primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT);
        let total_fee = 50u128;
        let fullnode_percent = primitives::fee_policy::ONCHAIN_FEE_FULLNODE_PERCENT as u128;
        let nrc_percent = primitives::fee_policy::ONCHAIN_FEE_NRC_PERCENT as u128;
        let safety_fund_percent = primitives::fee_policy::ONCHAIN_FEE_SAFETY_FUND_PERCENT as u128;
        let total_percent = fullnode_percent
            .saturating_add(nrc_percent)
            .saturating_add(safety_fund_percent);
        let expected_fullnode = total_fee.saturating_mul(fullnode_percent) / total_percent;
        let remainder = total_fee.saturating_sub(expected_fullnode);
        let expected_nrc =
            remainder.saturating_mul(nrc_percent) / nrc_percent.saturating_add(safety_fund_percent);
        let expected_safety_fund = remainder.saturating_sub(expected_nrc);

        TestExistentialDeposit::set(100);
        // 只让全节点奖励账户保持未创建状态，确保本用例命中 fullnode resolve 失败。
        let _ = Balances::deposit_creating(&nrc, 100);
        let _ = Balances::deposit_creating(&safety_fund, 100);
        let issuance_before = Balances::total_issuance();
        fullnode_issuance::RewardAccountIdByMiner::<Test>::insert(&miner, &reward_account_id);
        MOCK_AUTHOR.with(|v| *v.borrow_mut() = Some(miner));
        let credit = <Balances as Balanced<AccountId32>>::withdraw(
            &payer_account_id,
            total_fee,
            Precision::Exact,
            Preservation::Preserve,
            Fortitude::Polite,
        )
        .expect("payer_account_id should have enough balance");

        OnchainFeeRouter::<
            Test,
            Balances,
            MockFindAuthor,
            MockNrcAccountProvider,
            MockSafetyFundAccountProvider,
        >::on_nonzero_unbalanced(credit);

        assert_eq!(Balances::free_balance(payer_account_id), 950);
        assert_eq!(Balances::free_balance(&reward_account_id), 0);
        assert_eq!(Balances::free_balance(&nrc), 100 + expected_nrc);
        assert_eq!(
            Balances::free_balance(&safety_fund),
            100 + expected_safety_fund
        );
        assert_eq!(
            Balances::total_issuance(),
            issuance_before - expected_fullnode
        );
        assert!(has_fee_share_burn_event(
            pallet::BurnReason::FullnodeResolveFailed,
            expected_fullnode
        ));
        TestExistentialDeposit::set(1);
    });
}

struct MockNrcAccountProviderResolveFail;
impl NrcAccountProvider<AccountId32> for MockNrcAccountProviderResolveFail {
    fn nrc_account() -> Option<AccountId32> {
        Some(account(42))
    }
}

#[test]
fn fee_router_burns_nrc_share_when_resolve_fails() {
    new_test_ext().execute_with(|| {
        let payer_account_id = account(1);
        let miner = account(9);
        let reward_account_id = account(8);
        let nrc = MockNrcAccountProviderResolveFail::nrc_account()
            .expect("nrc account must exist for resolve failure test");
        let safety_fund = AccountId32::new(primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT);
        let total_fee = 500u128;
        let fullnode_percent = primitives::fee_policy::ONCHAIN_FEE_FULLNODE_PERCENT as u128;
        let nrc_percent = primitives::fee_policy::ONCHAIN_FEE_NRC_PERCENT as u128;
        let safety_fund_percent = primitives::fee_policy::ONCHAIN_FEE_SAFETY_FUND_PERCENT as u128;
        let total_percent = fullnode_percent
            .saturating_add(nrc_percent)
            .saturating_add(safety_fund_percent);
        let expected_fullnode = total_fee.saturating_mul(fullnode_percent) / total_percent;
        let remainder = total_fee.saturating_sub(expected_fullnode);
        let expected_nrc =
            remainder.saturating_mul(nrc_percent) / nrc_percent.saturating_add(safety_fund_percent);
        let expected_safety_fund = remainder.saturating_sub(expected_nrc);
        // NRC resolve 失败（ED 过高），NRC 份额销毁；安全基金正常分配。
        let expected_burn = expected_nrc;

        TestExistentialDeposit::set(100);
        // 本用例只验证 NRC 新账户低于 ED 时被销毁；
        // 安全基金账户先置为已存在账户，避免同样低于 ED 的份额也被销毁。
        let safety_fund_initial = 100;
        let _ = Balances::deposit_creating(&safety_fund, safety_fund_initial);
        let issuance_before = Balances::total_issuance();
        fullnode_issuance::RewardAccountIdByMiner::<Test>::insert(&miner, &reward_account_id);
        MOCK_AUTHOR.with(|v| *v.borrow_mut() = Some(miner));
        let credit = <Balances as Balanced<AccountId32>>::withdraw(
            &payer_account_id,
            total_fee,
            Precision::Exact,
            Preservation::Preserve,
            Fortitude::Polite,
        )
        .expect("payer_account_id should have enough balance");

        OnchainFeeRouter::<
            Test,
            Balances,
            MockFindAuthor,
            MockNrcAccountProviderResolveFail,
            MockSafetyFundAccountProvider,
        >::on_nonzero_unbalanced(credit);

        assert_eq!(Balances::free_balance(payer_account_id), 500);
        assert_eq!(
            Balances::free_balance(&reward_account_id),
            expected_fullnode
        );
        assert_eq!(Balances::free_balance(&nrc), 0);
        assert_eq!(
            Balances::free_balance(&safety_fund),
            safety_fund_initial + expected_safety_fund
        );
        assert_eq!(Balances::total_issuance(), issuance_before - expected_burn);
        assert!(has_fee_share_burn_event(
            pallet::BurnReason::NrcResolveFailed,
            expected_burn
        ));
        assert!(expected_nrc < 100, "nrc share should stay below high ED");
        TestExistentialDeposit::set(1);
    });
}

#[test]
fn fee_router_burns_safety_fund_share_when_resolve_fails() {
    new_test_ext().execute_with(|| {
        let payer_account_id = account(1);
        let miner = account(9);
        let reward_account_id = account(8);
        let nrc = MockNrcAccountProvider::nrc_account().expect("nrc account must exist");
        let safety_fund = AccountId32::new(primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT);
        let total_fee = 500u128;
        let fullnode_percent = primitives::fee_policy::ONCHAIN_FEE_FULLNODE_PERCENT as u128;
        let nrc_percent = primitives::fee_policy::ONCHAIN_FEE_NRC_PERCENT as u128;
        let safety_fund_percent = primitives::fee_policy::ONCHAIN_FEE_SAFETY_FUND_PERCENT as u128;
        let total_percent = fullnode_percent
            .saturating_add(nrc_percent)
            .saturating_add(safety_fund_percent);
        let expected_fullnode = total_fee.saturating_mul(fullnode_percent) / total_percent;
        let remainder = total_fee.saturating_sub(expected_fullnode);
        let expected_nrc =
            remainder.saturating_mul(nrc_percent) / nrc_percent.saturating_add(safety_fund_percent);
        let expected_safety_fund = remainder.saturating_sub(expected_nrc);

        TestExistentialDeposit::set(100);
        // 全节点奖励账户与 NRC 账户先置为已存在账户，只让安全基金新账户低于 ED。
        let _ = Balances::deposit_creating(&reward_account_id, 100);
        let _ = Balances::deposit_creating(&nrc, 100);
        let issuance_before = Balances::total_issuance();
        fullnode_issuance::RewardAccountIdByMiner::<Test>::insert(&miner, &reward_account_id);
        MOCK_AUTHOR.with(|v| *v.borrow_mut() = Some(miner));
        let credit = <Balances as Balanced<AccountId32>>::withdraw(
            &payer_account_id,
            total_fee,
            Precision::Exact,
            Preservation::Preserve,
            Fortitude::Polite,
        )
        .expect("payer_account_id should have enough balance");

        OnchainFeeRouter::<
            Test,
            Balances,
            MockFindAuthor,
            MockNrcAccountProvider,
            MockSafetyFundAccountProvider,
        >::on_nonzero_unbalanced(credit);

        assert_eq!(Balances::free_balance(payer_account_id), 500);
        assert_eq!(
            Balances::free_balance(&reward_account_id),
            100 + expected_fullnode
        );
        assert_eq!(Balances::free_balance(&nrc), 100 + expected_nrc);
        assert_eq!(Balances::free_balance(&safety_fund), 0);
        assert_eq!(
            Balances::total_issuance(),
            issuance_before - expected_safety_fund
        );
        assert!(has_fee_share_burn_event(
            pallet::BurnReason::SafetyFundResolveFailed,
            expected_safety_fund
        ));
        TestExistentialDeposit::set(1);
    });
}

#[test]
fn correct_and_deposit_does_not_refund_overpayment() {
    type Adapter = OnchainChargeAdapter<
        Balances,
        OnchainFeeRouter<
            Test,
            Balances,
            MockFindAuthor,
            MockNrcAccountProviderNone,
            MockSafetyFundAccountProvider,
        >,
        FeeRouteOnchain,
    >;

    new_test_ext().execute_with(|| {
        let who = account(1);
        let call = sample_call();
        let info = call.get_dispatch_info();
        let safety_fund = AccountId32::new(primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT);
        let issuance_before = Balances::total_issuance();
        let total_fee = 50u128;
        let fullnode_percent = primitives::fee_policy::ONCHAIN_FEE_FULLNODE_PERCENT as u128;
        let nrc_percent = primitives::fee_policy::ONCHAIN_FEE_NRC_PERCENT as u128;
        let safety_fund_percent = primitives::fee_policy::ONCHAIN_FEE_SAFETY_FUND_PERCENT as u128;
        let total_percent = fullnode_percent
            .saturating_add(nrc_percent)
            .saturating_add(safety_fund_percent);
        let expected_fullnode = total_fee.saturating_mul(fullnode_percent) / total_percent;
        let remainder = total_fee.saturating_sub(expected_fullnode);
        let expected_nrc_split = if nrc_percent.saturating_add(safety_fund_percent) == 0 {
            0
        } else {
            remainder.saturating_mul(nrc_percent) / nrc_percent.saturating_add(safety_fund_percent)
        };
        let expected_safety_fund = remainder.saturating_sub(expected_nrc_split);
        // 无作者 + 无 NRC 账户：全节点分成和 NRC 分成销毁，安全基金正常分配。
        let expected_burn = total_fee.saturating_sub(expected_safety_fund);

        MOCK_AUTHOR.with(|v| *v.borrow_mut() = None);
        let liquidity =
            <Adapter as OnChargeTransaction<Test>>::withdraw_fee(&who, &call, &info, 0, 0)
                .expect("withdraw should succeed");
        assert_eq!(Balances::free_balance(&who), 950);

        assert_ok!(
            <Adapter as OnChargeTransaction<Test>>::correct_and_deposit_fee(
                &who,
                &info,
                &Default::default(),
                1, // pretend corrected fee is tiny; adapter intentionally ignores it
                0,
                liquidity,
            )
        );

        assert_eq!(Balances::free_balance(&who), 950);
        assert_eq!(Balances::free_balance(&safety_fund), expected_safety_fund);
        assert_eq!(Balances::total_issuance(), issuance_before - expected_burn);
        assert_eq!(
            fee_share_burn_event_count(pallet::BurnReason::AuthorMissing, expected_fullnode),
            1
        );
        assert_eq!(
            fee_share_burn_event_count(pallet::BurnReason::NrcMissing, expected_nrc_split),
            1
        );
    });
}

#[test]
fn correct_and_deposit_fee_none_is_noop() {
    type Adapter = OnchainChargeAdapter<
        Balances,
        OnchainFeeRouter<
            Test,
            Balances,
            MockFindAuthor,
            MockNrcAccountProvider,
            MockSafetyFundAccountProvider,
        >,
        FeeRouteOnchain,
    >;

    new_test_ext().execute_with(|| {
        let who = account(1);
        let call = sample_call();
        let info = call.get_dispatch_info();
        let issuance_before = Balances::total_issuance();
        let balance_before = Balances::free_balance(&who);

        assert_ok!(
            <Adapter as OnChargeTransaction<Test>>::correct_and_deposit_fee(
                &who,
                &info,
                &Default::default(),
                0,
                0,
                None,
            )
        );

        assert_eq!(Balances::free_balance(&who), balance_before);
        assert_eq!(Balances::total_issuance(), issuance_before);
        assert!(!has_fee_paid_event());
    });
}

#[test]
fn nonzero_tip_is_rejected_without_deducting_or_distributing() {
    type Adapter = OnchainChargeAdapter<
        Balances,
        OnchainFeeRouter<
            Test,
            Balances,
            MockFindAuthor,
            MockNrcAccountProvider,
            MockSafetyFundAccountProvider,
        >,
        FeeRouteTinyOnchain,
    >;

    new_test_ext().execute_with(|| {
        let who = account(1);
        let call = sample_call();
        let info = call.get_dispatch_info();
        let balance_before = Balances::free_balance(&who);
        let result = <Adapter as OnChargeTransaction<Test>>::withdraw_fee(&who, &call, &info, 0, 5);
        assert_eq!(
            result.expect_err("non-zero tip must fail"),
            InvalidTransaction::Payment.into()
        );
        assert_eq!(Balances::free_balance(who), balance_before);
        assert_eq!(fee_share_burn_event_total(), 0);
    });
}

#[test]
fn charge_transaction_amount_path_routes_fee_to_all_accounts() {
    type Adapter = OnchainChargeAdapter<
        Balances,
        OnchainFeeRouter<
            Test,
            Balances,
            MockFindAuthor,
            MockNrcAccountProvider,
            MockSafetyFundAccountProvider,
        >,
        FeeRouteOnchain,
    >;

    new_test_ext().execute_with(|| {
        let who = account(1);
        let call = sample_call();
        let info = call.get_dispatch_info();
        let miner = account(9);
        let reward_account_id = account(8);
        let nrc = MockNrcAccountProvider::nrc_account().expect("nrc account must exist");
        let safety_fund = AccountId32::new(primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT);

        fullnode_issuance::RewardAccountIdByMiner::<Test>::insert(&miner, &reward_account_id);
        MOCK_AUTHOR.with(|v| *v.borrow_mut() = Some(miner));

        // FeeRouteOnchain 固定返回 50_000 分,链上资金交易费率 0.1%,应扣 50 分。
        let total_fee = 50u128;
        let fullnode_percent = primitives::fee_policy::ONCHAIN_FEE_FULLNODE_PERCENT as u128;
        let nrc_percent = primitives::fee_policy::ONCHAIN_FEE_NRC_PERCENT as u128;
        let safety_fund_percent = primitives::fee_policy::ONCHAIN_FEE_SAFETY_FUND_PERCENT as u128;
        let total_percent = fullnode_percent
            .saturating_add(nrc_percent)
            .saturating_add(safety_fund_percent);
        let expected_fullnode = total_fee.saturating_mul(fullnode_percent) / total_percent;
        let remainder = total_fee.saturating_sub(expected_fullnode);
        let expected_nrc = if nrc_percent.saturating_add(safety_fund_percent) == 0 {
            0
        } else {
            remainder.saturating_mul(nrc_percent) / nrc_percent.saturating_add(safety_fund_percent)
        };
        let expected_safety_fund = remainder.saturating_sub(expected_nrc);

        let liquidity =
            <Adapter as OnChargeTransaction<Test>>::withdraw_fee(&who, &call, &info, 0, 0)
                .expect("withdraw should succeed");
        assert_eq!(Balances::free_balance(&who), 950);

        assert_ok!(
            <Adapter as OnChargeTransaction<Test>>::correct_and_deposit_fee(
                &who,
                &info,
                &Default::default(),
                total_fee,
                0,
                liquidity,
            )
        );

        assert_eq!(Balances::free_balance(&who), 950);
        assert_eq!(
            Balances::free_balance(&reward_account_id),
            expected_fullnode
        );
        assert_eq!(Balances::free_balance(&nrc), expected_nrc);
        assert_eq!(Balances::free_balance(&safety_fund), expected_safety_fund);
        assert_eq!(fee_share_burn_event_total(), 0);
        assert!(has_fee_paid_event());
    });
}

/// 下发到 metadata 的三项收费参数必须逐字等于费率库真源。
///
/// 这三个常量只为把链上收费口径带给客户端（客户端据此预估费用、做余额预检），
/// 本 pallet 与任何客户端都不得另立交易费常量；本用例即是「只转发、不另立」的守卫：
/// 一旦有人在 runtime 绑定处改写数字，这里立刻红。
#[test]
fn exposed_fee_constants_forward_fee_policy_exactly() {
    use frame_support::traits::Get;

    let min_fee: u128 = <<Test as crate::pallet::Config>::OnchainMinFee as Get<u128>>::get();
    let fee_rate: sp_runtime::Perbill =
        <<Test as crate::pallet::Config>::OnchainFeeRate as Get<sp_runtime::Perbill>>::get();
    let vote_fee: u128 = <<Test as crate::pallet::Config>::VoteFlatFee as Get<u128>>::get();

    assert_eq!(min_fee, primitives::fee_policy::ONCHAIN_MIN_FEE);
    assert_eq!(fee_rate, primitives::fee_policy::ONCHAIN_FEE_RATE);
    assert_eq!(vote_fee, primitives::fee_policy::VOTE_FLAT_FEE);
}
