use super::*;

use crate::service::ethereum_rpc_available;

use crate::service::rpc_stage_error;

use crate::service::mining_ready_transactions;

use runtime;

#[test]
fn rpc_stage_preserves_original_error_chain() {
    let original = std::io::Error::other("固定上游错误(4003)");
    let error = rpc_stage_error("Ethereum客户端初始化", original);
    assert_eq!(
        error.to_string(),
        "Ethereum客户端初始化: 固定上游错误(4003)"
    );
    assert_eq!(error.source().unwrap().to_string(), "固定上游错误(4003)");
}

#[test]
fn ethereum_rpc_requires_runtime_revive_after_genesis() {
    let metadata: Vec<u8> = runtime::Runtime::metadata().into();
    assert_eq!(ethereum_rpc_available(&metadata, 0), Ok(true));
    assert_eq!(ethereum_rpc_available(&metadata, 1), Ok(true));

    let mut input = metadata.as_slice();
    let mut old = RuntimeMetadataPrefixed::decode(&mut input).expect("metadata decodes");
    assert!(input.is_empty());
    match &mut old.1 {
        RuntimeMetadata::V14(value) => value.pallets.retain(|pallet| pallet.name != "Revive"),
        RuntimeMetadata::V15(value) => value.pallets.retain(|pallet| pallet.name != "Revive"),
        RuntimeMetadata::V16(value) => value.pallets.retain(|pallet| pallet.name != "Revive"),
        _ => panic!("unsupported metadata version"),
    }
    let old_metadata = old.encode();
    assert_eq!(ethereum_rpc_available(&old_metadata, 0), Ok(false));
    assert!(ethereum_rpc_available(&old_metadata, 1).is_err());
    assert!(ethereum_rpc_available(&[0, 1, 2], 0).is_err());

    let mut trailing = metadata;
    trailing.push(0);
    assert!(ethereum_rpc_available(&trailing, 0).is_err());
}

#[test]
fn mining_ready_transactions_keeps_nonempty_pool_when_not_major_syncing() {
    // peer 状态不再是参数：没有 peer 不能抹掉交易池中的有效交易。
    assert_eq!(mining_ready_transactions(3, false), 3);
}

#[test]
fn mining_ready_transactions_keeps_empty_pool_blocked() {
    assert_eq!(mining_ready_transactions(0, false), 0);
}

#[test]
fn mining_ready_transactions_blocks_during_major_sync() {
    assert_eq!(mining_ready_transactions(3, true), 0);
}
