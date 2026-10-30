use crate::dashboard::MiningComputationCache;

#[test]
fn mining_cache_rejects_unknown_field() {
    let current =
        serde_json::to_value(crate::dashboard::MiningComputationCache::default()).unwrap();

    let mut stale = current;
    stale["unexpected_field"] = serde_json::json!(true);
    assert!(serde_json::from_value::<MiningComputationCache>(stale).is_err());
}
