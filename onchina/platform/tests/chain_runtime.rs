//! 验证官方RPC字段、人员任职及平台链状态的解码合同。

use super::{
    decode_scale_u128, is_production_mode, parse_hex_hash32, trusted_production_chain_by_hash,
    ActiveMembership, FRG_CODE,
};

#[test]
fn upstream_rpc_envelopes_keep_official_jsonrpc_fields() {
    // JSON-RPC 的 jsonrpc/id 属于上游封装，局部投影不得拒绝未使用的官方字段。
    let hash: super::ChainGetBlockHashResponse = serde_json::from_value(serde_json::json!({
        "jsonrpc":"2.0", "id":1, "result":"0x11"
    }))
    .unwrap();
    assert_eq!(hash.result.as_deref(), Some("0x11"));
    let value: super::ChainRpcValueResponse = serde_json::from_value(serde_json::json!({
        "jsonrpc":"2.0", "id":2, "result":null
    }))
    .unwrap();
    assert_eq!(value.id, 2);
    let header: super::ChainHeaderResponse = serde_json::from_value(serde_json::json!({
        "jsonrpc":"2.0", "id":3, "result":null
    }))
    .unwrap();
    assert!(header.result.is_none());
}

#[test]
fn candidate_identifies_institution_and_selected_role() {
    let member = ActiveMembership {
        institution_code: FRG_CODE,
        cid_number: "institution".into(),
        person_cid: crate::testing::person_cid(),
        role_code: "ROLE_A".into(),
        role_name: "岗位A".into(),
        town_code: vec![],
        frg_province_code: Some(*b"GZ"),
    };
    let other = ActiveMembership {
        role_code: "ROLE_B".into(),
        ..member.clone()
    };
    assert_ne!(member.candidate_id(), other.candidate_id());
    assert_eq!(member.candidate_id(), "institution:ROLE_A");
    let rebound = ActiveMembership {
        frg_province_code: Some(*b"GD"),
        ..member.clone()
    };
    assert_eq!(member.candidate_id(), rebound.candidate_id());
}

#[test]
fn platform_price_storage_values_decode_strictly() {
    // 平台 CID 已是创世常量，不再从链上解码；仅严格校验三档价格 u128 解码。
    let price = 123_456_u128;
    assert_eq!(
        decode_scale_u128(&format!("0x{}", hex::encode(price.to_le_bytes()))).unwrap(),
        price
    );
    assert!(decode_scale_u128("0x01").is_err());
}

#[test]
fn entity_namespaces_follow_runtime_and_personal_is_rejected() {
    use super::EntityPallet;
    assert_eq!(EntityPallet::Public.pallet_name(), "PublicEntity");
    assert_eq!(EntityPallet::Private.pallet_name(), "PrivateEntity");
    for code in [b"NJD\0", b"NRC\0", b"CREG"] {
        assert_eq!(
            super::console_entity_pallets(code).unwrap(),
            vec![EntityPallet::Public]
        );
    }
    assert_eq!(
        super::console_entity_pallets(b"SFGY").unwrap(),
        vec![EntityPallet::Private]
    );
    assert_eq!(
        super::console_entity_pallets(b"UNIN").unwrap(),
        vec![EntityPallet::Public, EntityPallet::Private]
    );
    assert_eq!(
        super::console_entity_pallets(b"PMUL").unwrap_err(),
        super::PERSONAL_MULTISIG_LOGIN_UNSUPPORTED
    );
}

#[test]
fn representative_scale_has_person_identity_without_wallet() {
    use codec::{DecodeAll, Encode};
    let value = entitybase::LegalRepresentative {
        family_name: b"family".to_vec(),
        given_name: b"given".to_vec(),
        person_cid: crate::testing::person_cid().into_bytes(),
    };
    let bytes = value.encode();
    let decoded = super::RawLegalRepresentative::decode_all(&mut bytes.as_slice()).unwrap();
    assert_eq!(decoded.person_cid, value.person_cid);
    let mut trailing = bytes;
    trailing.push(0);
    assert!(super::RawLegalRepresentative::decode_all(&mut trailing.as_slice()).is_err());
}

#[test]
fn parse_hex_hash32_accepts_prefixed_hash() {
    let parsed = parse_hex_hash32(&format!("0x{}", "11".repeat(32))).unwrap();
    assert_eq!(parsed, [0x11; 32]);
}

#[test]
fn trusted_production_chain_lookup_returns_none_for_unknown_hash() {
    let result = trusted_production_chain_by_hash(&[0x22; 32]).unwrap();
    assert!(result.is_none());
}

#[test]
fn production_mode_detects_prod_env() {
    let previous = std::env::var("ONCHINA_ENV").ok();
    std::env::set_var("ONCHINA_ENV", "prod");
    assert!(is_production_mode());
    if let Some(value) = previous {
        std::env::set_var("ONCHINA_ENV", value);
    } else {
        std::env::remove_var("ONCHINA_ENV");
    }
}
