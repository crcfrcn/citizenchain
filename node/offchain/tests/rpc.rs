use super::*;

use crate::rpc::calc_fee;
use crate::rpc::decode_hex;
use crate::rpc::deposit_balance_storage_key;
use crate::rpc::encode_hex;
use crate::rpc::l2_ack_signing_message;
use crate::rpc::l2_fee_rate_bp_storage_key;
use crate::rpc::l3_payment_nonce_storage_key;
use crate::rpc::user_bank_storage_key;
use crate::rpc::PALLET_NAME;
use offchain::batch_item::PaymentIntent;
use sp_runtime::AccountId32;

fn acc(b: u8) -> AccountId32 {
    AccountId32::new([b; 32])
}

#[test]
fn user_bank_storage_key_layout() {
    let user = acc(0x11);
    let encoded = user.encode();
    let key = user_bank_storage_key(&user);
    assert_eq!(key.0.len(), 16 + 16 + 16 + encoded.len());
    assert_eq!(&key.0[..16], &sp_io::hashing::twox_128(PALLET_NAME));
    assert_eq!(&key.0[16..32], &sp_io::hashing::twox_128(b"UserBank"));
    assert_eq!(&key.0[32..48], &sp_io::hashing::blake2_128(&encoded));
    assert_eq!(&key.0[48..], &encoded[..]);
}

#[test]
fn l2_fee_rate_bp_storage_key_layout() {
    let bank: Vec<u8> = b"FR001-PRB08-000000000-2026".to_vec();
    let encoded = bank.encode();
    let key = l2_fee_rate_bp_storage_key(&bank);
    assert_eq!(key.0.len(), 16 + 16 + 16 + encoded.len());
    assert_eq!(&key.0[..16], &sp_io::hashing::twox_128(PALLET_NAME));
    assert_eq!(&key.0[16..32], &sp_io::hashing::twox_128(b"L2FeeRateBp"));
    assert_eq!(&key.0[32..48], &sp_io::hashing::blake2_128(&encoded));
    assert_eq!(&key.0[48..], &encoded[..]);
}

#[test]
fn l3_payment_nonce_storage_key_layout() {
    let user = acc(0x33);
    let encoded = user.encode();
    let key = l3_payment_nonce_storage_key(&user);
    assert_eq!(key.0.len(), 16 + 16 + 16 + encoded.len());
    assert_eq!(&key.0[..16], &sp_io::hashing::twox_128(PALLET_NAME));
    assert_eq!(&key.0[16..32], &sp_io::hashing::twox_128(b"L3PaymentNonce"));
    assert_eq!(&key.0[32..48], &sp_io::hashing::blake2_128(&encoded));
    assert_eq!(&key.0[48..], &encoded[..]);
}

#[test]
fn deposit_balance_storage_key_layout() {
    let bank: Vec<u8> = b"DB001-PRB08-000000000-2026".to_vec();
    let user = acc(0x55);
    let bank_encoded = bank.encode();
    let user_encoded = user.encode();
    let key = deposit_balance_storage_key(&bank, &user);
    assert_eq!(
        key.0.len(),
        16 + 16 + 16 + bank_encoded.len() + 16 + user_encoded.len()
    );
    assert_eq!(&key.0[..16], &sp_io::hashing::twox_128(PALLET_NAME));
    assert_eq!(&key.0[16..32], &sp_io::hashing::twox_128(b"DepositBalance"));
    assert_eq!(&key.0[32..48], &sp_io::hashing::blake2_128(&bank_encoded));
    // CID 一级键变长:偏移按 bank_encoded 长度动态计算。
    let b = 48 + bank_encoded.len();
    assert_eq!(&key.0[48..b], &bank_encoded[..]);
    assert_eq!(
        &key.0[b..b + 16],
        &sp_io::hashing::blake2_128(&user_encoded)
    );
    assert_eq!(&key.0[b + 16..], &user_encoded[..]);
}

#[test]
fn storage_keys_distinct_per_account() {
    assert_ne!(
        user_bank_storage_key(&acc(1)).0,
        user_bank_storage_key(&acc(2)).0
    );
    assert_ne!(
        l2_fee_rate_bp_storage_key(b"CID-A").0,
        l2_fee_rate_bp_storage_key(b"CID-B").0,
    );
    assert_ne!(
        l3_payment_nonce_storage_key(&acc(1)).0,
        l3_payment_nonce_storage_key(&acc(2)).0,
    );
    assert_ne!(
        deposit_balance_storage_key(b"CID-A", &acc(2)).0,
        deposit_balance_storage_key(b"CID-B", &acc(1)).0,
    );
}

#[test]
fn hex_codec_roundtrip() {
    let bytes = vec![0x00, 0xab, 0xff, 0x5c];
    let hex = encode_hex(&bytes);
    assert_eq!(hex, "00abff5c");
    assert_eq!(decode_hex(&format!("0x{hex}")).unwrap(), bytes);
    assert_eq!(decode_hex(&hex).unwrap(), bytes);
}

#[test]
fn hex_decode_rejects_odd_length() {
    assert!(decode_hex("0xabc").is_err());
}

#[test]
fn calc_fee_matches_runtime_rounding() {
    assert_eq!(calc_fee(10_000, 5).unwrap(), 5);
    assert_eq!(
        calc_fee(1, 1).unwrap(),
        primitives::fee_policy::OFFCHAIN_MIN_FEE
    );
    assert_eq!(calc_fee(15_000, 1).unwrap(), 2);
    assert!(calc_fee(u128::MAX, 10_000).is_err());
}

#[test]
fn l2_ack_signing_message_changes_with_inputs() {
    let intent = PaymentIntent {
        tx_id: sp_core::H256::repeat_byte(1),
        payer_account_id: acc(1),
        payer_bank_cid: b"PB001-PRB08-000000000-2026"
            .to_vec()
            .try_into()
            .expect("测试CID在原链边界内"),
        recipient_account_id: acc(3),
        recipient_bank_cid: b"RB001-PRB08-000000000-2026"
            .to_vec()
            .try_into()
            .expect("测试CID在原链边界内"),
        amount: 10_000,
        fee: 5,
        nonce: 1,
        expires_at: 100,
    };
    let sig = [9u8; 64];
    let h1 = l2_ack_signing_message(b"BANK-A", &intent, &sig, 1_717_000_000);
    let h2 = l2_ack_signing_message(b"BANK-A", &intent, &sig, 1_717_000_000);
    assert_eq!(h1, h2);
    assert_ne!(
        h1,
        l2_ack_signing_message(b"BANK-B", &intent, &sig, 1_717_000_000)
    );
    assert_ne!(
        h1,
        l2_ack_signing_message(b"BANK-A", &intent, &[8u8; 64], 1_717_000_000)
    );
    assert_ne!(
        h1,
        l2_ack_signing_message(b"BANK-A", &intent, &sig, 1_717_000_001)
    );
}
