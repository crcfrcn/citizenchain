use super::*;

#[test]
fn catalog_bootnode_lookup_and_home_identity_share_the_same_role() {
    let options = crate::bootnodes::genesis_bootnode_options().unwrap();
    assert!(!options.is_empty());
    for option in options {
        assert!(!option.peer_id.is_empty());
        assert!(crate::bootnodes::is_genesis_bootnode_peer_id(&option.peer_id).unwrap());
        assert_eq!(
            crate::bootnodes::find_genesis_bootnode_cid_short_name_by_peer_id(&option.peer_id)
                .unwrap()
                .as_deref(),
            Some(option.cid_short_name.as_str())
        );
        assert_eq!(
            role_from_peer_id(Some(&option.peer_id)),
            option.cid_short_name
        );
    }
}
