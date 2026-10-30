use super::*;

use crate::bootstrap::last_clearing_batch_seq_key;
use crate::bootstrap::PALLET_NAME;

#[test]
fn last_clearing_batch_seq_key_layout_stable() {
    let bank: Vec<u8> = b"ZS001-PRB08-233384677-2026".to_vec();
    let encoded = bank.encode();
    let key = last_clearing_batch_seq_key(&bank);
    assert_eq!(key.0.len(), 16 + 16 + 16 + encoded.len());
    assert_eq!(&key.0[..16], &sp_io::hashing::twox_128(PALLET_NAME));
    assert_eq!(
        &key.0[16..32],
        &sp_io::hashing::twox_128(b"LastClearingBatchSeq")
    );
    assert_eq!(&key.0[32..48], &sp_io::hashing::blake2_128(&encoded));
    assert_eq!(&key.0[48..], &encoded[..]);
}
