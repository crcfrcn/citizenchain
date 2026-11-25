//! 验证PoW哈希确定性、输入变化及独立BLAKE2计算一致性。

use super::*;

#[test]
fn parent_difficulty_remains_raw_nonzero_and_canonical() {
    assert_eq!(raw_difficulty(&1u64.to_le_bytes()).unwrap(), U256::one());
    assert_eq!(
        raw_difficulty(&u64::MAX.to_le_bytes()).unwrap(),
        U256::from(u64::MAX)
    );
    assert!(raw_difficulty(&0u64.to_le_bytes()).is_err());
    for size in 0..8 {
        assert!(raw_difficulty(&1u64.to_le_bytes()[..size]).is_err());
    }
    let mut trailing = 1u64.to_le_bytes().to_vec();
    trailing.push(0);
    assert!(raw_difficulty(&trailing).is_err());
}

#[test]
fn pow_hash_deterministic() {
    let pre_hash = [0u8; 32];
    let h1 = pow_hash(&pre_hash, 42);
    let h2 = pow_hash(&pre_hash, 42);
    assert_eq!(h1, h2);
}

#[test]
fn pow_hash_differs_with_different_nonce() {
    let pre_hash = [1u8; 32];
    assert_ne!(pow_hash(&pre_hash, 0), pow_hash(&pre_hash, 1));
}

#[test]
fn pow_hash_differs_with_different_pre_hash() {
    assert_ne!(pow_hash(&[0u8; 32], 0), pow_hash(&[1u8; 32], 0));
}

#[test]
fn pow_hash_matches_manual_blake2() {
    let pre_hash = [7u8; 32];
    let nonce = 123u64;
    let mut payload = Vec::new();
    payload.extend_from_slice(&pre_hash);
    payload.extend_from_slice(&nonce.to_le_bytes());
    assert_eq!(pow_hash(&pre_hash, nonce), blake2_256(&payload));
}

#[test]
fn hash_meets_difficulty_zero_always_false() {
    assert!(!hash_meets_difficulty(&[0u8; 32], U256::zero()));
}

#[test]
fn hash_meets_difficulty_one_always_true() {
    // difficulty=1 → target=U256::MAX, any hash passes
    assert!(hash_meets_difficulty(&[0xFF; 32], U256::one()));
}

#[test]
fn hash_meets_difficulty_max_only_zero_hash() {
    // difficulty=U256::MAX → target=1, only hash ≤ 1 passes
    assert!(hash_meets_difficulty(&[0u8; 32], U256::MAX));
    let mut h = [0u8; 32];
    h[31] = 2;
    assert!(!hash_meets_difficulty(&h, U256::MAX));
}

#[test]
fn hash_meets_difficulty_boundary() {
    let difficulty = U256::from(2);
    let target = U256::MAX / difficulty;
    // At target: pass
    let at_target: [u8; 32] = target.to_big_endian();
    assert!(hash_meets_difficulty(&at_target, difficulty));
    // Above target: fail
    let above = target + U256::one();
    let above_bytes: [u8; 32] = above.to_big_endian();
    assert!(!hash_meets_difficulty(&above_bytes, difficulty));
}
