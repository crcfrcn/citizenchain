use super::*;

use crate::network::cid_count_storage_key;
use crate::network::collect_local_peers;
use crate::network::count_observed_institutions;
use crate::network::normalize_peer_id;
use crate::network::NetworkOverview;
use sp_crypto_hashing::twox_128;

// 合法 PeerId 示例：以 "12D3KooW" 开头 + 至少 46 字符 + 纯 ASCII 字母数字
const VALID_PEER_ID: &str = "12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN";

#[test]
fn local_peers_are_deduplicated_and_classified_by_their_actual_roles() {
    let light_id = VALID_PEER_ID.replace("XTN", "XTM");
    let (full, light, invalid) = collect_local_peers(&serde_json::json!([
        {"peerId": VALID_PEER_ID, "roles": "FULL"},
        {"peerId": VALID_PEER_ID, "roles": ["FULL"]},
        {"peerId": light_id, "roles": "LIGHT"},
        {"peerId": light_id, "roles": ["LIGHT"]},
        {"peerId": "invalid", "roles": "FULL"},
        {"roles": "FULL"}
    ]))
    .unwrap();
    assert_eq!(full, HashSet::from([VALID_PEER_ID.to_string()]));
    assert_eq!(light, HashSet::from([light_id]));
    assert_eq!(invalid, 2);
}

#[test]
fn zero_local_connections_are_valid_but_malformed_results_are_errors() {
    let (full, light, invalid) = collect_local_peers(&serde_json::json!([])).unwrap();
    assert!(full.is_empty() && light.is_empty());
    assert_eq!(invalid, 0);
    assert!(collect_local_peers(&Value::Null).is_err());
    assert!(collect_local_peers(&serde_json::json!({})).is_err());
}

#[test]
fn institution_catalog_is_only_an_identity_map_not_an_online_node_list() {
    let province = VALID_PEER_ID.replace("XTN", "XTM");
    let bank = VALID_PEER_ID.replace("XTN", "XTL");
    let roles = HashMap::from([
        (VALID_PEER_ID.to_string(), "nrc".to_string()),
        (province.clone(), "prc".to_string()),
        (bank.clone(), "prb".to_string()),
    ]);
    let full = HashSet::from([VALID_PEER_ID.to_string(), "ordinary-full-node".to_string()]);
    assert_eq!(
        count_observed_institutions(&full, &HashSet::new(), &roles),
        (1, 0, 0, 0)
    );
    assert_eq!(
        count_observed_institutions(&full, &HashSet::from([province, bank]), &roles),
        (1, 1, 1, 0)
    );
}

/// 存储键必须与链端 `CitizenIdentity::CidCount` 逐字节一致。
///
/// pallet 或 storage 改名后，`state_getStorage` 只会安静地返回 null，
/// 挖矿页轻节点一格随之常年显示 0 —— 没有任何报错。本测试是唯一的早期告警。
#[test]
fn cid_count_storage_key_matches_chain_pallet_and_storage_name() {
    let key = cid_count_storage_key();
    assert_eq!(key.len(), 32);
    assert_eq!(&key[..16], twox_128(b"CitizenIdentity").as_slice());
    assert_eq!(&key[16..], twox_128(b"CidCount").as_slice());
}

/// 链端 `CidCount` 是 `u64`；节点按 8 字节 SCALE 解码，多一字节即判非规范。
#[test]
fn cid_count_decodes_exactly_eight_scale_bytes() {
    use codec::Encode;

    let encoded = 4_294_967_296u64.encode();
    assert_eq!(encoded.len(), 8);
    let mut input = encoded.as_slice();
    assert_eq!(u64::decode(&mut input).expect("decode"), 4_294_967_296u64);
    assert!(input.is_empty());
}

#[test]
fn normalize_peer_id_valid() {
    assert_eq!(
        normalize_peer_id(VALID_PEER_ID),
        Some(VALID_PEER_ID.to_string())
    );
}

#[test]
fn normalize_peer_id_trims_whitespace() {
    let padded = format!("  {VALID_PEER_ID}  ");
    assert_eq!(normalize_peer_id(&padded), Some(VALID_PEER_ID.to_string()));
}

#[test]
fn normalize_peer_id_empty_rejected() {
    assert_eq!(normalize_peer_id(""), None);
    assert_eq!(normalize_peer_id("   "), None);
}

#[test]
fn normalize_peer_id_too_long_rejected() {
    let long = format!("12D3KooW{}", "a".repeat(121)); // 8 + 121 = 129 > 128
    assert_eq!(normalize_peer_id(&long), None);
}

#[test]
fn normalize_peer_id_max_length_ok() {
    let max = format!("12D3KooW{}", "a".repeat(120)); // 8 + 120 = 128
    assert!(normalize_peer_id(&max).is_some());
}

#[test]
fn normalize_peer_id_too_short_rejected() {
    // 以 12D3KooW 开头但不足 46 字符
    assert_eq!(normalize_peer_id("12D3KooWShort"), None);
}

#[test]
fn normalize_peer_id_non_alphanumeric_rejected() {
    assert_eq!(normalize_peer_id("12D3/KooW"), None);
    assert_eq!(normalize_peer_id("peer-id"), None);
}

#[test]
fn network_overview_json_excludes_removed_cards() {
    let overview = NetworkOverview {
        online_nodes: 2,
        nrc_nodes: 1,
        prc_nodes: 0,
        prb_nodes: 0,
        full_nodes: 1,
        light_nodes: 1,
        warning: None,
    };
    let json = serde_json::to_value(&overview).expect("NetworkOverview should serialize");
    let object = json
        .as_object()
        .expect("NetworkOverview should be an object");

    assert!(!object.contains_key("totalNodes"));
    assert!(!object.contains_key("clearingNodes"));
    assert!(object.contains_key("onlineNodes"));
    assert!(object.contains_key("fullNodes"));
    assert!(object.contains_key("lightNodes"));
}
