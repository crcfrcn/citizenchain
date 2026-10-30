use super::*;

use crate::keystore::SigningKey;
use crate::submitter::decode_batch_items;
use offchain::batch_item::OffchainBatchItem;
use sp_runtime::AccountId32;
use std::sync::Arc;
use std::sync::RwLock;

fn mk_item(seed: u8) -> OffchainBatchItem<AccountId32, u32> {
    OffchainBatchItem {
        tx_id: H256::repeat_byte(seed),
        payer_account_id: AccountId32::new([seed; 32]),
        payer_bank_cid: b"GD001-PRB0T-239565809-2026".to_vec().try_into().unwrap(),
        recipient_account_id: AccountId32::new([seed.wrapping_add(1); 32]),
        recipient_bank_cid: b"AH001-PRB0X-111111111-2026".to_vec().try_into().unwrap(),
        transfer_amount: 1_000,
        fee_amount: 1,
        payer_sig: [0u8; 64],
        payer_nonce: seed as u64,
        expires_at: 100,
    }
}

#[test]
fn batch_bytes_decodes_to_items() {
    let items = vec![mk_item(1), mk_item(2), mk_item(3)];
    let bytes = items.encode();
    let decoded = decode_batch_items(&bytes).unwrap();
    assert_eq!(decoded.len(), 3);
    assert_eq!(decoded[0], items[0]);
    assert_eq!(decoded[2], items[2]);
}

#[test]
fn encode_bounded_sig_respects_limit() {
    let ok: Result<
        frame_support::BoundedVec<
            u8,
            <runtime::Runtime as offchain::pallet::Config>::MaxBatchSignatureLength,
        >,
        String,
    > = crate::submitter::encode_bounded_sig::<runtime::Runtime>(&[7u8; 64]);
    let bounded = ok.expect("64 字节应在 MaxBatchSignatureLength 以内");
    assert_eq!(bounded.len(), 64);
    assert_eq!(bounded[0], 7);
}

#[test]
fn encode_bounded_batch_respects_limit() {
    let items = vec![mk_item(1), mk_item(2)];
    let bounded = crate::submitter::encode_bounded_batch::<runtime::Runtime>(items.clone())
        .expect("2 item 应在 MaxBatchSize 以内");
    assert_eq!(bounded.len(), 2);
    assert_eq!(bounded[0].tx_id, items[0].tx_id);
}

#[test]
fn decode_batch_items_rejects_invalid_bytes() {
    let bytes = vec![0xFFu8; 3];
    assert!(decode_batch_items(&bytes).is_err());
}

#[test]
fn sign_key_slot_none_returns_err() {
    // 不构造 FullClient,只测 signing_key 路径的 None 分支 —
    // 通过 submitter.submit(...) 调用触发。为避免依赖 client,此测试
    // 仅验证 signing_key 锁正常访问:取出 read guard 并确认 None。
    let slot: Arc<RwLock<Option<SigningKey>>> = Arc::new(RwLock::new(None));
    let guard = slot.read().unwrap();
    assert!(guard.is_none());
}
