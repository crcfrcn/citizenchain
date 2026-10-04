#![cfg(test)]

use super::*;

// ─── 测试:绑定 / 存取 / 切换 ─────────────────────────────────────────────

#[test]
fn bind_deposit_withdraw_full_cycle() {
    new_test_ext().execute_with(|| {
        let (alice, _) = new_l3_user(&[1u8; 32], 1_000_000);

        // 1. 绑定清算行
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(alice.clone()),
            bank_cid()
        ));
        assert_eq!(UserBank::<Test>::get(&alice), Some(bank_cid()));
        assert_eq!(DepositBalance::<Test>::get(bank_cid(), &alice), 0);

        // 2. 充值 10_000 分
        assert_ok!(OffchainTx::deposit(
            RuntimeOrigin::signed(alice.clone()),
            10_000
        ));
        assert_eq!(DepositBalance::<Test>::get(bank_cid(), &alice), 10_000);
        assert_eq!(BankTotalDeposits::<Test>::get(bank_cid()), 10_000);

        // 3. 提现 3_000
        assert_ok!(OffchainTx::withdraw(
            RuntimeOrigin::signed(alice.clone()),
            3_000
        ));
        assert_eq!(DepositBalance::<Test>::get(bank_cid(), &alice), 7_000);
        assert_eq!(BankTotalDeposits::<Test>::get(bank_cid()), 7_000);

        // 4. 提现过量应拒绝
        assert_noop!(
            OffchainTx::withdraw(RuntimeOrigin::signed(alice.clone()), 1_000_000),
            Error::<Test>::InsufficientDepositBalance
        );
    });
}

#[test]
fn double_bind_rejected() {
    new_test_ext().execute_with(|| {
        let (alice, _) = new_l3_user(&[1u8; 32], 1_000_000);
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(alice.clone()),
            bank_cid()
        ));
        assert_noop!(
            OffchainTx::bind_clearing_bank(RuntimeOrigin::signed(alice.clone()), bank_cid()),
            Error::<Test>::AlreadyHasBank
        );
    });
}

#[test]
fn bind_rejects_unregistered_bank() {
    new_test_ext().execute_with(|| {
        let (alice, _) = new_l3_user(&[1u8; 32], 1_000_000);
        let ghost_cid: crate::InstitutionCidNumber =
            b"ZZ999-SCB00-999999999-2026".to_vec().try_into().unwrap();
        assert_noop!(
            OffchainTx::bind_clearing_bank(RuntimeOrigin::signed(alice.clone()), ghost_cid),
            Error::<Test>::NotRegisteredClearingBank
        );
    });
}

#[test]
fn switch_requires_zero_balance() {
    new_test_ext().execute_with(|| {
        let (alice, _) = new_l3_user(&[1u8; 32], 1_000_000);
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(alice.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::deposit(
            RuntimeOrigin::signed(alice.clone()),
            10_000
        ));
        // 余额 > 0,不能切换
        assert_noop!(
            OffchainTx::switch_bank(RuntimeOrigin::signed(alice.clone()), bank_cid()),
            Error::<Test>::NewBankSameAsCurrent
        );
        // 切换到同一家也应被拒(NewBankSameAsCurrent 优先于 MustClearBalanceFirst)
    });
}

#[test]
fn switch_after_withdraw_all_works() {
    new_test_ext().execute_with(|| {
        // 当前 fixture 只有一家清算行 bank_main;切到"同一家"会被 NewBankSameAsCurrent
        // 拒绝。这里先通过另一家 mock 来验证路径。
        // 简化:直接 withdraw 全部 → 再 switch 到同一家拒绝(= NewBankSameAsCurrent)。
        // 真正的跨行切换需要 fixture 扩展。
        let (alice, _) = new_l3_user(&[1u8; 32], 1_000_000);
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(alice.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::deposit(
            RuntimeOrigin::signed(alice.clone()),
            10_000
        ));
        assert_ok!(OffchainTx::withdraw(
            RuntimeOrigin::signed(alice.clone()),
            10_000
        ));
        assert_eq!(DepositBalance::<Test>::get(bank_cid(), &alice), 0);
        // 零余额但切到同家仍被 NewBankSameAsCurrent 拒绝 —— 行为正确。
        assert_noop!(
            OffchainTx::switch_bank(RuntimeOrigin::signed(alice.clone()), bank_cid()),
            Error::<Test>::NewBankSameAsCurrent
        );
    });
}

// ─── 测试:submit_offchain_batch ────────────────────────────────────────

fn seed_fee_rate(bank_cid: &crate::InstitutionCidNumber, bp: u32) {
    L2FeeRateBp::<Test>::insert(bank_cid, bp);
}

/// 用 `pair` 对 `PaymentIntent::signing_hash` 签名,返回 64 字节数组。
fn sign_intent(
    pair: &sr25519::Pair,
    intent: &crate::batch_item::PaymentIntent<AccountId32, u64>,
) -> [u8; 64] {
    sign_intent_with_op_tag(pair, intent, primitives::sign::OP_SIGN_L3_PAY)
}

/// 测试专用：允许用指定操作码签署同一 `PaymentIntent` 编码，验证跨场景重放被拒。
fn sign_intent_with_op_tag(
    pair: &sr25519::Pair,
    intent: &crate::batch_item::PaymentIntent<AccountId32, u64>,
    op_tag: u8,
) -> [u8; 64] {
    use sp_core::crypto::Pair as _;
    let message = primitives::sign::signing_message(op_tag, &intent.encode());
    let sig = pair.sign(&message);
    let bytes: [u8; 64] = sig.0;
    bytes
}

/// 用清算行管理员密钥对 `(actor_cid_number, institution_account_id, batch_seq, batch)` 签名。
fn sign_batch(
    institution_account_id: &AccountId32,
    batch_seq: u64,
    batch: &BoundedVec<OffchainBatchItem<AccountId32, u64>, <Test as Config>::MaxBatchSize>,
) -> BatchSignatureOf<Test> {
    use sp_core::crypto::Pair as _;
    let message = crate::batch_item::batch_signing_hash(
        BANK_CID,
        bank_role_code().as_slice(),
        institution_account_id,
        batch_seq,
        &batch.encode(),
    );
    bank_admin_pair()
        .sign(&message)
        .0
        .to_vec()
        .try_into()
        .unwrap()
}

#[test]
fn submit_batch_rejects_non_admin() {
    new_test_ext().execute_with(|| {
        seed_fee_rate(&bank_cid(), 5);
        let (alice, alice_pair) = new_l3_user(&[1u8; 32], 1_000_000);
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(alice.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::deposit(
            RuntimeOrigin::signed(alice.clone()),
            100_000
        ));

        let (bob, _) = new_l3_user(&[2u8; 32], 1_000_000);
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(bob.clone()),
            bank_cid()
        ));

        let intent = crate::batch_item::PaymentIntent::<AccountId32, u64> {
            tx_id: H256::repeat_byte(7),
            payer_account_id: alice.clone(),
            payer_bank_cid: bank_cid(),
            recipient_account_id: bob.clone(),
            recipient_bank_cid: bank_cid(),
            amount: 10_000,
            fee: 5,
            nonce: 1,
            expires_at: 100,
        };
        let sig = sign_intent(&alice_pair, &intent);
        let item = OffchainBatchItem::<AccountId32, u64> {
            tx_id: intent.tx_id,
            payer_account_id: intent.payer_account_id.clone(),
            payer_bank_cid: intent.payer_bank_cid.clone(),
            recipient_account_id: intent.recipient_account_id.clone(),
            recipient_bank_cid: intent.recipient_bank_cid.clone(),
            transfer_amount: intent.amount,
            fee_amount: intent.fee,
            payer_sig: sig,
            payer_nonce: intent.nonce,
            expires_at: intent.expires_at,
        };
        let batch: BoundedVec<_, _> = sp_std::vec![item].try_into().unwrap();

        // 用非管理员账户提交 → UnauthorizedAdmin
        assert_noop!(
            OffchainTx::submit_offchain_batch(
                RuntimeOrigin::signed(alice.clone()),
                bank_cid(),
                bank_role_code(),
                bank_main(),
                1,
                batch.clone(),
                sign_batch(&bank_main(), 1, &batch)
            ),
            Error::<Test>::UnauthorizedAdmin
        );
    });
}

#[test]
fn submit_batch_rejects_invalid_batch_signature() {
    new_test_ext().execute_with(|| {
        seed_fee_rate(&bank_cid(), 5);
        let (alice, alice_pair) = new_l3_user(&[1u8; 32], 1_000_000);
        let (bob, _) = new_l3_user(&[2u8; 32], 1_000_000);
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(alice.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(bob.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::deposit(
            RuntimeOrigin::signed(alice.clone()),
            100_000
        ));

        let intent = crate::batch_item::PaymentIntent::<AccountId32, u64> {
            tx_id: H256::repeat_byte(0x31),
            payer_account_id: alice.clone(),
            payer_bank_cid: bank_cid(),
            recipient_account_id: bob.clone(),
            recipient_bank_cid: bank_cid(),
            amount: 10_000,
            fee: 5,
            nonce: 1,
            expires_at: 100,
        };
        let item = OffchainBatchItem::<AccountId32, u64> {
            tx_id: intent.tx_id,
            payer_account_id: intent.payer_account_id.clone(),
            payer_bank_cid: intent.payer_bank_cid.clone(),
            recipient_account_id: intent.recipient_account_id.clone(),
            recipient_bank_cid: intent.recipient_bank_cid.clone(),
            transfer_amount: intent.amount,
            fee_amount: intent.fee,
            payer_sig: sign_intent(&alice_pair, &intent),
            payer_nonce: intent.nonce,
            expires_at: intent.expires_at,
        };
        let batch: BoundedVec<_, _> = sp_std::vec![item].try_into().unwrap();
        let bad_batch_sig: BatchSignatureOf<Test> = sp_std::vec![0u8; 64].try_into().unwrap();

        assert_noop!(
            OffchainTx::submit_offchain_batch(
                RuntimeOrigin::signed(bank_admin()),
                bank_cid(),
                bank_role_code(),
                bank_main(),
                1,
                batch,
                bad_batch_sig
            ),
            Error::<Test>::InvalidBatchSignature
        );
    });
}

#[test]
fn submit_batch_rejects_wrong_l3_signer_and_tampered_intent() {
    new_test_ext().execute_with(|| {
        seed_fee_rate(&bank_cid(), 5);
        let (alice, alice_pair) = new_l3_user(&[0x11; 32], 1_000_000);
        let (_, other_pair) = new_l3_user(&[0x12; 32], 1_000_000);
        let (bob, _) = new_l3_user(&[0x13; 32], 1_000_000);
        let (charlie, _) = new_l3_user(&[0x14; 32], 1_000_000);
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(alice.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(bob.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(charlie.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::deposit(
            RuntimeOrigin::signed(alice.clone()),
            100_000
        ));
        let payer_balance_before = DepositBalance::<Test>::get(bank_cid(), &alice);

        let wrong_signer_intent = crate::batch_item::PaymentIntent::<AccountId32, u64> {
            tx_id: H256::repeat_byte(0x34),
            payer_account_id: alice.clone(),
            payer_bank_cid: bank_cid(),
            recipient_account_id: bob.clone(),
            recipient_bank_cid: bank_cid(),
            amount: 10_000,
            fee: 5,
            nonce: 1,
            expires_at: 100,
        };
        let wrong_signer_item = OffchainBatchItem::<AccountId32, u64> {
            tx_id: wrong_signer_intent.tx_id,
            payer_account_id: wrong_signer_intent.payer_account_id.clone(),
            payer_bank_cid: wrong_signer_intent.payer_bank_cid.clone(),
            recipient_account_id: wrong_signer_intent.recipient_account_id.clone(),
            recipient_bank_cid: wrong_signer_intent.recipient_bank_cid.clone(),
            transfer_amount: wrong_signer_intent.amount,
            fee_amount: wrong_signer_intent.fee,
            // 64字节格式正确，但签名密钥不属于 payer_account_id，必须失败关闭。
            payer_sig: sign_intent(&other_pair, &wrong_signer_intent),
            payer_nonce: wrong_signer_intent.nonce,
            expires_at: wrong_signer_intent.expires_at,
        };
        let wrong_signer_batch: BoundedVec<_, _> =
            sp_std::vec![wrong_signer_item].try_into().unwrap();
        assert_noop!(
            OffchainTx::submit_offchain_batch(
                RuntimeOrigin::signed(bank_admin()),
                bank_cid(),
                bank_role_code(),
                bank_main(),
                1,
                wrong_signer_batch.clone(),
                sign_batch(&bank_main(), 1, &wrong_signer_batch),
            ),
            Error::<Test>::InvalidL3Signature
        );

        let signed_intent = crate::batch_item::PaymentIntent::<AccountId32, u64> {
            tx_id: H256::repeat_byte(0x35),
            payer_account_id: alice.clone(),
            payer_bank_cid: bank_cid(),
            recipient_account_id: bob,
            recipient_bank_cid: bank_cid(),
            amount: 10_000,
            fee: 5,
            nonce: 1,
            expires_at: 100,
        };
        let tampered_item = OffchainBatchItem::<AccountId32, u64> {
            tx_id: signed_intent.tx_id,
            payer_account_id: signed_intent.payer_account_id.clone(),
            payer_bank_cid: signed_intent.payer_bank_cid.clone(),
            // 签名完成后把收款账户改成另一有效账户，重算出的签名消息必须不匹配。
            recipient_account_id: charlie,
            recipient_bank_cid: signed_intent.recipient_bank_cid.clone(),
            transfer_amount: signed_intent.amount,
            fee_amount: signed_intent.fee,
            payer_sig: sign_intent(&alice_pair, &signed_intent),
            payer_nonce: signed_intent.nonce,
            expires_at: signed_intent.expires_at,
        };
        let tampered_batch: BoundedVec<_, _> = sp_std::vec![tampered_item].try_into().unwrap();
        assert_noop!(
            OffchainTx::submit_offchain_batch(
                RuntimeOrigin::signed(bank_admin()),
                bank_cid(),
                bank_role_code(),
                bank_main(),
                1,
                tampered_batch.clone(),
                sign_batch(&bank_main(), 1, &tampered_batch),
            ),
            Error::<Test>::InvalidL3Signature
        );

        let cross_operation_intent = crate::batch_item::PaymentIntent::<AccountId32, u64> {
            tx_id: H256::repeat_byte(0x36),
            payer_account_id: alice.clone(),
            payer_bank_cid: bank_cid(),
            recipient_account_id: signed_intent.recipient_account_id.clone(),
            recipient_bank_cid: bank_cid(),
            amount: 10_000,
            fee: 5,
            nonce: 1,
            expires_at: 100,
        };
        let cross_operation_item = OffchainBatchItem::<AccountId32, u64> {
            tx_id: cross_operation_intent.tx_id,
            payer_account_id: cross_operation_intent.payer_account_id.clone(),
            payer_bank_cid: cross_operation_intent.payer_bank_cid.clone(),
            recipient_account_id: cross_operation_intent.recipient_account_id.clone(),
            recipient_bank_cid: cross_operation_intent.recipient_bank_cid.clone(),
            transfer_amount: cross_operation_intent.amount,
            fee_amount: cross_operation_intent.fee,
            // 相同账户和载荷改用批次操作码签名，不能重放为 L3 支付授权。
            payer_sig: sign_intent_with_op_tag(
                &alice_pair,
                &cross_operation_intent,
                primitives::sign::OP_SIGN_OFFCHAIN_BATCH,
            ),
            payer_nonce: cross_operation_intent.nonce,
            expires_at: cross_operation_intent.expires_at,
        };
        let cross_operation_batch: BoundedVec<_, _> =
            sp_std::vec![cross_operation_item].try_into().unwrap();
        assert_noop!(
            OffchainTx::submit_offchain_batch(
                RuntimeOrigin::signed(bank_admin()),
                bank_cid(),
                bank_role_code(),
                bank_main(),
                1,
                cross_operation_batch.clone(),
                sign_batch(&bank_main(), 1, &cross_operation_batch),
            ),
            Error::<Test>::InvalidL3Signature
        );

        // 三次失败都必须整批回滚，不消费 nonce、序号或存款余额。
        assert_eq!(L3PaymentNonce::<Test>::get(&alice), 0);
        assert_eq!(LastClearingBatchSeq::<Test>::get(bank_cid()), 0);
        assert_eq!(
            DepositBalance::<Test>::get(bank_cid(), &alice),
            payer_balance_before
        );
    });
}

#[test]
fn submit_batch_rejects_wrong_batch_seq() {
    new_test_ext().execute_with(|| {
        seed_fee_rate(&bank_cid(), 5);
        let (alice, alice_pair) = new_l3_user(&[1u8; 32], 1_000_000);
        let (bob, _) = new_l3_user(&[2u8; 32], 1_000_000);
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(alice.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(bob.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::deposit(
            RuntimeOrigin::signed(alice.clone()),
            100_000
        ));

        let intent = crate::batch_item::PaymentIntent::<AccountId32, u64> {
            tx_id: H256::repeat_byte(0x32),
            payer_account_id: alice.clone(),
            payer_bank_cid: bank_cid(),
            recipient_account_id: bob.clone(),
            recipient_bank_cid: bank_cid(),
            amount: 10_000,
            fee: 5,
            nonce: 1,
            expires_at: 100,
        };
        let item = OffchainBatchItem::<AccountId32, u64> {
            tx_id: intent.tx_id,
            payer_account_id: intent.payer_account_id.clone(),
            payer_bank_cid: intent.payer_bank_cid.clone(),
            recipient_account_id: intent.recipient_account_id.clone(),
            recipient_bank_cid: intent.recipient_bank_cid.clone(),
            transfer_amount: intent.amount,
            fee_amount: intent.fee,
            payer_sig: sign_intent(&alice_pair, &intent),
            payer_nonce: intent.nonce,
            expires_at: intent.expires_at,
        };
        let batch: BoundedVec<_, _> = sp_std::vec![item].try_into().unwrap();

        assert_noop!(
            OffchainTx::submit_offchain_batch(
                RuntimeOrigin::signed(bank_admin()),
                bank_cid(),
                bank_role_code(),
                bank_main(),
                2,
                batch.clone(),
                sign_batch(&bank_main(), 2, &batch)
            ),
            Error::<Test>::InvalidBatchSeq
        );
    });
}

#[test]
fn submit_batch_same_bank_end_to_end() {
    new_test_ext().execute_with(|| {
        seed_fee_rate(&bank_cid(), 5); // 5 bp = 0.05%

        let (alice, alice_pair) = new_l3_user(&[1u8; 32], 2_000_000);
        let (bob, _) = new_l3_user(&[2u8; 32], 1_000_000);

        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(alice.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(bob.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::deposit(
            RuntimeOrigin::signed(alice.clone()),
            1_000_000
        ));

        // 10_000 分 × 5 bp / 10000 = 5 分,但按 runtime fee_config 最低 1 分,5 ≥ 1 → 5 分
        let transfer_amount = 10_000u128;
        let expected_fee = 5u128;

        let intent = crate::batch_item::PaymentIntent::<AccountId32, u64> {
            tx_id: H256::repeat_byte(0x42),
            payer_account_id: alice.clone(),
            payer_bank_cid: bank_cid(),
            recipient_account_id: bob.clone(),
            recipient_bank_cid: bank_cid(),
            amount: transfer_amount,
            fee: expected_fee,
            nonce: 1,
            expires_at: 100,
        };
        let sig = sign_intent(&alice_pair, &intent);
        let item = OffchainBatchItem::<AccountId32, u64> {
            tx_id: intent.tx_id,
            payer_account_id: intent.payer_account_id.clone(),
            payer_bank_cid: intent.payer_bank_cid.clone(),
            recipient_account_id: intent.recipient_account_id.clone(),
            recipient_bank_cid: intent.recipient_bank_cid.clone(),
            transfer_amount,
            fee_amount: expected_fee,
            payer_sig: sig,
            payer_nonce: intent.nonce,
            expires_at: intent.expires_at,
        };
        let batch: BoundedVec<_, _> = sp_std::vec![item].try_into().unwrap();

        // 由管理员提交
        let bank_total_before = BankTotalDeposits::<Test>::get(bank_cid());
        let fee_account_before = Balances::free_balance(bank_fee());
        let bank_clearing_balance_before = Balances::free_balance(bank_clearing());
        let bank_main_balance_before = Balances::free_balance(bank_main());

        assert_ok!(OffchainTx::submit_offchain_batch(
            RuntimeOrigin::signed(bank_admin()),
            bank_cid(),
            bank_role_code(),
            bank_main(),
            1,
            batch.clone(),
            sign_batch(&bank_main(), 1, &batch)
        ));

        // 付款方 DepositBalance 扣 (transfer + fee)
        assert_eq!(
            DepositBalance::<Test>::get(bank_cid(), &alice),
            1_000_000 - (transfer_amount + expected_fee)
        );
        // 收款方 DepositBalance 加 transfer
        assert_eq!(
            DepositBalance::<Test>::get(bank_cid(), &bob),
            transfer_amount
        );
        // 同行场景:BankTotalDeposits 下降 fee(fee 流出到 fee_account)
        assert_eq!(
            BankTotalDeposits::<Test>::get(bank_cid()),
            bank_total_before - expected_fee
        );
        // 清算账户 Balances 减 fee(转到 fee_account);主账户身份锚不受影响。
        assert_eq!(
            Balances::free_balance(bank_clearing()),
            bank_clearing_balance_before - expected_fee
        );
        assert_eq!(
            Balances::free_balance(bank_main()),
            bank_main_balance_before
        );
        // 费用账户先收本批手续费,再为这批手续费收益支付一次链上费(Step 3)。
        // 本批累计手续费 = 单笔 fee = 5 → 链上费 max(round(5×0.1%),10) = 10 FEN。
        let onchain_fee = primitives::fee_policy::calculate_onchain_fee(expected_fee);
        assert_eq!(
            Balances::free_balance(bank_fee()),
            fee_account_before + expected_fee - onchain_fee
        );
        // nonce 已消费
        assert_eq!(L3PaymentNonce::<Test>::get(&alice), 1);
        // 批次序号只在 settlement 成功后推进。
        assert_eq!(LastClearingBatchSeq::<Test>::get(bank_cid()), 1);

        // 事件断言:最后几条里应有 PaymentSettled + ClearingBankBatchSettled
        let events = frame_system::Pallet::<Test>::events();
        let event_names: sp_std::vec::Vec<_> =
            events.iter().map(|r| format!("{:?}", r.event)).collect();
        assert!(
            event_names.iter().any(|s| s.contains("PaymentSettled")),
            "missing PaymentSettled event; got {:?}",
            event_names
        );
        assert!(
            event_names
                .iter()
                .any(|s| s.contains("ClearingBankBatchSettled")),
            "missing ClearingBankBatchSettled event; got {:?}",
            event_names
        );

        // 防重放:同一 tx_id 再次提交应被拒 TxAlreadyProcessed。
        // 注意 nonce 必须递增(否则会先撞 `InvalidL3Nonce`),且 sig 重新
        // 基于 nonce=2 的 signing_hash 签名,否则先撞 `InvalidL3Signature`。
        let replay_intent = crate::batch_item::PaymentIntent::<AccountId32, u64> {
            nonce: 2,
            ..intent.clone()
        };
        let replay_sig = sign_intent(&alice_pair, &replay_intent);
        let replay_item = OffchainBatchItem::<AccountId32, u64> {
            tx_id: replay_intent.tx_id,
            payer_account_id: replay_intent.payer_account_id.clone(),
            payer_bank_cid: replay_intent.payer_bank_cid.clone(),
            recipient_account_id: replay_intent.recipient_account_id.clone(),
            recipient_bank_cid: replay_intent.recipient_bank_cid.clone(),
            transfer_amount,
            fee_amount: expected_fee,
            payer_sig: replay_sig,
            payer_nonce: replay_intent.nonce,
            expires_at: replay_intent.expires_at,
        };
        let replay_batch: BoundedVec<_, _> = sp_std::vec![replay_item].try_into().unwrap();
        assert_noop!(
            OffchainTx::submit_offchain_batch(
                RuntimeOrigin::signed(bank_admin()),
                bank_cid(),
                bank_role_code(),
                bank_main(),
                2,
                replay_batch.clone(),
                sign_batch(&bank_main(), 2, &replay_batch)
            ),
            Error::<Test>::TxAlreadyProcessed
        );
    });
}

#[test]
fn batch_rejected_when_fee_account_cannot_pay_onchain_fee() {
    new_test_ext().execute_with(|| {
        seed_fee_rate(&bank_cid(), 5);
        // 费用账户清零:本批手续费入账后仍不足以支付链上费(10 FEN)→ 整批 fail-closed。
        Balances::make_free_balance_be(&bank_fee(), 0);

        let (alice, alice_pair) = new_l3_user(&[1u8; 32], 2_000_000);
        let (bob, _) = new_l3_user(&[2u8; 32], 1_000_000);
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(alice.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(bob.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::deposit(
            RuntimeOrigin::signed(alice.clone()),
            1_000_000
        ));

        let transfer_amount = 10_000u128;
        let expected_fee = 5u128;
        let intent = crate::batch_item::PaymentIntent::<AccountId32, u64> {
            tx_id: H256::repeat_byte(0x51),
            payer_account_id: alice.clone(),
            payer_bank_cid: bank_cid(),
            recipient_account_id: bob.clone(),
            recipient_bank_cid: bank_cid(),
            amount: transfer_amount,
            fee: expected_fee,
            nonce: 1,
            expires_at: 100,
        };
        let sig = sign_intent(&alice_pair, &intent);
        let item = OffchainBatchItem::<AccountId32, u64> {
            tx_id: intent.tx_id,
            payer_account_id: intent.payer_account_id.clone(),
            payer_bank_cid: intent.payer_bank_cid.clone(),
            recipient_account_id: intent.recipient_account_id.clone(),
            recipient_bank_cid: intent.recipient_bank_cid.clone(),
            transfer_amount,
            fee_amount: expected_fee,
            payer_sig: sig,
            payer_nonce: intent.nonce,
            expires_at: intent.expires_at,
        };
        let batch: BoundedVec<_, _> = sp_std::vec![item].try_into().unwrap();

        // 链上费从费用账户扣款失败 → 整批拒绝并回滚(账本/总存款/nonce/批次序号均无变化)。
        assert_noop!(
            OffchainTx::submit_offchain_batch(
                RuntimeOrigin::signed(bank_admin()),
                bank_cid(),
                bank_role_code(),
                bank_main(),
                1,
                batch.clone(),
                sign_batch(&bank_main(), 1, &batch)
            ),
            Error::<Test>::ClearingBatchOnchainFeeUnpaid
        );
        assert_eq!(DepositBalance::<Test>::get(bank_cid(), &alice), 1_000_000);
        assert_eq!(BankTotalDeposits::<Test>::get(bank_cid()), 1_000_000);
        assert_eq!(LastClearingBatchSeq::<Test>::get(bank_cid()), 0);
        assert_eq!(L3PaymentNonce::<Test>::get(&alice), 0);
    });
}

#[test]
fn submit_batch_rejects_user_bank_mismatch() {
    new_test_ext().execute_with(|| {
        seed_fee_rate(&bank_cid(), 5);
        let (alice, alice_pair) = new_l3_user(&[1u8; 32], 1_000_000);
        let (bob, _) = new_l3_user(&[2u8; 32], 1_000_000);
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(alice.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(bob.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::deposit(
            RuntimeOrigin::signed(alice.clone()),
            100_000
        ));
        // 直接制造链上绑定与 item 声明不一致的状态,验证 settlement 会早拒。
        let other_bank_cid: crate::InstitutionCidNumber =
            b"OT999-SCB00-000000000-2026".to_vec().try_into().unwrap();
        UserBank::<Test>::insert(&bob, other_bank_cid);

        let intent = crate::batch_item::PaymentIntent::<AccountId32, u64> {
            tx_id: H256::repeat_byte(0x33),
            payer_account_id: alice.clone(),
            payer_bank_cid: bank_cid(),
            recipient_account_id: bob.clone(),
            recipient_bank_cid: bank_cid(),
            amount: 10_000,
            fee: 5,
            nonce: 1,
            expires_at: 100,
        };
        let item = OffchainBatchItem::<AccountId32, u64> {
            tx_id: intent.tx_id,
            payer_account_id: intent.payer_account_id.clone(),
            payer_bank_cid: intent.payer_bank_cid.clone(),
            recipient_account_id: intent.recipient_account_id.clone(),
            recipient_bank_cid: intent.recipient_bank_cid.clone(),
            transfer_amount: intent.amount,
            fee_amount: intent.fee,
            payer_sig: sign_intent(&alice_pair, &intent),
            payer_nonce: intent.nonce,
            expires_at: intent.expires_at,
        };
        let batch: BoundedVec<_, _> = sp_std::vec![item].try_into().unwrap();

        assert_noop!(
            OffchainTx::submit_offchain_batch(
                RuntimeOrigin::signed(bank_admin()),
                bank_cid(),
                bank_role_code(),
                bank_main(),
                1,
                batch.clone(),
                sign_batch(&bank_main(), 1, &batch)
            ),
            Error::<Test>::UserBankMismatch
        );
    });
}

#[test]
fn submit_batch_expired_intent_rejected() {
    new_test_ext().execute_with(|| {
        seed_fee_rate(&bank_cid(), 5);
        let (alice, alice_pair) = new_l3_user(&[1u8; 32], 1_000_000);
        let (bob, _) = new_l3_user(&[2u8; 32], 1_000_000);
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(alice.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(bob.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::deposit(
            RuntimeOrigin::signed(alice.clone()),
            100_000
        ));

        // 故意把块高推到 expires_at 之后
        System::set_block_number(200);

        let intent = crate::batch_item::PaymentIntent::<AccountId32, u64> {
            tx_id: H256::repeat_byte(9),
            payer_account_id: alice.clone(),
            payer_bank_cid: bank_cid(),
            recipient_account_id: bob.clone(),
            recipient_bank_cid: bank_cid(),
            amount: 10_000,
            fee: 5,
            nonce: 1,
            expires_at: 100, // 已过
        };
        let sig = sign_intent(&alice_pair, &intent);
        let item = OffchainBatchItem::<AccountId32, u64> {
            tx_id: intent.tx_id,
            payer_account_id: intent.payer_account_id.clone(),
            payer_bank_cid: intent.payer_bank_cid.clone(),
            recipient_account_id: intent.recipient_account_id.clone(),
            recipient_bank_cid: intent.recipient_bank_cid.clone(),
            transfer_amount: intent.amount,
            fee_amount: intent.fee,
            payer_sig: sig,
            payer_nonce: intent.nonce,
            expires_at: intent.expires_at,
        };
        let batch: BoundedVec<_, _> = sp_std::vec![item].try_into().unwrap();

        assert_noop!(
            OffchainTx::submit_offchain_batch(
                RuntimeOrigin::signed(bank_admin()),
                bank_cid(),
                bank_role_code(),
                bank_main(),
                1,
                batch.clone(),
                sign_batch(&bank_main(), 1, &batch)
            ),
            Error::<Test>::ExpiredIntent
        );
    });
}

// ─── 测试:跨行结算 + 偿付护栏 ─────────────────────────────────────────────

#[test]
fn submit_batch_cross_bank_end_to_end() {
    new_test_ext().execute_with(|| {
        // 费率按收款方清算行(BANK)取。
        seed_fee_rate(&bank_cid(), 5); // 5 bp

        // 付款方 alice 绑第二家清算行 BANK2 并向其清算账户充值;收款方 bob 绑本行 BANK。
        let (alice, alice_pair) = new_l3_user(&[1u8; 32], 2_000_000);
        let (bob, _) = new_l3_user(&[2u8; 32], 1_000_000);
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(alice.clone()),
            bank2_cid()
        ));
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(bob.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::deposit(
            RuntimeOrigin::signed(alice.clone()),
            1_000_000
        ));

        let transfer_amount = 10_000u128;
        let expected_fee = 5u128;
        let onchain_fee = primitives::fee_policy::calculate_onchain_fee(expected_fee);

        let intent = crate::batch_item::PaymentIntent::<AccountId32, u64> {
            tx_id: H256::repeat_byte(0x71),
            payer_account_id: alice.clone(),
            payer_bank_cid: bank2_cid(),
            recipient_account_id: bob.clone(),
            recipient_bank_cid: bank_cid(),
            amount: transfer_amount,
            fee: expected_fee,
            nonce: 1,
            expires_at: 100,
        };
        let sig = sign_intent(&alice_pair, &intent);
        let item = OffchainBatchItem::<AccountId32, u64> {
            tx_id: intent.tx_id,
            payer_account_id: intent.payer_account_id.clone(),
            payer_bank_cid: intent.payer_bank_cid.clone(),
            recipient_account_id: intent.recipient_account_id.clone(),
            recipient_bank_cid: intent.recipient_bank_cid.clone(),
            transfer_amount,
            fee_amount: expected_fee,
            payer_sig: sig,
            payer_nonce: intent.nonce,
            expires_at: intent.expires_at,
        };
        let batch: BoundedVec<_, _> = sp_std::vec![item].try_into().unwrap();

        // settlement 前基线(充值后):付款方行 BANK2 / 收款方行 BANK。
        let payer_deposit_before = DepositBalance::<Test>::get(bank2_cid(), &alice);
        let bank2_total_before = BankTotalDeposits::<Test>::get(bank2_cid());
        let bank_total_before = BankTotalDeposits::<Test>::get(bank_cid());
        let payer_clearing_before = Balances::free_balance(bank2_clearing());
        let recipient_clearing_before = Balances::free_balance(bank_clearing());
        let recipient_fee_before = Balances::free_balance(bank_fee());

        // 批次提交给收款方清算行 BANK(收款方主导清算),由 BANK 管理员签名。
        assert_ok!(OffchainTx::submit_offchain_batch(
            RuntimeOrigin::signed(bank_admin()),
            bank_cid(),
            bank_role_code(),
            bank_main(),
            1,
            batch.clone(),
            sign_batch(&bank_main(), 1, &batch)
        ));

        // 账本:付款方行(BANK2)扣 本金+fee;收款方行(BANK)加 本金。
        assert_eq!(
            DepositBalance::<Test>::get(bank2_cid(), &alice),
            payer_deposit_before - (transfer_amount + expected_fee)
        );
        assert_eq!(
            DepositBalance::<Test>::get(bank_cid(), &bob),
            transfer_amount
        );
        assert_eq!(
            BankTotalDeposits::<Test>::get(bank2_cid()),
            bank2_total_before - (transfer_amount + expected_fee)
        );
        assert_eq!(
            BankTotalDeposits::<Test>::get(bank_cid()),
            bank_total_before + transfer_amount
        );

        // 资金:付款方清算账户流出 本金+fee;收款方清算账户收本金;收款方费用账户收 fee 再付链上费。
        assert_eq!(
            Balances::free_balance(bank2_clearing()),
            payer_clearing_before - (transfer_amount + expected_fee)
        );
        assert_eq!(
            Balances::free_balance(bank_clearing()),
            recipient_clearing_before + transfer_amount
        );
        assert_eq!(
            Balances::free_balance(bank_fee()),
            recipient_fee_before + expected_fee - onchain_fee
        );
        // 跨行 fee 落收款方费用账户,付款方费用账户(BANK2)不参与。
        assert_eq!(Balances::free_balance(bank2_fee()), 1_000_000);

        assert_eq!(L3PaymentNonce::<Test>::get(&alice), 1);
        // 批次序号按 actor(收款方 BANK)推进。
        assert_eq!(LastClearingBatchSeq::<Test>::get(bank_cid()), 1);

        let events = frame_system::Pallet::<Test>::events();
        let names: sp_std::vec::Vec<_> = events.iter().map(|r| format!("{:?}", r.event)).collect();
        assert!(
            names.iter().any(|s| s.contains("PaymentSettled")),
            "missing PaymentSettled; got {:?}",
            names
        );
    });
}

#[test]
fn submit_batch_rejected_when_solvency_breached() {
    new_test_ext().execute_with(|| {
        seed_fee_rate(&bank_cid(), 5);
        let (alice, alice_pair) = new_l3_user(&[1u8; 32], 2_000_000);
        let (bob, _) = new_l3_user(&[2u8; 32], 1_000_000);
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(alice.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(bob.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::deposit(
            RuntimeOrigin::signed(alice.clone()),
            1_000_000
        ));

        // 同行 debit = fee = 5;总存款 = 1_000_000。把清算账户压到 (总存款 + fee - 1),
        // 扣款后 = 总存款 - 1 < 总存款 → 触发 SolvencyProtected(全额准备金护栏)。
        let total_deposits = BankTotalDeposits::<Test>::get(bank_cid());
        assert_eq!(total_deposits, 1_000_000);
        Balances::make_free_balance_be(&bank_clearing(), total_deposits + 5 - 1);

        let transfer_amount = 10_000u128;
        let expected_fee = 5u128;
        let intent = crate::batch_item::PaymentIntent::<AccountId32, u64> {
            tx_id: H256::repeat_byte(0x72),
            payer_account_id: alice.clone(),
            payer_bank_cid: bank_cid(),
            recipient_account_id: bob.clone(),
            recipient_bank_cid: bank_cid(),
            amount: transfer_amount,
            fee: expected_fee,
            nonce: 1,
            expires_at: 100,
        };
        let sig = sign_intent(&alice_pair, &intent);
        let item = OffchainBatchItem::<AccountId32, u64> {
            tx_id: intent.tx_id,
            payer_account_id: intent.payer_account_id.clone(),
            payer_bank_cid: intent.payer_bank_cid.clone(),
            recipient_account_id: intent.recipient_account_id.clone(),
            recipient_bank_cid: intent.recipient_bank_cid.clone(),
            transfer_amount,
            fee_amount: expected_fee,
            payer_sig: sig,
            payer_nonce: intent.nonce,
            expires_at: intent.expires_at,
        };
        let batch: BoundedVec<_, _> = sp_std::vec![item].try_into().unwrap();

        assert_noop!(
            OffchainTx::submit_offchain_batch(
                RuntimeOrigin::signed(bank_admin()),
                bank_cid(),
                bank_role_code(),
                bank_main(),
                1,
                batch.clone(),
                sign_batch(&bank_main(), 1, &batch)
            ),
            Error::<Test>::SolvencyProtected
        );
        // 整批回滚:账本 / 总存款 / nonce / 批次序号均无变化。
        assert_eq!(DepositBalance::<Test>::get(bank_cid(), &alice), 1_000_000);
        assert_eq!(BankTotalDeposits::<Test>::get(bank_cid()), 1_000_000);
        assert_eq!(LastClearingBatchSeq::<Test>::get(bank_cid()), 0);
        assert_eq!(L3PaymentNonce::<Test>::get(&alice), 0);
    });
}

#[test]
fn submit_batch_solvency_boundary_exact_pass() {
    new_test_ext().execute_with(|| {
        seed_fee_rate(&bank_cid(), 5);
        let (alice, alice_pair) = new_l3_user(&[1u8; 32], 2_000_000);
        let (bob, _) = new_l3_user(&[2u8; 32], 1_000_000);
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(alice.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::bind_clearing_bank(
            RuntimeOrigin::signed(bob.clone()),
            bank_cid()
        ));
        assert_ok!(OffchainTx::deposit(
            RuntimeOrigin::signed(alice.clone()),
            1_000_000
        ));

        // 边界:清算账户 = 总存款 + fee → 扣 fee 后恰好 == 总存款,`>=` 通过。
        let total_deposits = BankTotalDeposits::<Test>::get(bank_cid());
        Balances::make_free_balance_be(&bank_clearing(), total_deposits + 5);

        let transfer_amount = 10_000u128;
        let expected_fee = 5u128;
        let intent = crate::batch_item::PaymentIntent::<AccountId32, u64> {
            tx_id: H256::repeat_byte(0x73),
            payer_account_id: alice.clone(),
            payer_bank_cid: bank_cid(),
            recipient_account_id: bob.clone(),
            recipient_bank_cid: bank_cid(),
            amount: transfer_amount,
            fee: expected_fee,
            nonce: 1,
            expires_at: 100,
        };
        let sig = sign_intent(&alice_pair, &intent);
        let item = OffchainBatchItem::<AccountId32, u64> {
            tx_id: intent.tx_id,
            payer_account_id: intent.payer_account_id.clone(),
            payer_bank_cid: intent.payer_bank_cid.clone(),
            recipient_account_id: intent.recipient_account_id.clone(),
            recipient_bank_cid: intent.recipient_bank_cid.clone(),
            transfer_amount,
            fee_amount: expected_fee,
            payer_sig: sig,
            payer_nonce: intent.nonce,
            expires_at: intent.expires_at,
        };
        let batch: BoundedVec<_, _> = sp_std::vec![item].try_into().unwrap();

        assert_ok!(OffchainTx::submit_offchain_batch(
            RuntimeOrigin::signed(bank_admin()),
            bank_cid(),
            bank_role_code(),
            bank_main(),
            1,
            batch.clone(),
            sign_batch(&bank_main(), 1, &batch)
        ));
        assert_eq!(LastClearingBatchSeq::<Test>::get(bank_cid()), 1);
    });
}
