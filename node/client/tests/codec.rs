//! 验证UTF-8截断、SCALE紧凑数及账户数据解码边界。

use super::*;

#[test]
fn storage_locator_uses_the_metadata_owner_for_pow_without_changing_values() {
    let original = runtime::Runtime::metadata_at_version(16).unwrap();
    let mut metadata = decode_runtime_metadata(original.as_slice()).unwrap();
    let frame_metadata::RuntimeMetadata::V16(value) = &mut metadata else {
        panic!("V16")
    };
    let pallet = value
        .pallets
        .iter_mut()
        .find(|p| p.name == "Difficulty")
        .unwrap();
    pallet.name = "ExecutedDifficulty".into();
    pallet.storage.as_mut().unwrap().prefix = "ExecutedDifficulty".into();
    let bytes =
        frame_metadata::RuntimeMetadataPrefixed(frame_metadata::META_RESERVED, metadata).encode();
    let layout = RuntimeStorage::from_metadata(&bytes).unwrap();
    let key = nodedesktop::storage::prefix(b"Difficulty", b"CurrentDifficulty");
    let actual = nodedesktop::storage::prefix(b"ExecutedDifficulty", b"CurrentDifficulty");
    assert_eq!(layout.key(&key, true), actual);
    assert_eq!(layout.key(&actual, false), key);
    let raw = std::collections::BTreeMap::from([(actual, Some(123u64.encode()))]);
    assert_eq!(layout.changes(&raw).unwrap()[&key], Some(123u64.encode()));
}
use codec::Encode;

#[test]
fn storage_locator_uses_storage_prefix_independently_of_display_name() {
    let original = runtime::Runtime::metadata_at_version(16).unwrap();
    let mut metadata = decode_runtime_metadata(original.as_slice()).unwrap();
    let RuntimeMetadata::V16(value) = &mut metadata else {
        panic!("V16");
    };
    let pallet = value
        .pallets
        .iter_mut()
        .find(|p| p.name == "Genesis")
        .unwrap();
    pallet.name = "DisplayGenesis".into();
    pallet.storage.as_mut().unwrap().prefix = "PhysicalGenesis".into();
    let bytes = RuntimeMetadataPrefixed(META_RESERVED, metadata).encode();
    let layout = RuntimeStorage::from_metadata(&bytes).unwrap();
    let key = nodedesktop::storage::prefix(b"Genesis", b"CitizensDeclaration");
    let physical = nodedesktop::storage::prefix(b"PhysicalGenesis", b"CitizensDeclaration");
    assert_eq!(layout.key(&key, true), physical);
    assert_eq!(layout.key(&physical, false), key);
}

#[test]
fn entity_layout_follows_value_type_even_when_pallet_names_change() {
    let bytes = runtime::Runtime::metadata_at_version(16).unwrap();
    let mut metadata = decode_runtime_metadata(bytes.as_slice()).unwrap();
    let RuntimeMetadata::V16(value) = &mut metadata else {
        panic!("V16");
    };
    let pallet = value.pallets.iter_mut().find(|p| p.index == 30).unwrap();
    pallet.name = "ExecutedPublic".into();
    pallet.storage.as_mut().unwrap().prefix = "ExecutedPublic".into();
    let bytes = frame_metadata::RuntimeMetadataPrefixed(
        frame_metadata::META_RESERVED,
        RuntimeMetadata::V16(value.clone()),
    )
    .encode();
    let layout = RuntimeStorage::from_metadata(&bytes).unwrap();
    assert!(!layout.entity_legacy(true).unwrap());
    assert!(!layout.entity_legacy(false).unwrap());
    let RuntimeMetadata::V16(value) = &mut metadata else {
        panic!("V16");
    };
    let entry = value
        .pallets
        .iter_mut()
        .find(|p| p.index == 30)
        .unwrap()
        .storage
        .as_mut()
        .unwrap()
        .entries
        .iter_mut()
        .find(|e| e.name == "Institutions")
        .unwrap();
    let frame_metadata::v14::StorageEntryType::Map { value, .. } = &mut entry.ty else {
        panic!("map");
    };
    value.id = u32::MAX;
    let bytes =
        frame_metadata::RuntimeMetadataPrefixed(frame_metadata::META_RESERVED, metadata).encode();
    assert!(RuntimeStorage::from_metadata(&bytes).is_err());
}

#[test]
fn verified_prefix_move_is_not_an_immutable_write_but_ordinary_same_value_write_remains() {
    let original = runtime::Runtime::metadata_at_version(16).unwrap();
    let mut metadata = decode_runtime_metadata(original.as_slice()).unwrap();
    let RuntimeMetadata::V16(value) = &mut metadata else {
        panic!("V16");
    };
    let genesis = value
        .pallets
        .iter_mut()
        .find(|p| p.name == "Genesis")
        .unwrap();
    genesis.name = "GenesisPallet".into();
    genesis.storage.as_mut().unwrap().prefix = "GenesisPallet".into();
    let bytes =
        frame_metadata::RuntimeMetadataPrefixed(frame_metadata::META_RESERVED, metadata).encode();
    let before = RuntimeStorage::from_metadata(&bytes).unwrap();
    let after = RuntimeStorage::from_metadata(original.as_slice()).unwrap();
    let old = nodedesktop::storage::prefix(b"GenesisPallet", b"CitizensDeclaration");
    let key = nodedesktop::storage::prefix(b"Genesis", b"CitizensDeclaration");
    let value = primitives::genesis::CITIZENS.as_bytes().to_vec().encode();
    let state = BTreeMap::from([(old.clone(), value.clone())]);
    let read = |k: &[u8]| Ok(state.get(k).cloned());
    let moved = BTreeMap::from([(old.clone(), None), (key.clone(), Some(value.clone()))]);
    assert!(before
        .transition_changes(&after, &moved, read)
        .unwrap()
        .is_empty());
    let ordinary = BTreeMap::from([(old.clone(), Some(value.clone()))]);
    assert_eq!(
        before.transition_changes(&before, &ordinary, read).unwrap()[&key],
        Some(value)
    );
    let tampered = BTreeMap::from([(old.clone(), None), (key.clone(), Some(vec![1]))]);
    assert_eq!(
        before.transition_changes(&after, &tampered, read).unwrap()[&key],
        Some(vec![1])
    );
    let shadow = BTreeMap::from([(old, Some(vec![1])), (key, Some(vec![1]))]);
    assert!(before.transition_changes(&after, &shadow, read).is_err());
}

#[test]
fn verified_prefix_move_then_delete_preserves_deletion_and_rejects_false_sources() {
    let original = runtime::Runtime::metadata_at_version(16).unwrap();
    let current = RuntimeStorage::from_metadata(original.as_slice()).unwrap();
    for (name, source, table) in [
        ("Difficulty", "PowDifficulty", "PendingParams"),
        ("Genesis", "GenesisPallet", "CitizensDeclaration"),
    ] {
        let layout = |owner: &str| {
            let mut metadata = decode_runtime_metadata(original.as_slice()).unwrap();
            let RuntimeMetadata::V16(value) = &mut metadata else {
                panic!("V16");
            };
            let pallet = value.pallets.iter_mut().find(|p| p.name == name).unwrap();
            pallet.name = owner.into();
            pallet.storage.as_mut().unwrap().prefix = owner.into();
            RuntimeStorage::from_metadata(
                &RuntimeMetadataPrefixed(META_RESERVED, metadata).encode(),
            )
            .unwrap()
        };
        let before = layout(source);
        let old = nodedesktop::storage::prefix(source.as_bytes(), table.as_bytes());
        let target = nodedesktop::storage::prefix(name.as_bytes(), table.as_bytes());
        let state = BTreeMap::from([(old.clone(), vec![1])]);
        let read = |key: &[u8]| Ok(state.get(key).cloned());
        let raw = BTreeMap::from([(old.clone(), None), (target.clone(), None)]);
        assert_eq!(
            before.transition_changes(&current, &raw, read).unwrap(),
            BTreeMap::from([(target.clone(), None)]),
            "搬迁后消费必须留下真实逻辑删除，永久记录的删除仍交给所属守卫拒绝"
        );
        assert!(
            before
                .transition_changes(&current, &raw, |_| Ok(None))
                .is_err(),
            "不存在的来源不能证明搬迁"
        );
        let conflict = BTreeMap::from([(old.clone(), vec![1]), (target.clone(), vec![1])]);
        assert!(
            before
                .transition_changes(&current, &raw, |key| Ok(conflict.get(key).cloned()))
                .is_err(),
            "不能用双删除吞掉父状态中的来源冲突"
        );
        let retained = BTreeMap::from([(old.clone(), Some(vec![1])), (target.clone(), None)]);
        assert!(before
            .transition_changes(&current, &retained, read)
            .is_err());
        assert!(before.transition_changes(&before, &raw, read).is_err());
        for failed in [&old, &target] {
            let result = before.transition_changes(&current, &raw, |key| {
                if key == failed {
                    Err("读取失败".into())
                } else {
                    read(key)
                }
            });
            assert_eq!(result.unwrap_err(), "读取失败");
        }
        let unknown = layout("UndeclaredSource");
        let fake = nodedesktop::storage::prefix(b"UndeclaredSource", table.as_bytes());
        assert!(unknown
            .transition_changes(
                &current,
                &BTreeMap::from([(fake.clone(), None), (target.clone(), None)]),
                |key| Ok((key == fake).then(|| vec![1])),
            )
            .is_err());
        let third = nodedesktop::storage::prefix(b"OtherTarget", table.as_bytes());
        let mut extra = raw.clone();
        extra.insert(third, None);
        assert!(before
            .transition_changes(&layout("OtherTarget"), &extra, read)
            .is_err());
    }
}

#[test]
fn migration_views_enumerate_both_sources_and_reject_conflicts_retired_keys_and_read_errors() {
    let metadata = runtime::Runtime::metadata_at_version(16).unwrap();
    let mut layout = RuntimeStorage::from_metadata(metadata.as_slice()).unwrap();
    let table = nodedesktop::storage::prefix(b"PublicEntity", b"Institutions");
    let source = nodedesktop::storage::prefix(b"PublicManage", b"Institutions");
    assert_eq!(layout.physical_prefixes(&table), vec![table.clone()]);
    // 测试阶段选择与Runtime实际进度编码的核验分开；后者在Runtime真实迁移测试覆盖。
    layout.migration = runtime::MigrationState::Running;
    assert_eq!(
        layout.physical_prefixes(&table),
        vec![table.clone(), source.clone()]
    );
    let state = BTreeMap::from([(source.clone(), vec![1])]);
    let row = layout
        .read_raw(&table, |k| Ok(state.get(k).cloned()))
        .unwrap()
        .unwrap();
    assert!(row.legacy);
    assert_eq!(row.bytes, vec![1]);
    assert!(layout.read_raw(&table, |_| Err("读取失败".into())).is_err());
    let shadow = BTreeMap::from([(source.clone(), vec![1]), (table.clone(), vec![1])]);
    assert!(layout
        .read_raw(&table, |k| Ok(shadow.get(k).cloned()))
        .is_err());
    layout.migration = runtime::MigrationState::Complete;
    assert_eq!(layout.physical_prefixes(&table), vec![table]);
    assert!(layout.logical_key(&source).is_err());
    assert!(layout
        .logical_key(&nodedesktop::storage::prefix(
            b"PublicAdmins",
            b"AdminAccounts"
        ))
        .is_err());
    assert!(RuntimeStorage::from_metadata(metadata.as_slice())
        .unwrap()
        .with_state(|_| Err("读取失败".into()))
        .is_err());
    assert!(RuntimeStorage::from_metadata(metadata.as_slice())
        .unwrap()
        .with_state(|_| Ok(Some(vec![255])))
        .is_err());
}

#[test]
fn migration_completion_accepts_only_real_source_deletes_and_frame_version_initialization() {
    let metadata = runtime::Runtime::metadata_at_version(16).unwrap();
    let mut before = RuntimeStorage::from_metadata(metadata.as_slice()).unwrap();
    before.migration = runtime::MigrationState::Running;
    let mut after = RuntimeStorage::from_metadata(metadata.as_slice()).unwrap();
    after.migration = runtime::MigrationState::Complete;
    let source = nodedesktop::storage::prefix(b"PublicManage", b":__STORAGE_VERSION__:");
    let target = nodedesktop::storage::prefix(b"PublicEntity", b":__STORAGE_VERSION__:");
    let admin = nodedesktop::storage::prefix(b"PublicAdmins", b":__STORAGE_VERSION__:");
    let state = BTreeMap::from([
        (source.clone(), 0u16.encode()),
        (target.clone(), 1u16.encode()),
        (admin.clone(), 0u16.encode()),
    ]);
    let read = |key: &[u8]| Ok(state.get(key).cloned());
    let record = before.read_raw(&target, read).unwrap().unwrap();
    assert!(!record.legacy);
    assert_eq!(record.bytes, 1u16.encode());
    let raw = BTreeMap::from([
        (source.clone(), None),
        (target.clone(), Some(1u16.encode())),
        (admin.clone(), None),
    ]);
    let changes = before.transition_changes(&after, &raw, read).unwrap();
    assert!(
        !changes.contains_key(&target),
        "FRAME版本初始化不冒充机构业务写入"
    );
    assert_eq!(changes.get(&admin), Some(&None));
    let rewrite = BTreeMap::from([(source.clone(), Some(0u16.encode()))]);
    assert!(before.transition_changes(&after, &rewrite, read).is_err());
    assert!(
        after.transition_changes(&after, &raw, read).is_err(),
        "完成后不得再触及旧来源"
    );
    for (old, new) in [(vec![0], 1u16.encode()), (0u16.encode(), 2u16.encode())] {
        let bad = BTreeMap::from([(source.clone(), old), (target.clone(), new)]);
        assert!(before
            .read_raw(&target, |key| Ok(bad.get(key).cloned()))
            .is_err());
    }
}

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

// 总额由真实 AccountInfo 编码回读，不能把冻结额重复算入资产。
#[test]
fn account_balance_decodes_total_and_rejects_invalid_storage() {
    let mut account = AccountInfo::default();
    account.data.free = 300_111;
    account.data.reserved = 200;
    account.data.frozen = 500;
    let bytes = account.encode();
    let decoded = decode_account(&bytes).unwrap();
    assert_eq!(decoded.data.free, 300_111);
    assert_eq!(decoded.data.reserved, 200);
    assert_eq!(decoded.data.frozen, 500);
    assert_eq!(account_total(&decoded).unwrap(), 300_311);
    for end in 0..bytes.len() {
        assert!(decode_account(&bytes[..end]).is_err());
    }
    let mut trailing = bytes;
    trailing.push(0);
    assert!(decode_account(&trailing).is_err());
    assert_eq!(account_total(&AccountInfo::default()).unwrap(), 0);
    account.data.free = u128::MAX;
    assert!(account_total(&account).is_err());
}
