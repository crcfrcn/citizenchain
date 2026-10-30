use super::*;

use crate::ledger::OffchainLedger;
use crate::ledger::PendingPayment;
use crate::packer::BatchSigner;
use crate::packer::BatchSubmitter;
use crate::packer::NoopBatchSigner;
use crate::packer::NoopBatchSubmitter;
use offchain::batch_item::batch_signing_hash;
use sp_core::H256;
use sp_runtime::AccountId32;
use std::fs;
use std::sync::Arc;
use std::sync::Mutex;

fn acc(b: u8) -> AccountId32 {
    AccountId32::new([b; 32])
}

fn cid() -> Vec<u8> {
    b"GD001-SCB05-000000001-2026".to_vec()
}

fn role_code() -> Vec<u8> {
    b"CLEARING_OPERATOR".to_vec()
}

fn mk_ledger() -> Arc<OffchainLedger> {
    let tmp = std::env::temp_dir().join("offchain_packer_test");
    let _ = fs::remove_dir_all(&tmp);
    Arc::new(crate::ledger::OffchainLedger::new(&tmp))
}

fn seed_pending(ledger: &OffchainLedger, tx_byte: u8, payer: AccountId32) {
    // 直接操纵 ledger.inner 注入一条 pending,避免 accept_payment 的签名校验
    let mut inner = ledger.inner.write().unwrap();
    inner.pending.push(PendingPayment {
        intent: offchain::batch_item::PaymentIntent {
            tx_id: H256::repeat_byte(tx_byte),
            payer_account_id: payer.clone(),
            payer_bank_cid: b"ZS001-PRB08-233384677-2026".to_vec().try_into().unwrap(),
            recipient_account_id: acc(9),
            recipient_bank_cid: b"ZS001-PRB08-233384677-2026".to_vec().try_into().unwrap(),
            amount: 1000,
            fee: 1,
            nonce: 1,
            expires_at: 100,
        },
        payer_sig: [0u8; 64],
        accepted_at: tx_byte as u64,
    });
    inner.accepted_tx_ids.insert(H256::repeat_byte(tx_byte));
    let state = inner.accounts.entry(payer).or_default();
    state.confirmed = 2000;
    state.pending_debit = 1001;
    state.cached_nonce = 1;
}

#[allow(clippy::type_complexity)]
struct MockSubmitter {
    calls: Mutex<Vec<(Vec<u8>, Vec<u8>, AccountId32, u64, Vec<u8>, [u8; 64])>>,
    reply: Mutex<Result<H256, String>>,
}

impl BatchSubmitter for MockSubmitter {
    fn submit(
        &self,
        actor_cid_number: Vec<u8>,
        actor_role_code: Vec<u8>,
        institution_account_id: AccountId32,
        batch_seq: u64,
        batch_bytes: Vec<u8>,
        batch_signature: [u8; 64],
    ) -> Result<H256, String> {
        self.calls.lock().unwrap().push((
            actor_cid_number,
            actor_role_code,
            institution_account_id,
            batch_seq,
            batch_bytes,
            batch_signature,
        ));
        let g = self.reply.lock().unwrap();
        g.clone()
    }
}

struct MockSigner {
    reply: Result<[u8; 64], String>,
}

impl BatchSigner for MockSigner {
    fn sign_batch(&self, _message: &[u8]) -> Result<[u8; 64], String> {
        self.reply.clone()
    }
}

#[tokio::test]
async fn should_pack_is_false_when_empty() {
    let packer = crate::packer::OffchainPacker::new(
        mk_ledger(),
        cid(),
        role_code(),
        acc(0xAA),
        Arc::new(NoopBatchSigner),
        Arc::new(NoopBatchSubmitter),
    );
    assert!(!packer.should_pack(100).await);
}

#[tokio::test]
async fn noop_signer_triggers_rollback() {
    let ledger = mk_ledger();
    seed_pending(&ledger, 1, acc(1));
    let packer = crate::packer::OffchainPacker::new(
        ledger.clone(),
        cid(),
        role_code(),
        acc(0xAA),
        Arc::new(NoopBatchSigner),
        Arc::new(NoopBatchSubmitter),
    );
    let result = packer.pack_and_submit(50).await;
    assert!(result.is_err());
    // rollback 成功后 pending 被清空(因为 reject_pending 会剔除)
    assert_eq!(ledger.pending_count(), 0);
}

#[tokio::test]
async fn happy_path_submits_and_keeps_pending() {
    let ledger = mk_ledger();
    seed_pending(&ledger, 2, acc(2));

    let submitter = Arc::new(MockSubmitter {
        calls: Mutex::new(Vec::new()),
        reply: Mutex::new(Ok(H256::repeat_byte(0xFF))),
    });
    let signer = Arc::new(MockSigner {
        reply: Ok([7u8; 64]),
    });

    let packer = crate::packer::OffchainPacker::new(
        ledger.clone(),
        cid(),
        role_code(),
        acc(0xAA),
        signer.clone(),
        submitter.clone(),
    );

    let result = packer.pack_and_submit(100).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(H256::repeat_byte(0xFF)));

    // pending 没有被 reject,保留等 PaymentSettled 事件清理
    assert_eq!(ledger.pending_count(), 1);
    // submitter 收到正确参数
    let calls = submitter.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, cid());
    assert_eq!(calls[0].1, role_code());
    assert_eq!(calls[0].2, acc(0xAA));
    assert_eq!(calls[0].3, 1);
    assert_eq!(calls[0].5, [7u8; 64]);
}

#[tokio::test]
async fn initial_batch_seq_continues_from_chain_value() {
    let ledger = mk_ledger();
    seed_pending(&ledger, 4, acc(4));
    let submitter = Arc::new(MockSubmitter {
        calls: Mutex::new(Vec::new()),
        reply: Mutex::new(Ok(H256::repeat_byte(0xEE))),
    });
    let signer = Arc::new(MockSigner {
        reply: Ok([8u8; 64]),
    });
    let packer = crate::packer::OffchainPacker::new_with_initial_seq(
        ledger,
        cid(),
        role_code(),
        acc(0xAA),
        signer,
        submitter.clone(),
        41,
    );

    assert_eq!(
        packer.pack_and_submit(100).await.unwrap(),
        Some(H256::repeat_byte(0xEE))
    );
    let calls = submitter.calls.lock().unwrap();
    assert_eq!(calls[0].3, 42);
}

#[tokio::test]
async fn submitter_error_rolls_back() {
    let ledger = mk_ledger();
    seed_pending(&ledger, 3, acc(3));
    let submitter = Arc::new(MockSubmitter {
        calls: Mutex::new(Vec::new()),
        reply: Mutex::new(Err("tx pool 拒绝".to_string())),
    });
    let signer = Arc::new(MockSigner {
        reply: Ok([1u8; 64]),
    });
    let packer = crate::packer::OffchainPacker::new(
        ledger.clone(),
        cid(),
        role_code(),
        acc(0xAA),
        signer,
        submitter,
    );

    let r = packer.pack_and_submit(200).await;
    assert!(r.is_err());
    // 回滚后 pending 被剔除
    assert_eq!(ledger.pending_count(), 0);
}

#[test]
fn batch_signing_hash_is_deterministic() {
    let inst = acc(0xAA);
    let bytes = vec![1u8, 2, 3, 4];
    let h1 = batch_signing_hash(&cid(), &role_code(), &inst, 42, &bytes);
    let h2 = batch_signing_hash(&cid(), &role_code(), &inst, 42, &bytes);
    assert_eq!(h1, h2);

    // 改任意输入都影响哈希
    let h3 = batch_signing_hash(&cid(), &role_code(), &inst, 43, &bytes);
    assert_ne!(h1, h3);
    let h4 = batch_signing_hash(&cid(), &role_code(), &acc(0xAB), 42, &bytes);
    assert_ne!(h1, h4);
    let h5 = batch_signing_hash(
        b"GD001-SCB05-000000002-2026",
        &role_code(),
        &inst,
        42,
        &bytes,
    );
    assert_ne!(h1, h5);
    let h6 = batch_signing_hash(&cid(), b"OTHER_ROLE", &inst, 42, &bytes);
    assert_ne!(h1, h6);
}

#[test]
fn node_batch_item_encodes_deterministically() {
    let item = OffchainBatchItem::<AccountId32, u32> {
        tx_id: H256::repeat_byte(5),
        payer_account_id: acc(1),
        payer_bank_cid: b"GD001-PRB0T-239565809-2026".to_vec().try_into().unwrap(),
        recipient_account_id: acc(3),
        recipient_bank_cid: b"AH001-PRB0X-111111111-2026".to_vec().try_into().unwrap(),
        transfer_amount: 10_000,
        fee_amount: 5,
        payer_sig: [9u8; 64],
        payer_nonce: 1,
        expires_at: 100,
    };
    let bytes = item.encode();
    // 变长布局:bank 字段为 CID = Compact(len)||bytes(两个 CID 各 26 字节 → 各 27)。
    // 32(tx)+32(payer)+(1+26)+32(recipient)+(1+26)+16+16+64+8+4 = 258。
    assert_eq!(
        bytes.len(),
        32 + 32 + (1 + 26) + 32 + (1 + 26) + 16 + 16 + 64 + 8 + 4
    );

    let decoded = OffchainBatchItem::<AccountId32, u32>::decode_all(&mut &bytes[..]).unwrap();
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(OffchainBatchItem::<AccountId32, u32>::decode_all(&mut &trailing[..]).is_err());
    assert_eq!(decoded, item);
}
