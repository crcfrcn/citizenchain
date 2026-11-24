//! 验证多签转账阈值、备注费用、安全基金及失败后的原子状态。

#![cfg(test)]

use super::*;

#[test]
fn nonempty_remark_fee_rolls_back_after_withdrawal_then_charges_once_on_retry() {
    use frame_support::traits::LockableCurrency;
    new_test_ext().execute_with(|| {
        let funding = nrc_main_account();
        let payer = nrc_fee_account();
        let dest = beneficiary_account_id();
        assert_ok!(propose_transfer(
            RuntimeOrigin::signed(nrc_admin(0)),
            Some(nrc_actor_cid()),
            funding.clone(),
            dest.clone(),
            1_000,
            b" memo ".to_vec().try_into().unwrap()
        ));
        let pid = last_proposal_id();
        <Balances as LockableCurrency<_>>::set_lock(
            *b"memofail",
            &funding,
            10_000,
            WithdrawReasons::TRANSFER,
        );
        let issuance = Balances::total_issuance();
        let votes = nrc_pass_pairs();
        assert_ok!(cast_transfer_votes_n(
            &votes[1..],
            nrc_pass_count() - 1,
            pid
        ));
        assert_eq!(VotingEngine::proposals(pid).unwrap().status, STATUS_PASSED);
        assert_eq!(Balances::free_balance(&payer), 10_000);
        assert_eq!(Balances::free_balance(&funding), 10_000);
        assert_eq!(Balances::free_balance(&dest), 0);
        assert_eq!(Balances::total_issuance(), issuance);
        <Balances as LockableCurrency<_>>::remove_lock(*b"memofail", &funding);
        assert_ok!(VotingEngine::retry_passed_proposal(
            RuntimeOrigin::signed(nrc_admin(0)),
            pid
        ));
        assert_eq!(Balances::free_balance(&payer), 9_890);
        assert_eq!(Balances::free_balance(&funding), 9_000);
        assert_eq!(Balances::free_balance(&dest), 1_000);
        assert_noop!(
            VotingEngine::retry_passed_proposal(RuntimeOrigin::signed(nrc_admin(0)), pid),
            engingcore::Error::<Test>::ProposalNotRetryable
        );
        assert_eq!(Balances::free_balance(&payer), 9_890);
    });
}

#[test]
fn remark_fee_precheck_includes_ed_and_never_charges_rejected_proposals() {
    new_test_ext().execute_with(|| {
        let payer = nrc_fee_account();
        assert_ok!(Balances::force_set_balance(
            RuntimeOrigin::root(),
            payer.clone(),
            110
        ));
        assert_noop!(
            propose_transfer(
                RuntimeOrigin::signed(nrc_admin(0)),
                Some(nrc_actor_cid()),
                nrc_main_account(),
                beneficiary_account_id(),
                1_000,
                vec![b'a'].try_into().unwrap()
            ),
            Error::<Test>::InsufficientFeeBalance
        );
        assert_eq!(Balances::free_balance(&payer), 110);
        assert_ok!(Balances::force_set_balance(
            RuntimeOrigin::root(),
            payer.clone(),
            111
        ));
        assert_ok!(propose_transfer(
            RuntimeOrigin::signed(nrc_admin(0)),
            Some(nrc_actor_cid()),
            nrc_main_account(),
            beneficiary_account_id(),
            1_000,
            vec![b'a'].try_into().unwrap()
        ));
        let pid = last_proposal_id();
        assert_eq!(Balances::free_balance(&payer), 111);
        let votes = nrc_pass_pairs();
        assert_ok!(cast_transfer_votes_n(
            &votes[1..],
            nrc_pass_count() - 1,
            pid
        ));
        assert_eq!(Balances::free_balance(&payer), 1);
        assert_eq!(
            VotingEngine::proposals(pid).unwrap().status,
            STATUS_EXECUTED
        );
    });
}

#[test]
fn safety_fund_remark_fee_is_paid_only_by_nrc_fee_account() {
    for remark in [vec![], b"a".to_vec(), b" ".to_vec(), vec![b'a'; 256]] {
        new_test_ext().execute_with(|| {
            let fund = AccountId32::new(primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT);
            let _ = Balances::deposit_creating(&fund, 10_000);
            let role = test_role_code(NRC).to_vec().try_into().unwrap();
            assert_ok!(Multisig::propose_safety_fund_transfer(
                RuntimeOrigin::signed(nrc_admin(0)),
                nrc_actor_cid(),
                role,
                fund.clone(),
                beneficiary_account_id(),
                1_000,
                remark.clone().try_into().unwrap()
            ));
            let pid = last_proposal_id();
            assert_eq!(Balances::free_balance(nrc_fee_account()), 10_000);
            let votes = nrc_pass_pairs();
            assert_ok!(cast_transfer_votes_n(
                &votes[1..],
                nrc_pass_count() - 1,
                pid
            ));
            let fee = 10
                + if remark.is_empty() {
                    0
                } else {
                    primitives::fee_policy::FLAT_FEE
                };
            assert_eq!(Balances::free_balance(&fund), 9_000);
            assert_eq!(Balances::free_balance(nrc_fee_account()), 10_000 - fee);
            assert_eq!(Balances::free_balance(nrc_main_account()), 10_000);
        });
    }
}

#[test]
fn nrc_transfer_executes_when_internal_vote_reaches_threshold() {
    for remark in [
        vec![],
        b"a".to_vec(),
        "中".as_bytes().to_vec(),
        b" ".to_vec(),
        b"\n".to_vec(),
        vec![b'a'; 256],
    ] {
        new_test_ext().execute_with(|| {
            let funding_account_id = nrc_main_account();
            let dest = beneficiary_account_id();

            assert_ok!(propose_transfer(
                RuntimeOrigin::signed(nrc_admin(0)),
                Some(nrc_actor_cid()),
                funding_account_id.clone(),
                dest.clone(),
                1_000,
                remark.clone().try_into().unwrap(),
            ));
            let pid = last_proposal_id();
            assert_eq!(Balances::free_balance(nrc_fee_account()), 10_000);

            let vote_pairs = nrc_pass_pairs();
            assert_ok!(cast_transfer_votes_n(
                &vote_pairs[1..],
                nrc_pass_count().saturating_sub(1),
                pid,
            ));

            // 本金只从主账户支出，金额费及非空备注费只从同一 CID 的费用账户支出。
            assert_eq!(Balances::free_balance(&funding_account_id), 9_000);
            assert_eq!(
                Balances::free_balance(nrc_fee_account()),
                9_990
                    - if remark.is_empty() {
                        0
                    } else {
                        primitives::fee_policy::FLAT_FEE
                    }
            );
            assert_eq!(Balances::free_balance(&dest), 1_000);
            // 提案数据仍保留（由 engingcore 延迟清理）
            assert!(engingcore::Pallet::<Test>::get_proposal_data(pid).is_some());
        });
    }
}

#[test]
fn prc_transfer_executes_when_internal_vote_reaches_threshold() {
    new_test_ext().execute_with(|| {
        let funding_account_id = prc_main_account();
        let dest = beneficiary_account_id();

        assert_ok!(propose_transfer(
            RuntimeOrigin::signed(prc_admin(0)),
            Some(prc_actor_cid()),
            funding_account_id.clone(),
            dest.clone(),
            2_000,
            BoundedVec::default(),
        ));
        let pid = last_proposal_id();

        let vote_pairs = prc_pass_pairs();
        assert_ok!(cast_transfer_votes_n(
            &vote_pairs[1..],
            prc_pass_count().saturating_sub(1),
            pid,
        ));

        assert_eq!(Balances::free_balance(&funding_account_id), 8_000);
        assert_eq!(Balances::free_balance(prc_fee_account()), 9_990);
        assert_eq!(Balances::free_balance(&dest), 2_000);
        assert!(engingcore::Pallet::<Test>::get_proposal_data(pid).is_some());
    });
}

#[test]
fn prc_fee_sweep_rejects_other_cids_and_pays_its_main_at_six_votes() {
    new_test_ext().execute_with(|| {
        let cid = prc_actor_cid();
        let role: RoleCode = test_role_code(PRC)
            .to_vec()
            .try_into()
            .expect("PRC role fits");
        let amount = 1_000;
        let fee = primitives::fee_policy::calculate_onchain_fee(amount);
        assert_noop!(
            Multisig::propose_sweep_to_main(
                RuntimeOrigin::signed(prc_admin(0)),
                cid.clone(),
                role.clone(),
                nrc_fee_account(),
                amount,
            ),
            Error::<Test>::InvalidInstitution
        );
        assert_ok!(Multisig::propose_sweep_to_main(
            RuntimeOrigin::signed(prc_admin(0)),
            cid,
            role,
            prc_fee_account(),
            amount,
        ));
        let pid = last_proposal_id();
        let votes = prc_pass_pairs();
        assert_eq!(votes.len(), 6);
        // 发起时已有一票，补四票后仍不得划转。
        assert_ok!(cast_transfer_votes_n(&votes[1..5], 4, pid));
        assert_eq!(VotingEngine::proposals(pid).unwrap().status, STATUS_VOTING);
        assert_eq!(Balances::free_balance(prc_main_account()), 10_000);
        assert_eq!(Balances::free_balance(prc_fee_account()), 10_000);

        assert_ok!(cast_transfer_votes_n(&votes[5..], 1, pid));
        assert_eq!(
            VotingEngine::proposals(pid).unwrap().status,
            STATUS_EXECUTED
        );
        assert_eq!(Balances::free_balance(prc_main_account()), 10_000 + amount);
        assert_eq!(
            Balances::free_balance(prc_fee_account()),
            10_000 - amount - fee
        );
        assert_eq!(Balances::free_balance(nrc_main_account()), 10_000);
        assert_eq!(Balances::free_balance(nrc_fee_account()), 10_000);
        assert_eq!(Balances::free_balance(prb_main_account()), 10_000);
    });
}

#[test]
fn fee_sweep_keeps_ed_and_caps_principal_at_eighty_percent() {
    let ed = Balances::minimum_balance();
    // 前两组隔离保留额校验，后两组隔离本金上限。手续费均为 10 分。
    for (available, amount, executes) in [
        (20, 10, true),
        (20, 11, false),
        (1_000, 800, true),
        (1_000, 801, false),
    ] {
        new_test_ext().execute_with(|| {
            let before = ed + available;
            assert_ok!(Balances::force_set_balance(
                RuntimeOrigin::root(),
                prc_fee_account(),
                before
            ));
            assert_ok!(Multisig::propose_sweep_to_main(
                RuntimeOrigin::signed(prc_admin(0)),
                prc_actor_cid(),
                test_role_code(PRC)
                    .to_vec()
                    .try_into()
                    .expect("PRC role fits"),
                prc_fee_account(),
                amount,
            ));
            let pid = last_proposal_id();
            assert_ok!(cast_transfer_votes_n(
                &prc_pass_pairs()[1..],
                prc_pass_count() - 1,
                pid
            ));
            let (status, main, fee) = if executes {
                (STATUS_EXECUTED, 10_000 + amount, before - amount - 10)
            } else {
                assert!(engingcore::ProposalExecutionRetryStates::<Test>::contains_key(pid));
                (STATUS_PASSED, 10_000, before)
            };
            assert_eq!(VotingEngine::proposals(pid).unwrap().status, status);
            assert_eq!(Balances::free_balance(prc_main_account()), main);
            assert_eq!(Balances::free_balance(prc_fee_account()), fee);
        });
    }
}

#[test]
fn prb_transfer_executes_when_internal_vote_reaches_threshold() {
    new_test_ext().execute_with(|| {
        let funding_account_id = prb_main_account();
        let dest = beneficiary_account_id();

        assert_ok!(propose_transfer(
            RuntimeOrigin::signed(prb_admin(0)),
            Some(prb_actor_cid()),
            funding_account_id.clone(),
            dest.clone(),
            3_000,
            BoundedVec::default(),
        ));
        let pid = last_proposal_id();

        let vote_pairs = prb_pass_pairs();
        assert_ok!(cast_transfer_votes_n(
            &vote_pairs[1..],
            prb_pass_count().saturating_sub(1),
            pid,
        ));

        assert_eq!(Balances::free_balance(&funding_account_id), 7_000);
        assert_eq!(Balances::free_balance(prb_fee_account()), 9_990);
        assert_eq!(Balances::free_balance(&dest), 3_000);
        assert!(engingcore::Pallet::<Test>::get_proposal_data(pid).is_some());
    });
}

#[test]
fn frg_and_njd_can_create_multisig_transfer_internal_proposals() {
    new_test_ext().execute_with(|| {
        for (institution_code, actor_cid_number, funding_account_id, proposer_account_id) in [
            (FRG, frg_actor_cid(), frg_main_account(), frg_admin(0)),
            (NJD, njd_actor_cid(), njd_main_account(), njd_admin(0)),
        ] {
            assert_ok!(propose_transfer(
                RuntimeOrigin::signed(proposer_account_id),
                Some(actor_cid_number),
                funding_account_id.clone(),
                beneficiary_account_id(),
                1_000,
                BoundedVec::default(),
            ));
            let proposal = engingcore::Pallet::<Test>::proposals(last_proposal_id())
                .expect("transfer proposal should exist");
            assert_eq!(proposal.internal_code, Some(institution_code));
            assert_eq!(proposal.status, STATUS_VOTING);
        }
    });
}

#[test]
fn personal_account_transfer_executes_when_internal_vote_reaches_threshold() {
    for threshold in [1u32, 2, 3] {
        for remark in [
            vec![],
            b"a".to_vec(),
            "中".as_bytes().to_vec(),
            b" ".to_vec(),
            b"\n".to_vec(),
            vec![b'a'; 256],
        ] {
            new_test_ext().execute_with(|| {
                let personal_account_id = personal_account_id();
                let dest = beneficiary_account_id();
                let admins = BoundedVec::try_from(
                    [
                        personal_account_admin(0),
                        personal_account_admin(1),
                        personal_account_admin(2),
                    ]
                    .into_iter()
                    .map(|account_id| personal::Admin {
                        account_id,
                        family_name: Default::default(),
                        given_name: Default::default(),
                    })
                    .collect::<Vec<_>>(),
                )
                .expect("admins should fit");

                personal::PersonalAccounts::<Test>::insert(
                    &personal_account_id,
                    personal::PersonalAccount {
                        creator_account_id: personal_account_admin(0),
                        account_name: b"personal"
                            .to_vec()
                            .try_into()
                            .expect("account name should fit"),
                        created_at: 1,
                        status: personal::PersonalStatus::Active,
                    },
                );
                personal::AdminAccounts::<Test>::insert(
                    personal_account_id.clone(),
                    personal::AdminAccount {
                        institution_code: PERSONAL_CODE,
                        kind: personal::AdminAccountKind::PersonalMultisig,
                        admins,
                        creator_account_id: personal_account_admin(0),
                        created_at: 1,
                        updated_at: 1,
                        status: personal::AdminAccountStatus::Active,
                    },
                );
                internalvote::ActivePersonalThresholds::<Test>::insert(
                    personal_account_id.clone(),
                    threshold,
                );
                let _ = Balances::deposit_creating(&personal_account_id, 10_000);

                assert_ok!(propose_transfer(
                    RuntimeOrigin::signed(personal_account_admin(0)),
                    None,
                    personal_account_id.clone(),
                    dest.clone(),
                    1_500,
                    remark.clone().try_into().unwrap(),
                ));
                let pid = last_proposal_id();

                assert_eq!(
                    internalvote::InternalThresholdSnapshot::<Test>::get(pid),
                    Some(threshold)
                );
                let vote_pairs = personal_account_pairs(threshold as u8);
                if threshold == 1 {
                    assert_eq!(Balances::free_balance(&dest), 0);
                    <VotingEngine as Hooks<u64>>::on_initialize(System::block_number());
                } else {
                    assert_ok!(cast_transfer_votes_n(
                        &vote_pairs[1..],
                        threshold as usize - 1,
                        pid
                    ));
                }

                assert_eq!(
                    Balances::free_balance(&personal_account_id),
                    8_490
                        - if remark.is_empty() {
                            0
                        } else {
                            primitives::fee_policy::FLAT_FEE
                        }
                );
                assert_eq!(Balances::free_balance(&dest), 1_500);
                assert_eq!(
                    engingcore::Pallet::<Test>::proposals(pid)
                        .expect("proposal should exist")
                        .status,
                    STATUS_EXECUTED
                );
            });
        }
    }
}

#[test]
fn institution_account_transfer_executes_when_internal_vote_reaches_threshold() {
    new_test_ext().execute_with(|| {
        let funding_account_id = institution_account_id();
        let dest = beneficiary_account_id();
        let admins = BoundedVec::try_from(vec![
            institution_admin(0),
            institution_admin(1),
            institution_admin(2),
        ])
        .expect("admins should fit");

        insert_active_institution_account(&funding_account_id, admins);
        let _ = Balances::deposit_creating(&funding_account_id, 10_000);

        assert_ok!(propose_transfer(
            RuntimeOrigin::signed(institution_admin(0)),
            Some(test_cid_number()),
            funding_account_id.clone(),
            dest.clone(),
            2_000,
            BoundedVec::default(),
        ));
        let pid = last_proposal_id();

        let vote_pairs = institution_pairs(2);
        assert_ok!(cast_transfer_votes_n(&vote_pairs[1..], 1, pid,));

        assert_eq!(Balances::free_balance(&funding_account_id), 8_000);
        assert_eq!(Balances::free_balance(institution_fee_account()), 9_990);
        assert_eq!(Balances::free_balance(&dest), 2_000);
        assert_eq!(
            engingcore::Pallet::<Test>::proposals(pid)
                .expect("proposal should exist")
                .status,
            STATUS_EXECUTED
        );
    });
}

#[test]
fn institution_account_rejects_mismatched_actor_cid() {
    new_test_ext().execute_with(|| {
        let funding_account_id = institution_account_id();
        let admins = BoundedVec::try_from(vec![
            institution_admin(0),
            institution_admin(1),
            institution_admin(2),
        ])
        .expect("admins should fit");

        insert_active_institution_account(&funding_account_id, admins);
        let _ = Balances::deposit_creating(&funding_account_id, 10_000);

        assert_noop!(
            propose_transfer(
                RuntimeOrigin::signed(institution_admin(0)),
                Some(nrc_actor_cid()),
                funding_account_id.clone(),
                beneficiary_account_id(),
                1_000,
                BoundedVec::default(),
            ),
            Error::<Test>::InvalidInstitution
        );
    });
}

#[test]
fn unknown_account_cannot_be_used_as_transfer_source() {
    new_test_ext().execute_with(|| {
        let funding_account_id = AccountId32::new([0x77; 32]);

        assert_noop!(
            propose_transfer(
                RuntimeOrigin::signed(institution_admin(0)),
                None,
                funding_account_id.clone(),
                beneficiary_account_id(),
                1_000,
                BoundedVec::default(),
            ),
            Error::<Test>::InvalidInstitution
        );
    });
}

#[test]
fn zero_amount_is_rejected() {
    new_test_ext().execute_with(|| {
        let funding_account_id = nrc_main_account();
        let dest = beneficiary_account_id();

        assert_noop!(
            propose_transfer(
                RuntimeOrigin::signed(nrc_admin(0)),
                Some(nrc_actor_cid()),
                funding_account_id.clone(),
                dest,
                0,
                BoundedVec::default(),
            ),
            Error::<Test>::ZeroAmount
        );
    });
}

#[test]
fn self_transfer_is_rejected() {
    new_test_ext().execute_with(|| {
        let funding_account_id = nrc_main_account();

        assert_noop!(
            propose_transfer(
                RuntimeOrigin::signed(nrc_admin(0)),
                Some(nrc_actor_cid()),
                funding_account_id.clone(),
                funding_account_id.clone(),
                100,
                BoundedVec::default(),
            ),
            Error::<Test>::SelfTransferNotAllowed
        );
    });
}

#[test]
fn insufficient_balance_is_rejected_on_propose() {
    new_test_ext().execute_with(|| {
        let funding_account_id = nrc_main_account();
        let dest = beneficiary_account_id();

        // 本金账户只承担本金并保留 ED；费用账户另行承担手续费并保留自己的 ED。
        // 主账户余额 10_000 时，amount=10_000 会越过主账户 ED，必须拒绝。
        assert_noop!(
            propose_transfer(
                RuntimeOrigin::signed(nrc_admin(0)),
                Some(nrc_actor_cid()),
                funding_account_id.clone(),
                dest.clone(),
                10_000,
                BoundedVec::default(),
            ),
            Error::<Test>::InsufficientBalance
        );

        // amount=9_999 时主账户恰好保留 ED，费用账户余额也足以支付最低费。
        assert_ok!(propose_transfer(
            RuntimeOrigin::signed(nrc_admin(0)),
            Some(nrc_actor_cid()),
            funding_account_id.clone(),
            dest,
            9_999,
            BoundedVec::default(),
        ));
    });
}

#[test]
fn institution_fee_account_shortage_never_falls_back_to_admin() {
    new_test_ext().execute_with(|| {
        let admin = nrc_admin(0);
        let fee_account = nrc_fee_account();
        // 即使签名管理员余额充足，机构交易也只能从同一 CID 的费用账户付费。
        assert_ok!(Balances::force_set_balance(
            RuntimeOrigin::root(),
            admin.clone(),
            100_000,
        ));
        assert_ok!(Balances::force_set_balance(
            RuntimeOrigin::root(),
            fee_account.clone(),
            1,
        ));

        assert_noop!(
            propose_transfer(
                RuntimeOrigin::signed(admin.clone()),
                Some(nrc_actor_cid()),
                nrc_main_account(),
                beneficiary_account_id(),
                100,
                BoundedVec::default(),
            ),
            Error::<Test>::InsufficientFeeBalance
        );
        assert_eq!(Balances::free_balance(admin), 100_000);
        assert_eq!(Balances::free_balance(fee_account), 1);
    });
}

#[test]
fn multiple_proposals_allowed_within_limit() {
    new_test_ext().execute_with(|| {
        let funding_account_id = nrc_main_account();
        let dest = beneficiary_account_id();

        assert_ok!(propose_transfer(
            RuntimeOrigin::signed(nrc_admin(0)),
            Some(nrc_actor_cid()),
            funding_account_id.clone(),
            dest.clone(),
            100,
            BoundedVec::default(),
        ));

        // 活跃提案数限制由 engingcore 全局管控（上限 10），第二个提案可以成功
        assert_ok!(propose_transfer(
            RuntimeOrigin::signed(nrc_admin(0)),
            Some(nrc_actor_cid()),
            funding_account_id.clone(),
            dest,
            200,
            BoundedVec::default(),
        ));
    });
}

#[test]
fn executed_transfer_does_not_block_new_proposal() {
    new_test_ext().execute_with(|| {
        let funding_account_id = nrc_main_account();
        let dest = beneficiary_account_id();

        assert_ok!(propose_transfer(
            RuntimeOrigin::signed(nrc_admin(0)),
            Some(nrc_actor_cid()),
            funding_account_id.clone(),
            dest.clone(),
            100,
            BoundedVec::default(),
        ));
        let pid1 = last_proposal_id();

        let vote_pairs = nrc_pass_pairs();
        assert_ok!(cast_transfer_votes_n(
            &vote_pairs[1..],
            nrc_pass_count().saturating_sub(1),
            pid1,
        ));

        // 转账已执行，可以创建新提案
        assert_ok!(propose_transfer(
            RuntimeOrigin::signed(nrc_admin(0)),
            Some(nrc_actor_cid()),
            funding_account_id.clone(),
            dest,
            200,
            BoundedVec::default(),
        ));
    });
}

#[test]
fn rejected_proposal_does_not_block_new_proposal() {
    new_test_ext().execute_with(|| {
        let funding_account_id = nrc_main_account();
        let dest = beneficiary_account_id();

        assert_ok!(propose_transfer(
            RuntimeOrigin::signed(nrc_admin(0)),
            Some(nrc_actor_cid()),
            funding_account_id.clone(),
            dest.clone(),
            100,
            vec![b'a'].try_into().unwrap(),
        ));
        let pid1 = last_proposal_id();
        assert_eq!(Balances::free_balance(nrc_fee_account()), 10_000);

        let end = engingcore::Pallet::<Test>::proposals(pid1)
            .expect("proposal should exist")
            .end;
        System::set_block_number(end + 1);
        assert_ok!(engingcore::Pallet::<Test>::finalize_proposal(
            RuntimeOrigin::signed(nrc_admin(0)),
            pid1
        ));
        assert_eq!(
            engingcore::Pallet::<Test>::proposals(pid1)
                .expect("proposal should exist")
                .status,
            STATUS_REJECTED
        );

        // 被拒绝后可以创建新提案
        assert_ok!(propose_transfer(
            RuntimeOrigin::signed(nrc_admin(0)),
            Some(nrc_actor_cid()),
            funding_account_id.clone(),
            dest,
            50,
            BoundedVec::default(),
        ));
    });
}

#[test]
fn existential_deposit_is_preserved() {
    new_test_ext().execute_with(|| {
        let funding_account_id = nrc_main_account();
        let dest = beneficiary_account_id();

        // 主账户 10_000 支出 9_999 后保留 ED；手续费由独立费用账户支付。
        assert_ok!(propose_transfer(
            RuntimeOrigin::signed(nrc_admin(0)),
            Some(nrc_actor_cid()),
            funding_account_id.clone(),
            dest.clone(),
            9_999,
            BoundedVec::default(),
        ));
        let pid = last_proposal_id();

        let vote_pairs = nrc_pass_pairs();
        assert_ok!(cast_transfer_votes_n(
            &vote_pairs[1..],
            nrc_pass_count().saturating_sub(1),
            pid,
        ));

        assert_eq!(Balances::free_balance(&funding_account_id), 1);
        assert_eq!(Balances::free_balance(nrc_fee_account()), 9_990);
        assert_eq!(Balances::free_balance(&dest), 9_999);
    });
}

#[test]
fn retry_passed_transfer_succeeds_after_failed_auto_execution() {
    new_test_ext().execute_with(|| {
        let funding_account_id = nrc_main_account();
        let dest = beneficiary_account_id();

        // 余额 10_000,提案 9_000(预检通过),然后在投票通过前转走 9_000。
        // 使余额仅 1_000,自动执行因余额不足失败,但提案保留,可统一手动重试。
        assert_ok!(propose_transfer(
            RuntimeOrigin::signed(nrc_admin(0)),
            Some(nrc_actor_cid()),
            funding_account_id.clone(),
            dest.clone(),
            9_000,
            BoundedVec::default(),
        ));
        let pid = last_proposal_id();

        // 投票通过前转走余额,使自动执行失败。
        let drain_dest = AccountId32::new([88u8; 32]);
        let _ = Balances::deposit_creating(&drain_dest, 1);
        assert_ok!(<Balances as frame_support::traits::Currency<_>>::transfer(
            &funding_account_id,
            &drain_dest,
            9_000,
            frame_support::traits::ExistenceRequirement::KeepAlive,
        ));
        assert_eq!(Balances::free_balance(&funding_account_id), 1_000);

        // 投票达阈值后自动执行,但 try_execute_transfer 因余额不足失败。
        // 提案仍为 PASSED,转账未执行。
        let vote_pairs = nrc_pass_pairs();
        assert_ok!(cast_transfer_votes_n(
            &vote_pairs[1..],
            nrc_pass_count().saturating_sub(1),
            pid,
        ));
        assert_eq!(
            engingcore::Pallet::<Test>::proposals(pid)
                .expect("proposal should exist")
                .status,
            STATUS_PASSED
        );
        assert_eq!(Balances::free_balance(&dest), 0);
        assert!(engingcore::Pallet::<Test>::get_proposal_data(pid).is_some());

        // 补充余额后通过投票引擎统一入口手动重试。
        let _ = Balances::deposit_creating(&funding_account_id, 9_000);
        assert_eq!(Balances::free_balance(&funding_account_id), 10_000);
        assert_ok!(VotingEngine::retry_passed_proposal(
            RuntimeOrigin::signed(nrc_admin(0)),
            pid
        ));
        // 重试成功：主账户只转出本金，费用账户单独支付手续费。
        assert_eq!(Balances::free_balance(&funding_account_id), 1_000);
        assert_eq!(Balances::free_balance(nrc_fee_account()), 9_990);
        assert_eq!(Balances::free_balance(&dest), 9_000);
    });
}

#[test]
fn retry_passed_transfer_rejects_non_passed_proposal() {
    new_test_ext().execute_with(|| {
        let funding_account_id = nrc_main_account();
        let dest = beneficiary_account_id();

        assert_ok!(propose_transfer(
            RuntimeOrigin::signed(nrc_admin(0)),
            Some(nrc_actor_cid()),
            funding_account_id.clone(),
            dest,
            100,
            BoundedVec::default(),
        ));
        let pid = last_proposal_id();

        // 提案仍在投票中，不能手动重试。
        assert_noop!(
            VotingEngine::retry_passed_proposal(RuntimeOrigin::signed(nrc_admin(0)), pid),
            engingcore::Error::<Test>::ProposalNotRetryable
        );
    });
}

#[test]
fn retry_passed_transfer_rejects_non_admin() {
    new_test_ext().execute_with(|| {
        let funding_account_id = nrc_main_account();
        let dest = beneficiary_account_id();
        let outsider = AccountId32::new([88u8; 32]);
        let _ = Balances::deposit_creating(&outsider, 1);

        assert_ok!(propose_transfer(
            RuntimeOrigin::signed(nrc_admin(0)),
            Some(nrc_actor_cid()),
            funding_account_id.clone(),
            dest.clone(),
            100,
            BoundedVec::default(),
        ));
        let pid = last_proposal_id();

        // 减余额使自动执行失败
        let drain_dest = AccountId32::new([77u8; 32]);
        let _ = Balances::deposit_creating(&drain_dest, 1);
        assert_ok!(<Balances as frame_support::traits::Currency<_>>::transfer(
            &funding_account_id,
            &drain_dest,
            9_900,
            frame_support::traits::ExistenceRequirement::KeepAlive,
        ));

        let vote_pairs = nrc_pass_pairs();
        assert_ok!(cast_transfer_votes_n(
            &vote_pairs[1..],
            nrc_pass_count().saturating_sub(1),
            pid,
        ));

        // 自动执行失败，补充余额
        assert_eq!(Balances::free_balance(&dest), 0);
        let _ = Balances::deposit_creating(&funding_account_id, 10_000);

        // 统一重试入口只允许快照管理员手动重试。
        assert_noop!(
            VotingEngine::retry_passed_proposal(RuntimeOrigin::signed(outsider), pid),
            engingcore::Error::<Test>::NoPermission
        );
        assert_eq!(Balances::free_balance(&dest), 0);
    });
}

#[test]
fn executed_transfer_cannot_be_executed_again() {
    new_test_ext().execute_with(|| {
        let funding_account_id = nrc_main_account();
        let dest = beneficiary_account_id();

        assert_ok!(propose_transfer(
            RuntimeOrigin::signed(nrc_admin(0)),
            Some(nrc_actor_cid()),
            funding_account_id.clone(),
            dest.clone(),
            1_000,
            BoundedVec::default(),
        ));
        let pid = last_proposal_id();

        let vote_pairs = nrc_pass_pairs();
        assert_ok!(cast_transfer_votes_n(
            &vote_pairs[1..],
            nrc_pass_count().saturating_sub(1),
            pid,
        ));

        // 自动执行成功，状态变为 EXECUTED
        assert_eq!(
            engingcore::Pallet::<Test>::proposals(pid)
                .expect("proposal should exist")
                .status,
            STATUS_EXECUTED
        );

        // 已执行提案再次走统一重试入口应被拒绝。
        assert_noop!(
            VotingEngine::retry_passed_proposal(RuntimeOrigin::signed(nrc_admin(0)), pid),
            engingcore::Error::<Test>::ProposalNotRetryable
        );
    });
}

#[test]
fn protected_account_is_rejected() {
    new_test_ext().execute_with(|| {
        let funding_account_id = nrc_main_account();
        let protected = AccountId32::new([77u8; 32]);

        // 标记为受保护地址
        PROTECTED_ACCOUNT.with(|pa| *pa.borrow_mut() = Some(protected.clone()));

        assert_noop!(
            propose_transfer(
                RuntimeOrigin::signed(nrc_admin(0)),
                Some(nrc_actor_cid()),
                funding_account_id.clone(),
                protected,
                100,
                BoundedVec::default(),
            ),
            Error::<Test>::BeneficiaryIsProtectedAddress
        );
    });
}

#[test]
fn institution_spend_guard_blocks_transfer_proposal() {
    new_test_ext().execute_with(|| {
        let funding_account_id = nrc_main_account();
        let dest = beneficiary_account_id();
        DENIED_SPEND_SOURCE
            .with(|blocked| *blocked.borrow_mut() = Some(funding_account_id.clone()));

        assert_noop!(
            propose_transfer(
                RuntimeOrigin::signed(nrc_admin(0)),
                Some(nrc_actor_cid()),
                funding_account_id.clone(),
                dest,
                100,
                BoundedVec::default(),
            ),
            Error::<Test>::InstitutionSpendNotAllowed
        );

        DENIED_SPEND_SOURCE.with(|blocked| *blocked.borrow_mut() = None);
    });
}

#[test]
fn fee_respects_minimum_on_small_amount() {
    new_test_ext().execute_with(|| {
        let funding_account_id = nrc_main_account();
        let dest = beneficiary_account_id();

        // amount=1 时链上费命中最低值 10；主账户与费用账户分别保留 ED。
        assert_ok!(propose_transfer(
            RuntimeOrigin::signed(nrc_admin(0)),
            Some(nrc_actor_cid()),
            funding_account_id.clone(),
            dest.clone(),
            1,
            BoundedVec::default(),
        ));
        let pid = last_proposal_id();

        let vote_pairs = nrc_pass_pairs();
        assert_ok!(cast_transfer_votes_n(
            &vote_pairs[1..],
            nrc_pass_count().saturating_sub(1),
            pid,
        ));

        assert_eq!(Balances::free_balance(&funding_account_id), 9_999);
        assert_eq!(Balances::free_balance(nrc_fee_account()), 9_990);
        assert_eq!(Balances::free_balance(&dest), 1);
    });
}
