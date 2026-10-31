//! 原所属模块回归，断言与输入保持。
use super::{
    decode_scale_u128, is_production_mode, one_frg_province, parse_hex_hash32,
    trusted_production_chain_by_hash, ActiveAdminMembership, FRG_CODE,
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
fn frg_candidate_identity_is_cid_only_and_role_scope_is_unambiguous() {
    let membership = ActiveAdminMembership {
        institution_code: FRG_CODE,
        cid_number: "FRG-CID".into(),
        frg_province_code: Some(*b"GZ"),
    };
    let moved = ActiveAdminMembership {
        frg_province_code: Some(*b"GD"),
        ..membership.clone()
    };
    assert_eq!(membership.candidate_id(), moved.candidate_id());
    assert!(membership.candidate_id().contains("FRG-CID"));
    assert_eq!(one_frg_province(&[*b"GZ"]).unwrap(), *b"GZ");
    assert!(one_frg_province(&[]).is_err());
    assert!(one_frg_province(&[*b"GZ", *b"GD"]).is_err());
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

/// 锁定公权管理员四字段 SCALE 布局；机构 CID 仍只存在于 storage key。
#[test]
fn onchain_institution_admin_account_decodes_unified_records_only() {
    use codec::Encode;

    let bytes = adminbase::InstitutionAdmins {
        institution_code: *b"CREG",
        admins: vec![adminbase::Admin {
            account_id: [0x42u8; 32],
            cid_number: "CN220-CTZN2-198805200-2026"
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("citizen cid fits"),
            family_name: Default::default(),
            given_name: Default::default(),
        }],
    }
    .encode();
    let decoded = super::decode_onchain_admin_account(&bytes, super::AdminPallet::PublicAdmins)
        .expect("public institution admin account must decode unified layout");
    assert_eq!(decoded.institution_code, *b"CREG");
    assert_eq!(decoded.admins.len(), 1);
    assert_eq!(decoded.admins[0].account_id, [0x42; 32]);
    // 管理员 CID 由链上名册提供，钱包码无需重复声明；实际签名账户由该 CID
    // 在同一个 finalized 区块上的当前绑定决定。
    assert_eq!(
        decoded.admins[0].cid_number.as_slice(),
        "CN220-CTZN2-198805200-2026".as_bytes()
    );
    let resolved = [super::ResolvedOnChainAdminRecord {
        role_account_id: decoded.admins[0].account_id,
        account_id: [0x43; 32],
        cid_number: decoded.admins[0].cid_number.clone(),
        family_name: decoded.admins[0].family_name.clone(),
        given_name: decoded.admins[0].given_name.clone(),
    }];
    // 换绑后岗位关联仍保留名册账户，但只有 CID 当前绑定的新账户能签名。
    assert!(resolved.iter().all(|admin| admin.account_id != [0x42; 32]));
    assert!(resolved.iter().any(|admin| admin.account_id == [0x43; 32]));
    assert_eq!(resolved[0].role_account_id, [0x42; 32]);

    let old_layout = (*b"CREG", vec![[0x42u8; 32]]).encode();
    assert!(
        super::decode_onchain_admin_account(&old_layout, super::AdminPallet::PublicAdmins,)
            .is_err()
    );
}

#[test]
fn private_institution_admin_account_decodes_unified_layout() {
    use codec::Encode;

    let bytes = adminbase::InstitutionAdmins {
        institution_code: *b"SFGY",
        admins: vec![adminbase::Admin {
            account_id: [0x24u8; 32],
            cid_number: "CN220-CTZN2-198805200-2026"
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("citizen cid fits"),
            family_name: "程"
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("family name fits"),
            given_name: "伟"
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("given name fits"),
        }],
    }
    .encode();
    let decoded = super::decode_onchain_admin_account(&bytes, super::AdminPallet::PrivateAdmins)
        .expect("private institution admin account must decode unified account/cid/name layout");
    assert_eq!(decoded.institution_code, *b"SFGY");
    assert_eq!(decoded.admins[0].account_id, [0x24; 32]);
    assert_eq!(
        decoded.admins[0].cid_number,
        "CN220-CTZN2-198805200-2026".as_bytes()
    );
    assert_eq!(decoded.admins[0].family_name, "程".as_bytes());
    assert_eq!(decoded.admins[0].given_name, "伟".as_bytes());

    let account_only = adminbase::InstitutionAdmins {
        institution_code: *b"SFGY",
        admins: vec![adminbase::Admin {
            account_id: [0x25u8; 32],
            cid_number: Default::default(),
            family_name: Default::default(),
            given_name: Default::default(),
        }],
    }
    .encode();
    let decoded_account_only =
        super::decode_onchain_admin_account(&account_only, super::AdminPallet::PrivateAdmins)
            .expect("private non-LR admin may remain account-only");
    assert!(decoded_account_only.admins[0].cid_number.is_empty());
    assert!(decoded_account_only.admins[0].family_name.is_empty());
    assert!(decoded_account_only.admins[0].given_name.is_empty());
}

#[test]
fn frozen_public_admin_without_cid_uses_roster_account_only() {
    let mut decoded = super::OnChainAdminAccount {
        institution_code: *b"FRG\0",
        admins: vec![super::OnChainAdminRecord {
            account_id: [0x31; 32],
            cid_number: Vec::new(),
            family_name: Vec::new(),
            given_name: Vec::new(),
        }],
    };
    assert!(super::matching_roster_admin_by_cid_or_public_account(
        &decoded,
        super::AdminPallet::PublicAdmins,
        &[0x31; 32],
        Some(b"CN220-CTZN2-198805200-2026"),
    )
    .is_some());

    // 一旦名册已有管理员 CID，旧名册账户不得再作为无 CID 例外回退。
    decoded.admins[0].cid_number = b"CN220-CTZN2-198805200-2026".to_vec();
    assert!(super::matching_roster_admin_by_cid_or_public_account(
        &decoded,
        super::AdminPallet::PublicAdmins,
        &[0x31; 32],
        Some(b"CN221-CTZN2-198805200-2026"),
    )
    .is_none());
}

#[test]
fn console_pallets_allow_njd_and_block_desktop_governance() {
    assert_eq!(
        super::console_admin_pallets(b"NJD\0").unwrap(),
        vec![super::AdminPallet::PublicAdmins]
    );

    for code in [b"NRC\0", b"PRC\0", b"PRB\0"] {
        assert_eq!(
            super::console_admin_pallets(code).unwrap_err(),
            super::DESKTOP_GOVERNANCE_LOGIN_UNSUPPORTED
        );
        // 遍历到受阻机构并不代表目标账户属于该机构；只有实际命中名册才返回边界错误。
        assert_eq!(
            super::console_login_block_reason_for_membership(code, false),
            None
        );
        assert_eq!(
            super::console_login_block_reason_for_membership(code, true),
            Some(super::DESKTOP_GOVERNANCE_LOGIN_UNSUPPORTED)
        );
    }
}

#[test]
fn console_pallets_keep_unincorporated_dual_probe_and_personal_rejected() {
    assert_eq!(
        super::console_admin_pallets(b"UNIN").unwrap(),
        vec![
            super::AdminPallet::PublicAdmins,
            super::AdminPallet::PrivateAdmins
        ]
    );
    assert_eq!(
        super::console_admin_pallets(b"PMUL").unwrap_err(),
        super::PERSONAL_MULTISIG_LOGIN_UNSUPPORTED
    );
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
