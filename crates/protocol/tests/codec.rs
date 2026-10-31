//! 原协议编解码回归，通过唯一公开接口执行。
#![allow(clippy::expect_used, clippy::unwrap_used)]
use qr_protocol::codec::*;
use serde_json::Value;

#[test]
fn decodes_public_key_to_lowercase_prefixed_hex() {
    let raw = [0xABu8; PUBLIC_KEY_BYTES];
    let encoded = bytes_to_b64(&raw);
    let hex_text =
        b64_to_prefixed_hex(&encoded, PUBLIC_KEY_BYTES, "b.u").expect("32 字节公钥必须解码成功");
    assert_eq!(hex_text, format!("0x{}", "ab".repeat(PUBLIC_KEY_BYTES)));
}

#[test]
fn round_trips_signature_length() {
    let raw = [0x01u8; SIGNATURE_BYTES];
    let encoded = bytes_to_b64(&raw);
    assert!(b64_to_prefixed_hex(&encoded, SIGNATURE_BYTES, "b.s").is_ok());
}

#[test]
fn rejects_wrong_length_without_truncating() {
    let encoded = bytes_to_b64(&[0u8; 31]);
    assert_eq!(
        b64_to_prefixed_hex(&encoded, PUBLIC_KEY_BYTES, "b.u"),
        Err(CodecError::BadLength {
            field: "b.u".to_string(),
            expected: 32,
            actual: 31,
        })
    );
}

#[test]
fn rejects_non_base64url_input() {
    // 标准 base64 的 `+` `/` 与 padding `=` 都不属于 base64url(no padding)。
    assert_eq!(
        b64_to_prefixed_hex("++//", 32, "b.u"),
        Err(CodecError::NotBase64Url {
            field: "b.u".to_string()
        })
    );
}

#[test]
fn public_key_b64_rejects_wrong_length() {
    assert!(public_key_b64(&[0u8; 33], "b.u").is_err());
    assert!(public_key_b64(&[0u8; PUBLIC_KEY_BYTES], "b.u").is_ok());
}

#[test]
fn schema_rejects_unknown_keys_and_noncanonical_account_id() {
    let valid = serde_json::json!({
        "p": QR_V1,
        "k": 5,
        "b": { "n": format!("0x{}", "11".repeat(32)) }
    });
    assert_eq!(
        validate_qr_value(&valid)
            .expect("规范账户码必须通过")
            .kind_key,
        "account_id_code"
    );

    let mut extra = valid.clone();
    extra["b"]["account"] = Value::String("forbidden".into());
    assert!(validate_qr_value(&extra).is_err());

    let mut uppercase = valid;
    uppercase["b"]["n"] = Value::String(format!("0x{}", "AA".repeat(32)));
    assert!(validate_qr_value(&uppercase).is_err());
}

#[test]
fn schema_enforces_signer_public_key_action_condition() {
    let payload = bytes_to_b64(&[0x22]);
    let self_account = serde_json::json!({
        "p": QR_V1,
        "k": 1,
        "i": "request-1",
        "e": 1_800_000_000,
        "b": { "a": 10, "g": 1, "u": "", "d": payload }
    });
    assert!(validate_qr_value(&self_account).is_ok());

    let mut invalid = self_account;
    invalid["b"]["u"] = Value::String(bytes_to_b64(&[0x11; PUBLIC_KEY_BYTES]));
    assert!(validate_qr_value(&invalid).is_err());
}

#[test]
fn every_owned_golden_is_accepted_and_envelope_unknown_fields_are_rejected() {
    for text in [
        include_str!("sign_request.json"),
        include_str!("sign_response.json"),
        include_str!("user_contact.json"),
        include_str!("user_transfer.json"),
        include_str!("account_id_code.json"),
    ] {
        let value: Value = serde_json::from_str(text).unwrap();
        assert!(validate_qr_value(&value).is_ok());
        let mut extra = value.clone();
        extra["unknown"] = true.into();
        assert!(validate_qr_value(&extra).is_err());
        let mut body = value.clone();
        body["b"]["unknown"] = true.into();
        assert!(validate_qr_value(&body).is_err());
        let mut protocol = value.clone();
        protocol["p"] = "invalid-protocol".into();
        assert!(validate_qr_value(&protocol).is_err());
    }
}

#[test]
fn temporary_expiry_request_id_and_fixed_shapes_have_strict_boundaries() {
    let request: Value = serde_json::from_str(include_str!("sign_request.json")).unwrap();
    for expiry in [
        serde_json::json!(0),
        serde_json::json!(-1),
        serde_json::json!(1.5),
        serde_json::json!("1800000000"),
    ] {
        let mut bad = request.clone();
        bad["e"] = expiry;
        assert!(validate_qr_value(&bad).is_err());
    }
    for id in [Value::Null, serde_json::json!(""), serde_json::json!(1)] {
        let mut bad = request.clone();
        bad["i"] = id;
        assert!(validate_qr_value(&bad).is_err());
    }
    for kind in [
        serde_json::json!(0),
        serde_json::json!(256),
        serde_json::json!(1.5),
        serde_json::json!("1"),
    ] {
        let mut bad = request.clone();
        bad["k"] = kind;
        assert!(validate_qr_value(&bad).is_err());
    }
    let mut fixed: Value = serde_json::from_str(include_str!("account_id_code.json")).unwrap();
    fixed["i"] = "extra".into();
    assert!(validate_qr_value(&fixed).is_err());
}

#[test]
fn optional_current_account_signature_is_an_atomic_pair_with_exact_lengths() {
    let mut value: Value = serde_json::from_str(include_str!("sign_response.json")).unwrap();
    value["b"].as_object_mut().unwrap().remove("o");
    value["b"].as_object_mut().unwrap().remove("r");
    assert!(validate_qr_value(&value).is_ok());
    value["b"]["o"] = bytes_to_b64(&[1; 32]).into();
    assert!(validate_qr_value(&value).is_err());
    value["b"]["r"] = bytes_to_b64(&[2; 64]).into();
    assert!(validate_qr_value(&value).is_ok());
    value["b"]["r"] = bytes_to_b64(&[2; 63]).into();
    assert!(validate_qr_value(&value).is_err());
    for input in ["=", "+/", "ab==", "é"] {
        assert!(b64_to_prefixed_hex(input, 32, "b.u").is_err());
    }
}
