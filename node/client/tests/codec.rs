use super::*;
use codec::Encode;

#[test]
fn truncate_chars_keeps_short_input_unchanged() {
    assert_eq!(truncate_chars("hello", 10), "hello");
}

#[test]
fn truncate_chars_appends_ellipsis_when_over_limit() {
    assert_eq!(truncate_chars("0123456789ABCDEF", 8), "01234567…");
}

#[test]
fn truncate_chars_safe_for_multibyte_utf8() {
    // 8 个汉字 ≤ 10 字符上限 → 不截断
    assert_eq!(truncate_chars("中华人民共和国家", 10), "中华人民共和国家");
    // 8 个汉字 > 4 字符上限 → 保留前 4 字符 + …
    assert_eq!(truncate_chars("中华人民共和国家", 4), "中华人民…");
}

#[test]
fn compact_boundaries_match_scale_codec_and_reject_each_short_prefix() {
    for value in [0, 1, 63, 64, 16_383, 16_384, 1_048_576, (1u32 << 30) - 1] {
        let encoded = codec::Compact(value).encode();
        assert_eq!(
            read_compact_u32(&encoded, 0).unwrap(),
            (value, encoded.len())
        );
        for n in 0..encoded.len() {
            assert!(read_compact_u32(&encoded[..n], 0).is_err());
        }
        let mut prefixed = vec![0, 0];
        prefixed.extend(&encoded);
        assert_eq!(
            read_compact_u32(&prefixed, 2).unwrap(),
            (value, encoded.len())
        );
    }
    assert!(read_compact_u32(&codec::Compact(u32::MAX).encode(), 0).is_err());
    assert!(read_compact_u32(&[], usize::MAX).is_err());
}

#[test]
fn cid_and_role_decoders_reject_invalid_utf8_or_truncated_vectors() {
    for bytes in [vec![], vec![8, b'a'], vec![4, 255]] {
        assert!(decode_cid_number(&bytes, &mut 0).is_none());
        assert!(decode_role_code(&bytes, &mut 0).is_none());
    }
    let encoded = b"44-NRC".to_vec().encode();
    let mut offset = 0;
    assert_eq!(
        decode_cid_number(&encoded, &mut offset).as_deref(),
        Some("44-NRC")
    );
    assert_eq!(offset, encoded.len());
}

#[test]
fn runtime_metadata_uses_the_real_producer_and_rejects_bad_prefix_or_suffix() {
    let metadata = runtime::Runtime::metadata().encode();
    assert!(decode_runtime_metadata(&metadata).is_ok());
    let mut wrong = metadata.clone();
    wrong[0] ^= 1;
    assert!(decode_runtime_metadata(&wrong).is_err());
    let mut extra = metadata;
    extra.push(0);
    assert!(decode_runtime_metadata(&extra).is_err());
    assert!(decode_runtime_metadata(&[]).is_err());
}

#[test]
fn unicode_truncation_keeps_zero_exact_and_multi_byte_boundaries() {
    for (input, maximum, expected) in [
        ("", 0, ""),
        ("公民", 0, "…"),
        ("公民", 1, "公…"),
        ("a👍b", 2, "a👍…"),
        ("a👍b", 3, "a👍b"),
    ] {
        assert_eq!(truncate_chars(input, maximum), expected);
    }
}
