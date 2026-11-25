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
