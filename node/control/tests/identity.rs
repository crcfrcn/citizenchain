//! 验证缺失或未知PeerID不被误识别为机构节点。

use super::*;

#[test]
fn absent_and_unrecognized_peer_never_claim_an_institution_role() {
    assert_eq!(role_from_peer_id(None), "全节点");
    assert_eq!(role_from_peer_id(Some("unrecognized-test-peer")), "全节点");
}
