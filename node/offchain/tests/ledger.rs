use super::*;

use crate::ledger::PendingPayment;
use offchain::batch_item::PaymentIntent;
use sp_core::sr25519;
use sp_core::Pair;
use sp_runtime::AccountId32;

fn acc(b: u8) -> AccountId32 {
    AccountId32::new([b; 32])
}

fn signed_intent(
    pair: &sr25519::Pair,
    tx_id: u8,
    payer_bank_cid: Vec<u8>,
    recipient_bank_cid: Vec<u8>,
    nonce: u64,
) -> (PaymentIntent<AccountId32, u32>, [u8; 64]) {
    let intent = PaymentIntent {
        tx_id: H256::repeat_byte(tx_id),
        payer_account_id: AccountId32::new(pair.public().0),
        payer_bank_cid: payer_bank_cid.try_into().expect("测试CID在原链边界内"),
        recipient_account_id: acc(0x22),
        recipient_bank_cid: recipient_bank_cid.try_into().expect("测试CID在原链边界内"),
        amount: 99,
        fee: 1,
        nonce,
        expires_at: 100,
    };
    let sig = <sr25519::Pair as Pair>::sign(pair, &intent.signing_hash());
    (intent, sig.0)
}

#[test]
fn pending_intent_preserves_flat_scale_layout() {
    let pair = <sr25519::Pair as Pair>::from_seed(&[0x41u8; 32]);
    let (intent, payer_sig) = signed_intent(&pair, 1, b"bank-a".to_vec(), b"bank-b".to_vec(), 7);
    let accepted_at = 1_717_000_000u64;
    // 原持久化字段序金标；只验证嵌入公开类型不改变既有有效记录字节。
    let mut golden = intent.tx_id.encode();
    golden.extend(intent.payer_account_id.encode());
    golden.extend(intent.payer_bank_cid.encode());
    golden.extend(intent.recipient_account_id.encode());
    golden.extend(intent.recipient_bank_cid.encode());
    golden.extend(intent.amount.encode());
    golden.extend(intent.fee.encode());
    golden.extend(intent.nonce.encode());
    golden.extend(intent.expires_at.encode());
    golden.extend(payer_sig.encode());
    golden.extend(accepted_at.encode());
    let pending = PendingPayment {
        intent: intent.clone(),
        payer_sig,
        accepted_at,
    };
    assert_eq!(pending.encode(), golden);
    let decoded = crate::ledger::PendingPayment::decode(&mut &golden[..]).unwrap();
    assert_eq!(decoded.intent, intent);
    assert_eq!(decoded.payer_sig, payer_sig);
    assert_eq!(decoded.accepted_at, accepted_at);
}

#[test]
fn accept_payment_rejects_signature_from_another_operation_tag() {
    let tmp = std::env::temp_dir().join("offchain_ledger_cross_operation_signature_test");
    let _ = fs::remove_dir_all(&tmp);
    let ledger = crate::ledger::OffchainLedger::new(&tmp);
    let pair = <sr25519::Pair as Pair>::from_seed(&[0x41u8; 32]);
    let payer = AccountId32::new(pair.public().0);
    let bank_cid = b"GD001-PRB0T-239565809-2026".to_vec();
    ledger.on_deposited(&payer, 1_000);
    let (intent, _) = signed_intent(&pair, 0x10, bank_cid.clone(), bank_cid, 1);
    let wrong_message = primitives::sign::signing_message(
        primitives::sign::OP_SIGN_OFFCHAIN_BATCH,
        &intent.encode(),
    );
    let wrong_signature = <sr25519::Pair as Pair>::sign(&pair, &wrong_message).0;

    let error = ledger
        .accept_payment(intent, wrong_signature, Some(1), [7u8; 64], 1_717_000_000)
        .expect_err("其它操作码签名不能重放为 L3 支付授权");
    assert!(error.contains("L3 sr25519 签名验证失败"));
    assert_eq!(ledger.pending_count(), 0);
    assert_eq!(ledger.next_nonce(&payer), 1);
    assert_eq!(ledger.available_balance(&payer), 1_000);
}

#[test]
fn deposited_then_withdrawn_roundtrip() {
    let tmp = std::env::temp_dir().join("offchain_ledger_roundtrip_test");
    let _ = fs::remove_dir_all(&tmp);
    let ledger = crate::ledger::OffchainLedger::new(&tmp);

    ledger.on_deposited(&acc(1), 1000);
    assert_eq!(ledger.available_balance(&acc(1)), 1000);
    ledger.on_withdrawn(&acc(1), 200);
    assert_eq!(ledger.available_balance(&acc(1)), 800);
}

#[test]
fn save_load_roundtrip() {
    let tmp = std::env::temp_dir().join("offchain_ledger_saveload_test");
    let _ = fs::remove_dir_all(&tmp);
    let ledger = crate::ledger::OffchainLedger::new(&tmp);
    ledger.on_deposited(&acc(7), 500);
    ledger.save_to_disk("passwd").unwrap();

    let ledger2 = crate::ledger::OffchainLedger::new(&tmp);
    ledger2.load_from_disk("passwd").unwrap();
    assert_eq!(ledger2.available_balance(&acc(7)), 500);
}

#[test]
fn wrong_password_rejected() {
    let tmp = std::env::temp_dir().join("offchain_ledger_password_test");
    let _ = fs::remove_dir_all(&tmp);
    let ledger = crate::ledger::OffchainLedger::new(&tmp);
    ledger.on_deposited(&acc(9), 123);
    ledger.save_to_disk("right").unwrap();

    let ledger2 = crate::ledger::OffchainLedger::new(&tmp);
    assert!(ledger2.load_from_disk("wrong").is_err());
}

#[test]
fn settled_same_bank_moves_pending_to_confirmed() {
    let tmp = std::env::temp_dir().join("offchain_ledger_settled_same_bank_test");
    let _ = fs::remove_dir_all(&tmp);
    let ledger = crate::ledger::OffchainLedger::new(&tmp);
    let my_bank: Vec<u8> = b"ZS001-PRB08-233384677-2026".to_vec();
    ledger.on_deposited(&acc(1), 1000);
    // 模拟本地扣款 pending_debit
    {
        let mut inner = ledger.inner.write().unwrap();
        let st = inner.accounts.entry(acc(1)).or_default();
        st.pending_debit = 100;
        // 模拟 B 的状态不存在,settled 时会创建(同行)
    }
    ledger.on_payment_settled(
        H256::repeat_byte(1),
        &acc(1),
        &my_bank,
        &acc(2),
        &my_bank,
        &my_bank,
        99,
        1,
    );
    // A:confirmed 1000 - 100 = 900,pending_debit 归 0
    assert_eq!(ledger.available_balance(&acc(1)), 900);
    // B:confirmed 99
    assert_eq!(ledger.available_balance(&acc(2)), 99);
}

#[test]
fn settled_cross_bank_payer_side_only_no_ghost_recipient() {
    // 跨行场景:my_bank == payer_bank != recipient_bank。
    // 修复前:recipient 在本地 accounts 会被误建 ghost 账户 confirmed=amount。
    // 修复后:只动 payer,recipient 不出现在本地 accounts。
    let tmp = std::env::temp_dir().join("offchain_ledger_settled_cross_payer_test");
    let _ = fs::remove_dir_all(&tmp);
    let ledger = crate::ledger::OffchainLedger::new(&tmp);
    let my_bank: Vec<u8> = b"ZS001-PRB08-233384677-2026".to_vec();
    let other_bank: Vec<u8> = b"AH001-PRB0X-111111111-2026".to_vec();
    ledger.on_deposited(&acc(1), 1000);
    {
        let mut inner = ledger.inner.write().unwrap();
        let st = inner.accounts.entry(acc(1)).or_default();
        st.pending_debit = 100;
    }
    ledger.on_payment_settled(
        H256::repeat_byte(2),
        &acc(1),
        &my_bank,
        &acc(2),
        &other_bank, // 收款方在别行
        &my_bank,
        99,
        1,
    );
    // A:正常扣减
    assert_eq!(ledger.available_balance(&acc(1)), 900);
    // B 不应有 ghost 账户
    let inner = ledger.inner.read().unwrap();
    assert!(
        !inner.accounts.contains_key(&acc(2)),
        "recipient 不在本行,不应被创建 ghost 账户"
    );
}

#[test]
fn settled_cross_bank_recipient_side_only() {
    // 跨行场景:my_bank == recipient_bank != payer_bank。
    // 只动 recipient,payer 不在本行不动。
    let tmp = std::env::temp_dir().join("offchain_ledger_settled_cross_recipient_test");
    let _ = fs::remove_dir_all(&tmp);
    let ledger = crate::ledger::OffchainLedger::new(&tmp);
    let my_bank: Vec<u8> = b"ZS001-PRB08-233384677-2026".to_vec();
    let other_bank: Vec<u8> = b"AH001-PRB0X-111111111-2026".to_vec();
    // 付款方 A 在 other_bank,但 A 在本行 ledger 里也有历史状态(不该被错扣)
    ledger.on_deposited(&acc(1), 500); // 本行里 A 的旧余额(比如用户曾经绑过我们)
    ledger.on_payment_settled(
        H256::repeat_byte(3),
        &acc(1),
        &other_bank, // 付款方在别行
        &acc(2),
        &my_bank,
        &my_bank,
        99,
        1,
    );
    // A 在本行的 confirmed 不动(非本行 payer)
    assert_eq!(ledger.available_balance(&acc(1)), 500);
    // B 新建 + 加 99
    assert_eq!(ledger.available_balance(&acc(2)), 99);
}

#[test]
fn settled_sum_invariant_same_bank_drops_by_fee() {
    // 同行场景对账不变式:settle 后本地 `Σ confirmed` 变化 = -fee。
    let tmp = std::env::temp_dir().join("offchain_ledger_sum_invariant_test");
    let _ = fs::remove_dir_all(&tmp);
    let ledger = crate::ledger::OffchainLedger::new(&tmp);
    let my_bank: Vec<u8> = b"ZS001-PRB08-233384677-2026".to_vec();
    ledger.on_deposited(&acc(1), 1000);
    {
        let mut inner = ledger.inner.write().unwrap();
        inner.accounts.entry(acc(1)).or_default().pending_debit = 100;
    }
    let before = ledger.confirmed_sum_snapshot();
    ledger.on_payment_settled(
        H256::repeat_byte(4),
        &acc(1),
        &my_bank,
        &acc(2),
        &my_bank,
        &my_bank,
        99, // amount
        1,  // fee
    );
    let after = ledger.confirmed_sum_snapshot();
    assert_eq!(after, before.saturating_sub(1), "Σ confirmed 应减 fee=1");
}

#[test]
fn accept_cross_bank_recipient_uses_chain_balance_without_ghost_payer() {
    // 跨行扫码由收款方清算节点接受 pending:付款方余额与 nonce 来自链上,
    // 本地 ledger 不应该创建付款方账户,否则 reserve 对账会被污染。
    let tmp = std::env::temp_dir().join("offchain_ledger_accept_cross_recipient_test");
    let _ = fs::remove_dir_all(&tmp);
    let ledger = crate::ledger::OffchainLedger::new(&tmp);
    let pair = <sr25519::Pair as Pair>::from_seed(&[0x42u8; 32]);
    let payer = AccountId32::new(pair.public().0);
    let payer_bank: Vec<u8> = b"GD001-PRB0T-239565809-2026".to_vec();
    let recipient_bank: Vec<u8> = b"AH001-PRB0X-111111111-2026".to_vec();

    let (intent1, sig1) = signed_intent(&pair, 0x11, payer_bank.clone(), recipient_bank.clone(), 1);
    let (tx1, _) = ledger
        .accept_payment_with_chain_state(
            intent1,
            sig1,
            Some(1),
            [7u8; 64],
            1_717_000_000,
            &recipient_bank,
            Some(1_000),
            Some(0),
        )
        .unwrap();

    assert_eq!(tx1, H256::repeat_byte(0x11));
    assert_eq!(ledger.pending_count(), 1);
    assert_eq!(ledger.next_nonce(&payer), 2);
    let inner = ledger.inner.read().unwrap();
    assert!(
        !inner.accounts.contains_key(&payer),
        "跨行收款方节点不能创建付款方 ghost 账户"
    );
}

#[test]
fn accept_cross_bank_recipient_nonce_counts_local_pending() {
    // 链上 nonce 尚未更新前,第二笔跨行 pending 应接在第一笔后面。
    let tmp = std::env::temp_dir().join("offchain_ledger_accept_cross_nonce_test");
    let _ = fs::remove_dir_all(&tmp);
    let ledger = crate::ledger::OffchainLedger::new(&tmp);
    let pair = <sr25519::Pair as Pair>::from_seed(&[0x43u8; 32]);
    let payer_bank: Vec<u8> = b"GD001-PRB0T-239565809-2026".to_vec();
    let recipient_bank: Vec<u8> = b"AH001-PRB0X-111111111-2026".to_vec();

    let (intent1, sig1) = signed_intent(&pair, 0x21, payer_bank.clone(), recipient_bank.clone(), 1);
    ledger
        .accept_payment_with_chain_state(
            intent1,
            sig1,
            Some(1),
            [7u8; 64],
            1_717_000_000,
            &recipient_bank,
            Some(1_000),
            Some(0),
        )
        .unwrap();

    let (intent2, sig2) = signed_intent(&pair, 0x22, payer_bank.clone(), recipient_bank.clone(), 2);
    ledger
        .accept_payment_with_chain_state(
            intent2,
            sig2,
            Some(1),
            [8u8; 64],
            1_717_000_001,
            &recipient_bank,
            Some(1_000),
            Some(0),
        )
        .unwrap();
    assert_eq!(ledger.pending_count(), 2);

    let (intent_bad, sig_bad) = signed_intent(&pair, 0x23, payer_bank, recipient_bank.clone(), 2);
    let err = ledger
        .accept_payment_with_chain_state(
            intent_bad,
            sig_bad,
            Some(1),
            [9u8; 64],
            1_717_000_002,
            &recipient_bank,
            Some(1_000),
            Some(0),
        )
        .unwrap_err();
    assert!(
        err.contains("L3 nonce 错位"),
        "重复 nonce 应被本地 pending 早拒:{err}"
    );
}
