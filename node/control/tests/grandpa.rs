use crate::grandpa::cid_short_name_for_cid;
use crate::grandpa::load_institution_catalog;
use crate::grandpa::StoredGrandpaMeta;
use crate::grandpa::INSTITUTION_CATALOG_SRC;
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
