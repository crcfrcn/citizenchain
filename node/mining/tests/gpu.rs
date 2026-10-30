use super::*;

use crate::gpu::difficulty_to_target_be;

#[test]
fn difficulty_to_target_be_zero_returns_zeros() {
    assert_eq!(difficulty_to_target_be(U256::zero()), [0u64; 4]);
}

#[test]
fn difficulty_to_target_be_one_returns_max() {
    let result = difficulty_to_target_be(U256::one());
    // U256::MAX / 1 = U256::MAX → 全 0xFF
    assert_eq!(result, [u64::MAX; 4]);
}

#[test]
fn difficulty_to_target_be_two() {
    let result = difficulty_to_target_be(U256::from(2));
    let target = U256::MAX / U256::from(2);
    let bytes: [u8; 32] = target.to_big_endian();
    let expected = bytes_to_words(&bytes);
    assert_eq!(result, expected);
}

#[test]
fn difficulty_to_target_be_roundtrip() {
    let difficulty = U256::from(1000);
    let words = difficulty_to_target_be(difficulty);
    let target = U256::MAX / difficulty;
    let mut reconstructed = [0u8; 32];
    for (i, word) in words.iter().enumerate() {
        reconstructed[i * 8..(i + 1) * 8].copy_from_slice(&word.to_be_bytes());
    }
    assert_eq!(U256::from_big_endian(&reconstructed), target);
}

fn bytes_to_words(bytes: &[u8; 32]) -> [u64; 4] {
    let mut words = [0u64; 4];
    for i in 0..4 {
        let o = i * 8;
        words[i] = u64::from_be_bytes([
            bytes[o],
            bytes[o + 1],
            bytes[o + 2],
            bytes[o + 3],
            bytes[o + 4],
            bytes[o + 5],
            bytes[o + 6],
            bytes[o + 7],
        ]);
    }
    words
}
