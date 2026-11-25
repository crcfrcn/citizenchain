//! 验证GRANDPA目录使用规范CID简称并拒绝重复名称字段。

use crate::key::local::cid_short_name_for_cid;
use crate::key::local::load_institution_catalog;
use crate::key::local::StoredGrandpaMeta;
use crate::key::local::INSTITUTION_CATALOG_SRC;
use primitives::cid::china::china_cb::CHINA_CB;
use std::collections::HashSet;

#[test]
fn every_grandpa_cid_uses_runtime_cid_short_name_without_duplicate_catalog_name() {
    assert!(!INSTITUTION_CATALOG_SRC.contains(concat!("authority", "NodeLabel")));
    assert!(!INSTITUTION_CATALOG_SRC.contains("cidShortName"));
    let cid_short_names = CHINA_CB
        .iter()
        .map(|entry| {
            let actual = cid_short_name_for_cid(entry.cid_number)
                .expect("每个 NRC/PRC CID 都必须匹配一个权威节点");
            assert_eq!(actual, entry.cid_short_name);
            actual
        })
        .collect::<HashSet<_>>();
    assert_eq!(cid_short_names.len(), CHINA_CB.len());
    assert_eq!(load_institution_catalog().unwrap().len(), CHINA_CB.len());
}

#[test]
fn old_duplicate_name_field_is_rejected_instead_of_silently_accepted() {
    let old_field = concat!("authority", "NodeLabel");
    let raw = format!(r#"{{"{old_field}":"旧名称","public_key":null}}"#);
    assert!(serde_json::from_str::<StoredGrandpaMeta>(&raw).is_err());
}

#[test]
fn catalog_uses_cids_without_any_static_public_key_copy() {
    let rows: serde_json::Value = serde_json::from_str(INSTITUTION_CATALOG_SRC).unwrap();
    for row in rows.as_array().unwrap() {
        assert!(row.get("grandpa_public_key").is_none());
        assert!(row.get("cidNumber").and_then(|v| v.as_str()).is_some());
    }
}
#[test]
fn registered_public_key_requires_exact_chain_forward_and_reverse_indices() {
    use codec::Encode;
    use std::collections::BTreeMap;
    let public = [7u8; 32];
    let cid = CHINA_CB[0].cid_number;
    let reverse = nodedesktop::storage::to_hex(&nodedesktop::storage::blake2_map(
        b"GrandpaKey",
        b"GrandpaKeyOwnerByKey",
        &public.encode(),
    ));
    let forward = nodedesktop::storage::to_hex(&nodedesktop::storage::blake2_map(
        b"GrandpaKey",
        b"CurrentGrandpaKeys",
        &cid.as_bytes().to_vec().encode(),
    ));
    let mut rows = BTreeMap::from([
        (
            reverse.clone(),
            format!("0x{}", hex::encode(cid.as_bytes().to_vec().encode())),
        ),
        (
            forward.clone(),
            format!("0x{}", hex::encode(public.encode())),
        ),
    ]);
    let key = format!("0x{}", hex::encode(public));
    assert_eq!(
        super::registered_owner(&key, |k| Ok(rows.get(k).cloned())).unwrap(),
        Some(cid.into())
    );
    rows.insert(forward.clone(), format!("0x{}", hex::encode([8u8; 32])));
    assert!(super::registered_owner(&key, |k| Ok(rows.get(k).cloned())).is_err());
    rows.remove(&forward);
    assert!(super::registered_owner(&key, |k| Ok(rows.get(k).cloned())).is_err());
    rows.clear();
    assert_eq!(
        super::registered_owner(&key, |k| Ok(rows.get(k).cloned())).unwrap(),
        None
    );
}
