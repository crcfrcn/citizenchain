use crate::balance::decode_account_balance;

#[test]
fn decode_account_balance_basic() {
    // 16 字节头 + 16 字节 free 余额
    let mut data = vec![0u8; 16]; // nonce+consumers+providers+sufficients
    let balance: u128 = 1_000_000;
    data.extend_from_slice(&balance.to_le_bytes());
    data.extend_from_slice(&[0u8; 32]); // reserved + frozen
    let result = decode_account_balance(&data).unwrap();
    assert_eq!(result, Some(1_000_000));
}

#[test]
fn malformed_account_is_not_an_absent_or_zero_balance() {
    for length in [0, 16, 31] {
        assert!(decode_account_balance(&vec![0; length]).is_err());
    }
    assert_eq!(decode_account_balance(&[0; 80]).unwrap(), Some(0));
}
