//! 原所属模块回归，断言与输入保持。
use super::*;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

const ACCOUNT_ID: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";

fn account_id_code_json() -> String {
    serde_json::json!({
        "p": QR_V1,
        "k": QrKind::AccountIdCode.code(),
        "b": { "n": ACCOUNT_ID }
    })
    .to_string()
}

#[test]
fn account_id_code_parser_returns_canonical_account_id() {
    let account_id = parse_account_id_code(&account_id_code_json()).expect("完整账户码应通过");
    assert_eq!(account_id, ACCOUNT_ID);
}

#[test]
fn account_id_code_parser_rejects_aliases_extra_and_temporary_fields() {
    // 字段别名。
    let mut alias: serde_json::Value =
        serde_json::from_str(&account_id_code_json()).expect("测试 JSON");
    let body = alias["b"].as_object_mut().expect("测试 body");
    let account_id = body.remove("n").expect("测试账户");
    body.insert("account".into(), account_id);
    assert!(parse_account_id_code(&alias.to_string()).is_err());

    // 固定码不得带时效字段。
    let mut temporary: serde_json::Value =
        serde_json::from_str(&account_id_code_json()).expect("测试 JSON");
    temporary["i"] = serde_json::json!("forbidden");
    assert!(parse_account_id_code(&temporary.to_string()).is_err());

    // 钱包码只声明账户,不得夹带身份字段。
    let mut extra: serde_json::Value =
        serde_json::from_str(&account_id_code_json()).expect("测试 JSON");
    extra["b"]["display_name"] = serde_json::json!("测试管理员");
    assert!(parse_account_id_code(&extra.to_string()).is_err());

    let mut with_cid: serde_json::Value =
        serde_json::from_str(&account_id_code_json()).expect("测试 JSON");
    with_cid["b"]["cid_number"] = serde_json::json!("CN001-CTZN-000000001-2026");
    assert!(parse_account_id_code(&with_cid.to_string()).is_err());
}

#[test]
fn account_id_code_parser_rejects_noncanonical_account_id() {
    // SS58 不是授权主键,钱包码只收 account_id。
    let ss58_address =
        crate::crypto::pubkey::account_id_to_ss58(ACCOUNT_ID).expect("测试账户可转 SS58");
    let mut as_ss58: serde_json::Value =
        serde_json::from_str(&account_id_code_json()).expect("测试 JSON");
    as_ss58["b"]["n"] = serde_json::json!(ss58_address);
    assert!(parse_account_id_code(&as_ss58.to_string()).is_err());

    // 大写 hex、缺 0x、首尾空格全部拒绝。
    for bad in [
        ACCOUNT_ID.to_uppercase(),
        ACCOUNT_ID.trim_start_matches("0x").to_string(),
        format!(" {ACCOUNT_ID}"),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_str(&account_id_code_json()).expect("测试 JSON");
        value["b"]["n"] = serde_json::json!(bad);
        assert!(parse_account_id_code(&value.to_string()).is_err());
    }
}

#[test]
fn account_id_code_parser_rejects_legacy_chat_node_pairing_payload() {
    // k=5 已从已废止的 chat_node_pairing 回收给钱包码;旧载荷靠 body 字段集拒绝。
    let legacy = serde_json::json!({
        "p": QR_V1,
        "k": 5,
        "b": {
            "node_peer_id": "12D3Koo",
            "node_multiaddr": "/ip4/1.2.3.4/tcp/30333",
            "endpoint_kind": "ip4"
        }
    })
    .to_string();
    assert!(parse_account_id_code(&legacy).is_err());
}

#[test]
fn account_id_code_parser_rejects_other_kinds() {
    for kind in [
        QrKind::SignRequest.code(),
        QrKind::SignResponse.code(),
        QrKind::UserContact.code(),
        QrKind::UserTransfer.code(),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_str(&account_id_code_json()).expect("测试 JSON");
        value["k"] = serde_json::json!(kind);
        assert!(parse_account_id_code(&value.to_string()).is_err());
    }
}

#[test]
fn login_request_always_targets_account_id_code_account() {
    let body = login_request_body("onchina", ACCOUNT_ID).expect("规范账户应生成登录请求");
    assert_eq!(
        b64_to_prefixed_hex(&body.account_id, 32, "b.u").expect("b.u 应可解码"),
        ACCOUNT_ID
    );
}

#[test]
fn sign_response_parses_paired_current_account_handover_signature() {
    let current_account = [0x22u8; 32];
    let current_account_signature = [0x33u8; 64];
    let raw = serde_json::json!({
        "p": QR_V1,
        "k": QrKind::SignResponse.code(),
        "i": "citizen-rebind-response-1",
        "e": 1_800_000_000i64,
        "b": {
            "u": URL_SAFE_NO_PAD.encode([0x11u8; 32]),
            "s": URL_SAFE_NO_PAD.encode([0x44u8; 64]),
            "o": URL_SAFE_NO_PAD.encode(current_account),
            "r": URL_SAFE_NO_PAD.encode(current_account_signature),
        }
    })
    .to_string();
    let parsed = parse_sign_response(raw.as_str()).expect("双签响应应通过");
    let current_account_hex = format!("0x{}", hex::encode(current_account));
    let current_account_signature_hex = format!("0x{}", hex::encode(current_account_signature));
    assert_eq!(
        parsed.body.current_account_id.as_deref(),
        Some(current_account_hex.as_str())
    );
    assert_eq!(
        parsed.body.current_account_signature.as_deref(),
        Some(current_account_signature_hex.as_str())
    );

    let mut missing_current_account_signature: serde_json::Value =
        serde_json::from_str(raw.as_str()).expect("测试 JSON");
    missing_current_account_signature["b"]
        .as_object_mut()
        .expect("测试 body")
        .remove("r");
    assert!(parse_sign_response(missing_current_account_signature.to_string().as_str()).is_err());
}

#[test]
fn domain_sign_request_leaves_account_empty_and_carries_exact_authorization_template() {
    // b.u 留空；b.d 必须原样携带完整授权模板，构造器不得重排或补写业务字段。
    let mut authorization_template = vec![0xaau8; 32];
    authorization_template.extend_from_slice(b"\x68CN220-CTZN2-198805200-2026");
    authorization_template.extend_from_slice(&[0u8; 32]);
    authorization_template.extend_from_slice(&0u64.to_le_bytes());
    authorization_template.extend_from_slice(&1_800_000_000u64.to_le_bytes());
    let json = build_domain_sign_request_bytes(
        "citizen-occupy-req-1",
        1_800_000_000,
        &authorization_template,
        action_occupy(),
    )
    .expect("域签名请求应生成");
    let value: serde_json::Value = serde_json::from_str(&json).expect("生成的应是合法 JSON");
    assert_eq!(value["p"], QR_V1);
    assert_eq!(value["k"], 1);
    assert_eq!(value["b"]["u"], "");
    assert_eq!(
        value["b"]["a"].as_u64().expect("动作码"),
        u64::from(action_occupy())
    );
    let encoded_payload = value["b"]["d"].as_str().expect("b.d 应是字符串");
    assert_eq!(
        URL_SAFE_NO_PAD
            .decode(encoded_payload)
            .expect("b.d 应可解码"),
        authorization_template,
    );
}

#[test]
fn occupy_and_rebind_actions_are_distinct_registered_codes() {
    assert_ne!(action_occupy(), action_rebind());
    assert_eq!(action_occupy(), 10);
    assert_eq!(action_rebind(), 11);
}
