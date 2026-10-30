use super::*;

#[test]
fn header_height_requires_hex_string_and_checks_overflow() {
    assert_eq!(
        header_block_height(&serde_json::json!({"number":"0xff"})),
        Some(255)
    );
    for value in [
        serde_json::json!({}),
        serde_json::json!({"number":255}),
        serde_json::json!({"number":"ff"}),
        serde_json::json!({"number":"0x10000000000000000"}),
    ] {
        assert_eq!(header_block_height(&value), None);
    }
}

#[test]
fn balances_keep_maximum_u128_and_reject_short_malformed_hex() {
    assert_eq!(
        scale_u128_from_storage_hex(&format!("0x{}", hex::encode(u128::MAX.to_le_bytes()))),
        Some(u128::MAX)
    );
    for value in ["0x0", "0xgg", "", "0x0000"] {
        assert_eq!(scale_u128_from_storage_hex(value), None);
    }
    let mut account = vec![0; 16];
    account.extend(111u128.to_le_bytes());
    assert_eq!(
        extract_free_balance(&format!("0x{}", hex::encode(account))),
        Some(111)
    );
    assert_eq!(format_fen_to_yuan(111), "1.11");
    assert_eq!(format_fen_to_yuan(123456789), "1,234,567.89");
}

#[test]
fn hex_parser_rejects_invalid_unicode_and_non_hex_without_panicking() {
    for value in [
        "中a",
        "0x中a",
        "é",
        "😀",
        "a中",
        "e\u{301}a",
        "0x0",
        "gg",
        "+a",
        "0Xff",
    ] {
        assert_eq!(hex_to_bytes(value), None, "invalid input: {value}");
        assert_eq!(scale_u128_from_storage_hex(value), None);
        assert_eq!(extract_free_balance(value), None);
    }
}

#[test]
fn hex_parser_preserves_empty_prefix_case_and_exact_bytes() {
    for value in ["", "0x"] {
        assert_eq!(hex_to_bytes(value), Some(Vec::new()));
    }
    for value in ["00aAfF", "0x00AAff"] {
        assert_eq!(hex_to_bytes(value), Some(vec![0, 170, 255]));
    }
    let value = format!("0x{}", "ff".repeat(32));
    assert_eq!(extract_free_balance(&value), Some(u128::MAX));
}
