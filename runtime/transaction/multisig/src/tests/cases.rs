#![cfg(test)]

use super::*;

#[test]
fn nrc_transfer_executes_when_internal_vote_reaches_threshold() {
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

        // 本金只从主账户支出，链上费只从同一 CID 的费用账户支出。
        assert_eq!(Balances::free_balance(&funding_account_id), 9_000);
        assert_eq!(Balances::free_balance(nrc_fee_account()), 9_990);
        assert_eq!(Balances::free_balance(&dest), 1_000);
        // 提案数据仍保留（由 votingengine 延迟清理）
        assert!(votingengine::Pallet::<Test>::get_proposal_data(pid).is_some());
    });
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
        assert!(votingengine::Pallet::<Test>::get_proposal_data(pid).is_some());
    });
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
        assert!(votingengine::Pallet::<Test>::get_proposal_data(pid).is_some());
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
            let proposal = votingengine::Pallet::<Test>::proposals(last_proposal_id())
                .expect("transfer proposal should exist");
            assert_eq!(proposal.internal_code, Some(institution_code));
            assert_eq!(proposal.status, STATUS_VOTING);
        }
    });
}

#[test]
fn personal_account_transfer_executes_when_internal_vote_reaches_threshold() {
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
            .map(|account_id| admin_primitives::Admin {
                account_id,
                cid_number: Default::default(),
                family_name: Default::default(),
                given_name: Default::default(),
            })
            .collect::<Vec<_>>(),
        )
        .expect("admins should fit");

        personal_manage::PersonalAccounts::<Test>::insert(
            &personal_account_id,
            personal_manage::PersonalAccount {
                creator_account_id: personal_account_admin(0),
                account_name: b"personal"
                    .to_vec()
                    .try_into()
                    .expect("account name should fit"),
                created_at: 1,
                status: personal_manage::PersonalStatus::Active,
            },
        );
        personal_admins::AdminAccounts::<Test>::insert(
            personal_account_id.clone(),
            admin_primitives::AdminAccount {
                cid_number: Default::default(),
                institution_code: PERSONAL_CODE,
                kind: admin_primitives::AdminAccountKind::PersonalMultisig,
                admins,
                creator_account_id: personal_account_admin(0),
                created_at: 1,
                updated_at: 1,
                status: admin_primitives::AdminAccountStatus::Active,
            },
        );
        internal_vote::ActivePersonalThresholds::<Test>::insert(personal_account_id.clone(), 2);
        let _ = Balances::deposit_creating(&personal_account_id, 10_000);

        assert_ok!(propose_transfer(
            RuntimeOrigin::signed(personal_account_admin(0)),
            None,
            personal_account_id.clone(),
            dest.clone(),
            1_500,
            BoundedVec::default(),
        ));
        let pid = last_proposal_id();

        let vote_pairs = personal_account_pairs(2);
        assert_ok!(cast_transfer_votes_n(&vote_pairs[1..], 1, pid,));

        assert_eq!(Balances::free_balance(&personal_account_id), 8_490);
        assert_eq!(Balances::free_balance(&dest), 1_500);
        assert_eq!(
            votingengine::Pallet::<Test>::proposals(pid)
                .expect("proposal should exist")
                .status,
            STATUS_EXECUTED
        );
    });
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
            votingengine::Pallet::<Test>::proposals(pid)
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

        // 活跃提案数限制由 votingengine 全局管控（上限 10），第二个提案可以成功
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
            BoundedVec::default(),
        ));
        let pid1 = last_proposal_id();

        let end = votingengine::Pallet::<Test>::proposals(pid1)
            .expect("proposal should exist")
            .end;
        System::set_block_number(end + 1);
        assert_ok!(votingengine::Pallet::<Test>::finalize_proposal(
            RuntimeOrigin::signed(nrc_admin(0)),
            pid1
        ));
        assert_eq!(
            votingengine::Pallet::<Test>::proposals(pid1)
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
            votingengine::Pallet::<Test>::proposals(pid)
                .expect("proposal should exist")
                .status,
            STATUS_PASSED
        );
        assert_eq!(Balances::free_balance(&dest), 0);
        assert!(votingengine::Pallet::<Test>::get_proposal_data(pid).is_some());

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
            votingengine::Error::<Test>::ProposalNotRetryable
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
            votingengine::Error::<Test>::NoPermission
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
            votingengine::Pallet::<Test>::proposals(pid)
                .expect("proposal should exist")
                .status,
            STATUS_EXECUTED
        );

        // 已执行提案再次走统一重试入口应被拒绝。
        assert_noop!(
            VotingEngine::retry_passed_proposal(RuntimeOrigin::signed(nrc_admin(0)), pid),
            votingengine::Error::<Test>::ProposalNotRetryable
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
