//! 验证清算登记、注销和更新调用布局及输入边界。

use crate::signing::build_register_call_data;
use crate::signing::build_unregister_call_data;
use crate::signing::build_update_endpoint_call_data;
use crate::signing::parse_signer_public_key;
use codec::{DecodeAll, Encode};

#[test]
fn register_call_data_starts_with_pallet_and_call() {
    let cd = build_register_call_data(
        "AH001-SCB0V-123456789-2026",
        "ROLE_A",
        "12D3KooWAbcDef0123456789012345678901234567890123456",
        "rpc.example.com",
        9944,
    )
    .unwrap();
    assert_eq!(cd[0], 19);
    assert_eq!(cd[1], 50);
    // tail 应为 little-endian u16 端口
    let n = cd.len();
    assert_eq!(u16::from_le_bytes([cd[n - 2], cd[n - 1]]), 9944);
}

#[test]
fn register_call_rejects_empty_cid() {
    let err = build_register_call_data(
        "",
        "ROLE_A",
        "12D3KooWaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "x.com",
        9944,
    )
    .unwrap_err();
    assert!(err.contains("actor_cid_number"));
}

#[test]
fn register_call_rejects_low_port() {
    let err = build_register_call_data(
        "S",
        "ROLE_A",
        "12D3KooWaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "x.com",
        80,
    )
    .unwrap_err();
    assert!(err.contains("rpc_port"));
}

#[test]
fn all_clearing_calls_decode_as_actual_runtime_calls_with_the_selected_role() {
    let cid = "AH001-SCB0V-123456789-2026";
    for role in ["ROLE_A", "ROLE_B"] {
        let register = build_register_call_data(
            cid,
            role,
            "12D3KooWAbcDef0123456789012345678901234567890123456",
            "rpc.example.com",
            9944,
        )
        .unwrap();
        let expected =
            runtime::RuntimeCall::Offchain(offchain::pallet::Call::register_clearing_bank {
                actor_cid_number: cid.as_bytes().to_vec().try_into().unwrap(),
                actor_role_code: role.as_bytes().to_vec().try_into().unwrap(),
                peer_id: b"12D3KooWAbcDef0123456789012345678901234567890123456"
                    .to_vec()
                    .try_into()
                    .unwrap(),
                rpc_domain: b"rpc.example.com".to_vec().try_into().unwrap(),
                rpc_port: 9944,
            });
        assert_eq!(register, expected.encode());
        assert!(
            matches!(runtime::RuntimeCall::decode_all(&mut register.as_slice()).unwrap(),runtime::RuntimeCall::Offchain(offchain::pallet::Call::register_clearing_bank{actor_role_code,..}) if actor_role_code.as_slice()==role.as_bytes())
        );
        let update = build_update_endpoint_call_data(cid, role, "new.example.com", 9443).unwrap();
        assert_eq!(&update[..2], &[19, 51]);
        assert!(
            matches!(runtime::RuntimeCall::decode_all(&mut update.as_slice()).unwrap(),runtime::RuntimeCall::Offchain(offchain::pallet::Call::update_clearing_bank_endpoint{actor_cid_number,actor_role_code,new_domain,new_port}) if actor_cid_number.as_slice()==cid.as_bytes()&&actor_role_code.as_slice()==role.as_bytes()&&new_domain.as_slice()==b"new.example.com"&&new_port==9443)
        );
        let unregister = build_unregister_call_data(cid, role).unwrap();
        assert_eq!(&unregister[..2], &[19, 52]);
        assert!(
            matches!(runtime::RuntimeCall::decode_all(&mut unregister.as_slice()).unwrap(),runtime::RuntimeCall::Offchain(offchain::pallet::Call::unregister_clearing_bank{actor_cid_number,actor_role_code}) if actor_cid_number.as_slice()==cid.as_bytes()&&actor_role_code.as_slice()==role.as_bytes())
        );
    }
}

#[test]
fn clearing_calls_reject_missing_noncanonical_and_overlong_role_instead_of_defaulting() {
    let cid = "AH001-SCB0V-123456789-2026";
    for role in [
        "".to_string(),
        " ROLE_A".into(),
        "ROLE_A ".into(),
        "A".repeat(1024),
    ] {
        assert!(
            build_register_call_data(cid, &role, "peer", "rpc.example.com", 9944)
                .unwrap_err()
                .contains("actor_role_code")
        );
        assert!(
            build_update_endpoint_call_data(cid, &role, "rpc.example.com", 9944)
                .unwrap_err()
                .contains("actor_role_code")
        );
        assert!(build_unregister_call_data(cid, &role)
            .unwrap_err()
            .contains("actor_role_code"));
    }
}

#[test]
fn signer_public_key_uses_prefixed_lowercase_hex() {
    let public_key = "0xaabbccddeeff00112233445566778899aabbccddeeff00112233445566778899";
    let (clean, bytes) = parse_signer_public_key(public_key).unwrap();
    assert_eq!(clean.len(), 66);
    assert_eq!(clean, public_key);
    assert_eq!(bytes.len(), 32);
    assert!(parse_signer_public_key(
        "0xAABBCCDDEEFF00112233445566778899AABBCCDDEEFF00112233445566778899"
    )
    .is_err());
}

#[test]
fn signer_public_key_rejects_bad_length() {
    assert!(parse_signer_public_key("0x").is_err());
    assert!(parse_signer_public_key("xyz").is_err());
}
