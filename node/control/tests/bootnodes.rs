use crate::bootnodes::StoredBootnodeMeta;

#[test]
fn old_duplicate_name_field_is_rejected_instead_of_silently_accepted() {
    let old_field = concat!("authority", "NodeLabel");
    let raw = format!(r#"{{"peer_id":"peer","{old_field}":"旧名称"}}"#);
    assert!(serde_json::from_str::<StoredBootnodeMeta>(&raw).is_err());
}
