//! 原所属模块回归，断言与输入保持。
use super::*;

#[test]
fn submit_rebind_input_uses_exact_current_and_new_account_fields() {
    let current_account_id = format!("0x{}", "11".repeat(32));
    let new_account_id = format!("0x{}", "22".repeat(32));
    let input: SubmitCitizenRebindInput = serde_json::from_value(serde_json::json!({
        "requestId": "request-1",
        "currentAccountId": current_account_id,
        "currentAccountSignature": format!("0x{}", "33".repeat(64)),
        "newAccountId": new_account_id,
        "newAccountSignature": format!("0x{}", "44".repeat(64)),
    }))
    .expect("official account field names must deserialize");
    assert_eq!(
        input.current_account_id.as_deref(),
        Some(current_account_id.as_str())
    );
    assert_eq!(input.new_account_id, new_account_id);

    let alias_result = serde_json::from_value::<SubmitCitizenRebindInput>(serde_json::json!({
        "requestId": "request-1",
        "currentAccountId": current_account_id,
        "currentAccountSignature": format!("0x{}", "33".repeat(64)),
        "newAccountId": format!("0x{}", "22".repeat(32)),
        "newAccountSignature": format!("0x{}", "44".repeat(64)),
        "accountId": format!("0x{}", "22".repeat(32)),
        "rebindSignature": format!("0x{}", "44".repeat(64)),
    }));
    assert!(alias_result.is_err(), "废弃泛化字段不得作为兼容输入");
}

fn finalized_snapshot(
    cid_status: FinalizedCidStatus,
    account_id: Option<[u8; 32]>,
    binding_revision: Option<u64>,
    chain_now_seconds: u64,
) -> FinalizedCidBinding {
    FinalizedCidBinding {
        genesis_hash: [0xaau8; 32],
        finalized_block_hash: [0xbbu8; 32],
        finalized_block_number: 9,
        chain_now_seconds,
        cid_status,
        registrar_cid_number: (cid_status != FinalizedCidStatus::Missing)
            .then(|| b"CN001-CREG-0001".to_vec()),
        commitment: (cid_status != FinalizedCidStatus::Missing).then_some([0x55u8; 32]),
        residence_province_code: b"GD".to_vec(),
        residence_city_code: b"001".to_vec(),
        account_id,
        binding_revision,
        identity_version: 0,
        voting: None,
        candidate: None,
    }
}

#[test]
fn authorization_expiry_uses_chain_time_and_keeps_runtime_margin() {
    // 服务端时钟不参与授权到期值；只以同一 finalized 快照中的链秒+300 签发。
    assert_eq!(authorization_expires_at(1_000), Some(1_300));
    assert!(authorization_is_live(1_000, 1_300));
    assert!(authorization_is_live(1_299, 1_300));
    assert!(!authorization_is_live(1_300, 1_300));
    // 即使传入未来超过 Runtime 最大窗口的时间也必须 fail-closed。
    assert!(!authorization_is_live(1_000, 1_601));
    assert_eq!(authorization_expires_at(u64::MAX), None);
}

#[test]
fn revoked_or_inconsistent_cid_never_becomes_rebind_source() {
    let account_id = [0x11u8; 32];
    assert_eq!(
        finalized_snapshot(
            FinalizedCidStatus::Revoked,
            Some(account_id),
            Some(4),
            1_000
        )
        .active_binding(),
        None
    );
    assert_eq!(
        finalized_snapshot(FinalizedCidStatus::Active, Some(account_id), None, 1_000)
            .active_binding(),
        None
    );
    assert!(finalized_snapshot(FinalizedCidStatus::Missing, None, None, 1_000).is_unoccupied());
}

#[test]
fn finalized_target_rejects_pending_failed_and_accepts_only_exact_target() {
    let current_account_id = [0x11u8; 32];
    let new_account_id = [0x22u8; 32];
    let expectation = FinalizedBindingExpectation {
        genesis_hash: [0xaau8; 32],
        current_binding: Some((current_account_id, 7)),
        target_account_id: new_account_id,
        target_binding_revision: 8,
        occupy_record: None,
        authorization_expires_at: 1_300,
    };
    // nonce 即使已被 txpool 推进，只要 finalized storage 仍是交易前绑定就只能等待。
    assert_eq!(
        finalized_target_state(
            &finalized_snapshot(
                FinalizedCidStatus::Active,
                Some(current_account_id),
                Some(7),
                1_100,
            ),
            &expectation,
        ),
        FinalizedTargetState::Pending
    );
    assert_eq!(
        finalized_target_state(
            &finalized_snapshot(
                FinalizedCidStatus::Active,
                Some(new_account_id),
                Some(8),
                1_200,
            ),
            &expectation,
        ),
        FinalizedTargetState::Confirmed
    );
    // 失败交易、并发换绑或确认超时都不得冒充成功。
    assert_eq!(
        finalized_target_state(
            &finalized_snapshot(
                FinalizedCidStatus::Active,
                Some(current_account_id),
                Some(7),
                1_300,
            ),
            &expectation,
        ),
        FinalizedTargetState::Conflict
    );
    assert_eq!(
        finalized_target_state(
            &finalized_snapshot(
                FinalizedCidStatus::Active,
                Some([0x33u8; 32]),
                Some(8),
                1_200,
            ),
            &expectation,
        ),
        FinalizedTargetState::Conflict
    );
}

#[test]
fn finalized_occupy_target_requires_same_registrar_and_account_commitment() {
    let account_id = [0x22u8; 32];
    let registrar = b"CN001-CREG-0001".to_vec();
    let commitment = blake2_256(&account_id);
    let expectation = FinalizedBindingExpectation {
        genesis_hash: [0xaau8; 32],
        current_binding: None,
        target_account_id: account_id,
        target_binding_revision: 1,
        occupy_record: Some((registrar.clone(), commitment)),
        authorization_expires_at: 1_300,
    };
    let mut snapshot =
        finalized_snapshot(FinalizedCidStatus::Active, Some(account_id), Some(1), 1_200);
    snapshot.registrar_cid_number = Some(registrar);
    snapshot.commitment = Some(commitment);
    assert_eq!(
        finalized_target_state(&snapshot, &expectation),
        FinalizedTargetState::Confirmed
    );

    snapshot.registrar_cid_number = Some(b"CN999-FRG-0001".to_vec());
    assert_eq!(
        finalized_target_state(&snapshot, &expectation),
        FinalizedTargetState::Conflict
    );
    snapshot.registrar_cid_number = Some(b"CN001-CREG-0001".to_vec());
    snapshot.commitment = Some([0x99u8; 32]);
    assert_eq!(
        finalized_target_state(&snapshot, &expectation),
        FinalizedTargetState::Conflict
    );
}

/// 占号调用字节 golden:锁死链↔onchina 字节契约(pallet 10 / call 6 占即绑新签名)。
/// 布局 = [10][6] Compact(len)+actor_cid ‖ Compact(len)+actor_role ‖ Compact(len)+cid
///        ‖ account_id(32 裸字节) ‖ expires_at(u64 LE) ‖ Compact(len)+occupy_signature。
#[test]
fn encode_occupy_cid_call_byte_golden() {
    let account_id = [0x11u8; 32];
    let occupy_signature = [0x22u8; 4];
    let out = encode_occupy_cid_call(
        "A",
        "B",
        "C",
        &account_id,
        0x0102_0304_0506_0708,
        &occupy_signature,
    );
    // 0a06 | 04 41 | 04 42 | 04 43 | 11*32 | 0807060504030201 | 10 | 22*4
    let expected = concat!(
        "0a06",
        "0441",
        "0442",
        "0443",
        "1111111111111111111111111111111111111111111111111111111111111111",
        "0807060504030201",
        "10",
        "22222222",
    );
    assert_eq!(hex::encode(out), expected);
}

/// 换绑调用字节 golden:pallet 10 / call 7,锁定 revision/expires 的 LE 顺序。
#[test]
fn encode_admin_rebind_cid_account_id_call_byte_golden() {
    let new_account_id = [0x11u8; 32];
    let new_account_signature = [0x22u8; 4];
    let out = encode_admin_rebind_cid_account_id_call(
        "A",
        "B",
        "C",
        &new_account_id,
        0x1112_1314_1516_1718,
        0x0102_0304_0506_0708,
        &new_account_signature,
    );
    // ... | new_account | revision LE | expires LE | Compact(sig len) | sig
    let expected = concat!(
        "0a07",
        "0441",
        "0442",
        "0443",
        "1111111111111111111111111111111111111111111111111111111111111111",
        "1817161514131211",
        "0807060504030201",
        "10",
        "22222222",
    );
    assert_eq!(hex::encode(out), expected);
}

/// 占号验签往返:真签名通过、篡改 cid 拒、换绑域验签器验占号签名拒(域分离防重放)。
#[test]
fn verify_occupy_signature_roundtrip_and_domain_separation() {
    let pair = sr25519::Pair::from_seed(&[7u8; 32]);
    let account = pair.public().0;
    let account_hex = format!("0x{}", hex::encode(account));
    let cid = "CN220-CTZN2-198805200-2026";
    let genesis_hash = [0xabu8; 32];
    let expires_at = 1_800_000_000;
    let payload = encode_cid_occupy_authorization(&genesis_hash, cid, &account, expires_at);
    let msg = primitives::sign::signing_message(primitives::sign::OP_SIGN_CID_OCCUPY, &payload);
    let sig_hex = format!("0x{}", hex::encode(pair.sign(&msg).0));

    assert!(verify_occupy_signature(
        account_hex.as_str(),
        &payload,
        sig_hex.as_str()
    ));
    // 篡改 CID、创世哈希、过期时间均拒。
    let changed_cid = encode_cid_occupy_authorization(
        &genesis_hash,
        "CN220-CTZN2-000000000-2026",
        &account,
        expires_at,
    );
    assert!(!verify_occupy_signature(
        account_hex.as_str(),
        &changed_cid,
        sig_hex.as_str()
    ));
    let changed_genesis = encode_cid_occupy_authorization(&[0xacu8; 32], cid, &account, expires_at);
    assert!(!verify_occupy_signature(
        account_hex.as_str(),
        &changed_genesis,
        sig_hex.as_str()
    ));
    let changed_expiry =
        encode_cid_occupy_authorization(&genesis_hash, cid, &account, expires_at + 1);
    assert!(!verify_occupy_signature(
        account_hex.as_str(),
        &changed_expiry,
        sig_hex.as_str()
    ));
    // 占号签名拿去过换绑域验签器 → 拒(0x12 与 0x1F 分离)。
    assert!(!verify_admin_rebind_signature(
        account_hex.as_str(),
        &payload,
        sig_hex.as_str()
    ));
}

/// 换绑验签往返 + 反向域分离。
#[test]
fn verify_admin_rebind_signature_roundtrip_and_domain_separation() {
    let pair = sr25519::Pair::from_seed(&[9u8; 32]);
    let account = pair.public().0;
    let account_hex = format!("0x{}", hex::encode(account));
    let cid = "CN330-NATP3-111111111-2026";
    let genesis_hash = [0xcdu8; 32];
    let current_account = [0x44u8; 32];
    let expected_binding_revision = 7;
    let expires_at = 1_800_000_000;
    let payload = encode_cid_rebind_authorization(
        &genesis_hash,
        cid,
        &current_account,
        &account,
        expected_binding_revision,
        expires_at,
    );
    let msg =
        primitives::sign::signing_message(primitives::sign::OP_SIGN_CID_ADMIN_REBIND, &payload);
    let sig_hex = format!("0x{}", hex::encode(pair.sign(&msg).0));

    assert!(verify_admin_rebind_signature(
        account_hex.as_str(),
        &payload,
        sig_hex.as_str()
    ));
    for changed in [
        encode_cid_rebind_authorization(
            &[0xceu8; 32],
            cid,
            &current_account,
            &account,
            expected_binding_revision,
            expires_at,
        ),
        encode_cid_rebind_authorization(
            &genesis_hash,
            cid,
            &[0x45u8; 32],
            &account,
            expected_binding_revision,
            expires_at,
        ),
        encode_cid_rebind_authorization(
            &genesis_hash,
            cid,
            &current_account,
            &account,
            expected_binding_revision + 1,
            expires_at,
        ),
        encode_cid_rebind_authorization(
            &genesis_hash,
            cid,
            &current_account,
            &account,
            expected_binding_revision,
            expires_at + 1,
        ),
    ] {
        assert!(!verify_admin_rebind_signature(
            account_hex.as_str(),
            &changed,
            sig_hex.as_str()
        ));
    }
    // 换绑签名拿去过占号域验签器 → 拒。
    assert!(!verify_occupy_signature(
        account_hex.as_str(),
        &payload,
        sig_hex.as_str()
    ));
}

#[test]
fn verify_current_account_signature_requires_chain_current_account_and_own_domain() {
    let current_pair = sr25519::Pair::from_seed(&[7u8; 32]);
    let new_pair = sr25519::Pair::from_seed(&[8u8; 32]);
    let current_account = current_pair.public().0;
    let new_account = new_pair.public().0;
    let current_account_hex = format!("0x{}", hex::encode(current_account));
    let payload = encode_cid_rebind_authorization(
        &[0xaau8; 32],
        "CN330-NATP3-111111111-2026",
        &current_account,
        &new_account,
        7,
        1_800_000_000,
    );
    let current_message =
        primitives::sign::signing_message(primitives::sign::OP_SIGN_CID_REBIND, &payload);
    let current_account_signature =
        format!("0x{}", hex::encode(current_pair.sign(&current_message).0));

    assert!(verify_current_account_signature(
        current_account_hex.as_str(),
        &payload,
        current_account_signature.as_str()
    ));
    assert!(!verify_current_account_signature(
        format!("0x{}", hex::encode(new_account)).as_str(),
        &payload,
        current_account_signature.as_str()
    ));
    assert!(!verify_admin_rebind_signature(
        current_account_hex.as_str(),
        &payload,
        current_account_signature.as_str()
    ));
}
