use super::*;

use crate::ledger::OffchainLedger;
use crate::reserve::bank_total_deposits_key;
use sp_runtime::AccountId32;
use std::fs;
use std::sync::Arc;

fn mk_ledger() -> Arc<OffchainLedger> {
    let tmp = std::env::temp_dir().join("offchain_reserve_monitor_test");
    let _ = fs::remove_dir_all(&tmp);
    Arc::new(crate::ledger::OffchainLedger::new(&tmp))
}

fn acc(b: u8) -> AccountId32 {
    AccountId32::new([b; 32])
}

#[test]
fn confirmed_sum_empty_is_zero() {
    let ledger = mk_ledger();
    assert_eq!(ledger.confirmed_sum_snapshot(), 0);
}

#[test]
fn confirmed_sum_adds_across_users() {
    let ledger = mk_ledger();
    ledger.on_deposited(&acc(1), 300);
    ledger.on_deposited(&acc(2), 700);
    ledger.on_deposited(&acc(1), 100); // 累加同 user
    assert_eq!(ledger.confirmed_sum_snapshot(), 1_100);
}

#[test]
fn confirmed_sum_after_withdraw() {
    let ledger = mk_ledger();
    ledger.on_deposited(&acc(1), 1_000);
    ledger.on_withdrawn(&acc(1), 250);
    assert_eq!(ledger.confirmed_sum_snapshot(), 750);
}

#[test]
fn bank_total_deposits_key_layout_stable() {
    // 关键性质:storage key 必须 = twox_128(pallet) ++ twox_128(storage) ++
    // blake2_128(encoded) ++ encoded。长度与前缀不能随实现漂移,否则读链上
    // 会 silently 命中不同 key,对账永远读到 0 而看不出偏差。
    let bank = acc(0xAA);
    let key = bank_total_deposits_key(&bank);
    let encoded = bank.encode();
    let expected_len = 16 + 16 + 16 + encoded.len();
    assert_eq!(key.0.len(), expected_len);
    assert_eq!(
        &key.0[..16],
        &sp_io::hashing::twox_128(b"OffchainTransaction")
    );
    assert_eq!(
        &key.0[16..32],
        &sp_io::hashing::twox_128(b"BankTotalDeposits")
    );
    assert_eq!(&key.0[32..48], &sp_io::hashing::blake2_128(&encoded));
    assert_eq!(&key.0[48..], &encoded[..]);
}

#[test]
fn bank_total_deposits_key_differs_per_bank() {
    assert_ne!(
        bank_total_deposits_key(&acc(1)).0,
        bank_total_deposits_key(&acc(2)).0,
    );
}
