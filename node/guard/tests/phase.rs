//! 验证固定创世声明和版本保护，不冻结正常业务字段。

use super::*;
use std::collections::BTreeMap;
fn facts() -> BTreeMap<Vec<u8>, Vec<u8>> {
    BTreeMap::from([
        (
            storage_key::citizens_declaration(),
            CITIZENS.as_bytes().to_vec().encode(),
        ),
        (
            storage_key::country_declaration(),
            COUNTRY.as_bytes().to_vec().encode(),
        ),
        (storage_key::citizen_max(), GENESIS_CITIZEN_MAX.encode()),
    ])
}
#[test]
fn fixed_declarations_and_version_are_enforced() {
    let mut s = facts();
    check_genesis(|k| s.get(k).cloned()).unwrap();
    check_full_state(|k| s.get(k).cloned()).unwrap();
    s.insert(storage_key::citizen_max(), 0u64.encode());
    assert!(check_full_state(|k| s.get(k).cloned()).is_err());
    let mut s = facts();
    s.insert(storage_key::storage_version(), 1u16.encode());
    assert!(check_full_state(|k| s.get(k).cloned()).is_err());
    let mut s = facts();
    s.insert(storage_key::country_declaration(), b"wrong".to_vec());
    assert!(check_full_state(|k| s.get(k).cloned()).is_err());
}
#[test]
fn fixed_fact_writes_and_unknown_keys_are_rejected() {
    let s = facts();
    let read = |k: &[u8]| s.get(k).cloned();
    let d = BTreeMap::from([(
        storage_key::citizen_max(),
        Some(GENESIS_CITIZEN_MAX.encode()),
    )]);
    assert!(check_transition(&d, &read, &read).is_err());
    let mut key = storage_key::pallet_prefix().to_vec();
    key.extend([7; 16]);
    let d = BTreeMap::from([(key, Some(vec![0]))]);
    assert!(check_transition(&d, &read, &read).is_err());
}
#[test]
fn known_business_fields_are_not_frozen_by_the_node_guard() {
    let s = facts();
    let mut after = s.clone();
    after.insert(storage_key::phase(), vec![0]);
    after.insert(storage_key::developer_upgrade_enabled(), true.encode());
    let delta = BTreeMap::from([
        (storage_key::phase(), Some(vec![0])),
        (
            storage_key::developer_upgrade_enabled(),
            Some(true.encode()),
        ),
    ]);
    check_transition(&delta, &|k| s.get(k).cloned(), &|k| after.get(k).cloned()).unwrap();
}
