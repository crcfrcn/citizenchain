use crate::signing::account_id_to_ss58;
use crate::signing::classify_invalid_tx;
use crate::signing::dry_run_reject_message;
use crate::signing::encode_compact_u32;
use crate::signing::now_secs;
use crate::signing::sha256_hash;

#[test]
fn encode_compact_u32_single_byte() {
    assert_eq!(encode_compact_u32(0), vec![0x00]);
    assert_eq!(encode_compact_u32(1), vec![0x04]);
    assert_eq!(encode_compact_u32(63), vec![0xfc]);
}

#[test]
fn encode_compact_u32_two_bytes() {
    assert_eq!(encode_compact_u32(64), vec![0x01, 0x01]);
}

#[test]
fn sha256_hash_deterministic() {
    let h1 = sha256_hash(b"hello");
    let h2 = sha256_hash(b"hello");
    assert_eq!(h1, h2);
    assert_ne!(h1, sha256_hash(b"world"));
}

#[test]
fn account_id_to_ss58_roundtrip() {
    let account_id = [0xAAu8; 32];
    let ss58 = account_id_to_ss58(&account_id).unwrap();
    assert!(!ss58.is_empty());
    // 验证可以用 bs58 解码回来
    let decoded = bs58::decode(&ss58).into_vec().unwrap();
    assert_eq!(&decoded[2..34], &account_id);
}

#[test]
fn classify_invalid_tx_known_variants() {
    // 0x01 00 xx = Err(InvalidTransaction::xxx)
    assert!(classify_invalid_tx(&[0x01, 0x00, 0x02]).contains("Future"));
    assert!(classify_invalid_tx(&[0x01, 0x00, 0x03]).contains("Stale"));
    assert!(classify_invalid_tx(&[0x01, 0x00, 0x04]).contains("BadProof"));
    assert!(classify_invalid_tx(&[0x01, 0x00, 0x01]).contains("Payment"));
}

#[test]
fn classify_invalid_tx_unknown_transaction() {
    // 0x01 01 xx = Err(UnknownTransaction::xxx)
    assert_eq!(
        classify_invalid_tx(&[0x01, 0x01, 0x00]),
        "UnknownTransaction"
    );
}

#[test]
fn classify_invalid_tx_unrecognized_code_does_not_panic() {
    // 越界/未知变体编号不得 panic，归入 Unknown
    assert!(classify_invalid_tx(&[0x01, 0x00, 0x63]).contains("Unknown"));
    assert_eq!(classify_invalid_tx(&[0x01]), "UnknownTransaction");
}

#[test]
fn dry_run_reject_future_gives_user_hint() {
    // Future = 上一笔还没出块，前端文案必须是人话，不带技术细节
    assert_eq!(
        dry_run_reject_message(&[0x01, 0x00, 0x02], "0x010002"),
        "上一笔交易尚未出块，请稍候再试"
    );
}

#[test]
fn dry_run_reject_other_variants_keep_technical_reason() {
    // Future 之外的变体保持原有技术报错格式（含 hex 便于排查）
    let stale = dry_run_reject_message(&[0x01, 0x00, 0x03], "0x010003");
    assert!(stale.contains("交易校验失败，已拒绝提交"));
    assert!(stale.contains("Stale"));
    assert!(stale.contains("0x010003"));

    let unknown_tx = dry_run_reject_message(&[0x01, 0x01, 0x00], "0x010100");
    assert!(unknown_tx.contains("UnknownTransaction"));
}

#[test]
fn now_secs_returns_positive() {
    // 正常系统时钟下必须返回 epoch 之后的正数秒
    assert!(now_secs().unwrap() > 1_700_000_000);
}
