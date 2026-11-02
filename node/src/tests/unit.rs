#[test]
fn node_product_version_is_identical_in_cli_ui_and_tauri() {
    use sc_cli::SubstrateCli;
    let version = nodecontrol::rpc::NODE_VERSION;
    assert_eq!(<crate::cli::Cli as SubstrateCli>::impl_version(), version);
    // 官方测试上下文避免重复嵌入macOS系统plist，版本仍读取同一真实配置。
    let context: tauri::Context<tauri::Wry> =
        tauri::generate_context!("tauri.conf.json", test = true);
    assert_eq!(context.package_info().version.to_string(), version);
}

// 最小绑定仅从同一次快照回调读取四项真实存储；缺失/吊销/坏编码/单向映射均不通过。
#[test]
fn identity_binding_uses_finalized_snapshot_and_rejects_invalid_facts() {
    use codec::Encode;
    use std::collections::BTreeMap;
    let cid: identity::CidNumberBound = b"CN220-CTZN2-198805201-2026".to_vec().try_into().unwrap();
    let account = runtime::AccountId::new([7; 32]);
    let info = sp_blockchain::Info::<runtime::opaque::Block> {
        best_hash: runtime::Hash::repeat_byte(10),
        best_number: 10,
        genesis_hash: runtime::Hash::zero(),
        finalized_hash: runtime::Hash::repeat_byte(9),
        finalized_number: 9,
        finalized_state: Some((runtime::Hash::repeat_byte(9), 9)),
        number_leaves: 1,
        block_gap: None,
    };
    let registry = identity::CidRecord::<u32> {
        registrar_cid_number: cid.clone(),
        commitment: [0; 32],
        residence_province_code: Default::default(),
        residence_city_code: Default::default(),
        status: identity::CidRecordStatus::Active,
        registered_at: 1,
        revoked_at: None,
    };
    let registry_key = identity::CidRegistry::<runtime::Runtime>::hashed_key_for(&cid);
    let forward_key = identity::AccountIdByCid::<runtime::Runtime>::hashed_key_for(&cid);
    let reverse_key = identity::CidByAccountId::<runtime::Runtime>::hashed_key_for(&account);
    let revision_key = identity::BindingRevisionByCid::<runtime::Runtime>::hashed_key_for(&cid);
    let mut data = BTreeMap::from([
        (registry_key.clone(), registry.encode()),
        (forward_key.clone(), account.encode()),
        (reverse_key.clone(), cid.encode()),
        (revision_key.clone(), 1u64.encode()),
    ]);
    let mut calls = 0;
    let value =
        crate::rpc::identity_binding(&cid, &account, info.genesis_hash, &info, |at, key| {
            assert_eq!(at, info.finalized_hash);
            assert_ne!(at, info.best_hash);
            calls += 1;
            Ok(data.get(&key).cloned())
        })
        .unwrap()
        .unwrap();
    assert_eq!(calls, 4);
    assert_eq!(
        value,
        serde_json::json!({"block":format!("0x{}","09".repeat(32)),"cid_number":std::str::from_utf8(cid.as_slice()).unwrap(),"account_id":format!("0x{}","07".repeat(32)),"bind_rev":1})
    );
    for key in [
        forward_key.clone(),
        reverse_key.clone(),
        revision_key.clone(),
    ] {
        let saved = data.remove(&key).unwrap();
        assert!(crate::rpc::identity_binding(
            &cid,
            &account,
            info.genesis_hash,
            &info,
            |_at, key| Ok(data.get(&key).cloned())
        )
        .is_err());
        data.insert(key, saved);
    }
    for (key, bytes) in [
        (
            forward_key.clone(),
            runtime::AccountId::new([8; 32]).encode(),
        ),
        (reverse_key, b"CN220-CTZN2-198805202-2026".to_vec().encode()),
        (revision_key, 0u64.encode()),
    ] {
        let saved = data.insert(key.clone(), bytes).unwrap();
        assert!(crate::rpc::identity_binding(
            &cid,
            &account,
            info.genesis_hash,
            &info,
            |_at, key| Ok(data.get(&key).cloned())
        )
        .is_err());
        data.insert(key, saved);
    }
    let error = crate::rpc::identity_binding(
        &cid,
        &account,
        runtime::Hash::repeat_byte(1),
        &info,
        |_, _| panic!("错误创世链不得读取存储"),
    )
    .unwrap_err();
    assert_eq!(error.code(), -32013);
    let error = crate::rpc::identity_binding(&cid, &account, info.genesis_hash, &info, |_, _| {
        Err(jsonrpsee::types::ErrorObjectOwned::owned(
            -32010, "offline", None::<()>,
        ))
    })
    .unwrap_err();
    assert_eq!(error.code(), -32010);
    let mut broken = registry.encode();
    broken.push(0);
    data.insert(registry_key.clone(), broken);
    assert!(
        crate::rpc::identity_binding(&cid, &account, info.genesis_hash, &info, |_at, key| Ok(
            data.get(&key).cloned()
        ))
        .is_err()
    );
    let mut revoked = registry;
    revoked.status = identity::CidRecordStatus::Revoked;
    revoked.revoked_at = Some(2);
    data.insert(registry_key.clone(), revoked.encode());
    assert!(
        crate::rpc::identity_binding(&cid, &account, info.genesis_hash, &info, |_at, key| Ok(
            data.get(&key).cloned()
        ))
        .is_err()
    );
    data.remove(&registry_key);
    assert!(
        crate::rpc::identity_binding(&cid, &account, info.genesis_hash, &info, |_at, key| Ok(
            data.get(&key).cloned()
        ))
        .unwrap()
        .is_none()
    );
}
