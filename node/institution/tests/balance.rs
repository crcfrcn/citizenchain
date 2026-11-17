use crate::balance::spendable_balance;
use codec::Encode;
use nodeclient::codec::{account_total, decode_account, AccountInfo};

#[test]
fn total_balance_includes_reserved_and_spendable_respects_frozen_and_ed() {
    for (free, reserved, frozen, total, spendable) in [
        (300_111, 0, 0, 300_111, 300_000),
        (69_990, 0, 0, 69_990, 69_879),
        (1_000, 200, 800, 1_200, 400),
        (1_000, 700, 800, 1_700, 889),
        (1_000, 200, 2_000, 1_200, 0),
        (111, 0, 0, 111, 0),
        (0, 0, 0, 0, 0),
    ] {
        let mut account = AccountInfo::default();
        account.data.free = free;
        account.data.reserved = reserved;
        account.data.frozen = frozen;
        let decoded = decode_account(&account.encode()).unwrap();
        assert_eq!(account_total(&decoded).unwrap(), total);
        assert_eq!(spendable_balance(&decoded), spendable);
    }
}

#[test]
fn malformed_account_is_not_an_absent_or_zero_balance() {
    for length in [0, 16, 31, 32, 64, 79, 81] {
        assert!(decode_account(&vec![0; length]).is_err());
    }
    assert_eq!(account_total(&decode_account(&[0; 80]).unwrap()).unwrap(), 0);
}
