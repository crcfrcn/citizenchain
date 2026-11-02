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
fn identity_binding_validates_both_maps_revision_and_registry() {
    use codec::Encode;
    use std::collections::BTreeMap;
    let cid: identity::CidNumberBound = b"CN220-CTZN2-198805201-2026".to_vec().try_into().unwrap();
    let account = runtime::AccountId::new([7; 32]);
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
    let value = crate::rpc::identity_binding(&cid, &account, |key| {
        calls += 1;
        Ok(data.get(&key).cloned())
    })
    .unwrap()
    .unwrap();
    assert_eq!(calls, 4);
    assert_eq!(
        value,
        serde_json::json!({"cid_number":std::str::from_utf8(cid.as_slice()).unwrap(),"account_id":format!("0x{}","07".repeat(32)),"bind_rev":1})
    );
    for key in [forward_key, reverse_key, revision_key] {
        let saved = data.remove(&key).unwrap();
        assert!(
            crate::rpc::identity_binding(&cid, &account, |key| Ok(data.get(&key).cloned()))
                .is_err()
        );
        data.insert(key, saved);
    }
    let mut broken = registry.encode();
    broken.push(0);
    data.insert(registry_key.clone(), broken);
    assert!(
        crate::rpc::identity_binding(&cid, &account, |key| Ok(data.get(&key).cloned())).is_err()
    );
    let mut revoked = registry;
    revoked.status = identity::CidRecordStatus::Revoked;
    revoked.revoked_at = Some(2);
    data.insert(registry_key.clone(), revoked.encode());
    assert!(
        crate::rpc::identity_binding(&cid, &account, |key| Ok(data.get(&key).cloned())).is_err()
    );
    data.remove(&registry_key);
    assert!(
        crate::rpc::identity_binding(&cid, &account, |key| Ok(data.get(&key).cloned()))
            .unwrap()
            .is_none()
    );
}
