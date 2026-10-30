use super::*;

#[test]
fn key_bytes_accept_exact_width_and_reject_bad_hex_or_length() {
    assert_eq!(
        decode_hex_32(&format!("0x{}", "11".repeat(32)), "public").unwrap(),
        [17u8; 32]
    );
    for input in ["11".repeat(31), "11".repeat(33), "gg".repeat(32)] {
        assert!(decode_hex_32(&input, "public").is_err());
    }
}

#[test]
fn cid_scale_obeys_empty_and_maximum_boundaries() {
    use codec::DecodeAll;
    let maximum = primitives::core_const::CID_NUMBER_MAX_BYTES as usize;
    assert!(cid_scale("").is_err());
    assert!(cid_scale(&"a".repeat(maximum + 1)).is_err());
    let encoded = cid_scale(&"a".repeat(maximum)).unwrap();
    assert_eq!(
        Vec::<u8>::decode_all(&mut encoded.as_slice()).unwrap(),
        vec![b'a'; maximum]
    );
}
