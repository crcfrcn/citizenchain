use super::*;
use sc_rpc_api;

fn network_module(peers: serde_json::Value) -> Arc<jsonrpsee::RpcModule<()>> {
    let mut module = jsonrpsee::RpcModule::new(());
    module.extensions_mut().insert(sc_rpc_api::DenyUnsafe::No);
    module
        .register_method("system_health", |_, _, _| {
            serde_json::json!({
                "peers": 1, "isSyncing": false, "shouldHavePeers": true,
            })
        })
        .unwrap();
    module
        .register_method("system_peers", move |_, _, extensions| {
            sc_rpc_api::check_if_safe(extensions).map(|_| peers.clone())
        })
        .unwrap();
    module
        .register_method("system_unstable_networkState", |_, _, extensions| {
            sc_rpc_api::check_if_safe(extensions).map(|_| serde_json::json!({"connectedPeers": {}}))
        })
        .unwrap();
    Arc::new(module)
}

#[test]
fn local_network_reader_reads_fixed_native_methods_including_zero_peers() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let reader = crate::network::LocalNetworkReader::new();
    assert!(reader.peers().unwrap_err().contains("尚未就绪"));
    reader
        .attach(
            network_module(serde_json::json!([])),
            runtime.handle().clone(),
        )
        .unwrap();
    let snapshot = reader.sync_snapshot().unwrap();
    assert_eq!(snapshot.peers, serde_json::json!([]));
    assert_eq!(snapshot.health["shouldHavePeers"], true);
    assert!(snapshot.network_state["connectedPeers"].is_object());
    reader.close();
}

#[test]
fn local_network_reader_close_releases_rpc_and_restart_does_not_revive_old_handles() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let reader = crate::network::LocalNetworkReader::new();
    let old = reader.clone();
    let rpc = network_module(serde_json::json!([]));
    let weak = Arc::downgrade(&rpc);
    reader.attach(rpc, runtime.handle().clone()).unwrap();
    reader.close();
    assert!(weak.upgrade().is_none());
    assert!(old.peers().unwrap_err().contains("已关闭"));
    assert!(old
        .attach(
            network_module(serde_json::json!([])),
            runtime.handle().clone()
        )
        .is_err());
    let restarted = crate::network::LocalNetworkReader::new();
    restarted
        .attach(
            network_module(serde_json::json!([{"peerId":"new-session"}])),
            runtime.handle().clone(),
        )
        .unwrap();
    assert_eq!(restarted.peers().unwrap().as_array().unwrap().len(), 1);
    assert!(old.sync_snapshot().is_err());
    restarted.close();
}

#[test]
fn local_network_reader_discards_late_response_after_close() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let started = entered.clone();
    let finish = release.clone();
    let mut module = jsonrpsee::RpcModule::new(());
    module
        .register_async_method("system_peers", move |_, _, _| {
            let started = started.clone();
            let finish = finish.clone();
            async move {
                started.notify_one();
                finish.notified().await;
                serde_json::json!([{"peerId":"stale"}])
            }
        })
        .unwrap();
    let reader = crate::network::LocalNetworkReader::new();
    reader
        .attach(Arc::new(module), runtime.handle().clone())
        .unwrap();
    let pending = reader.clone();
    let query = thread::spawn(move || pending.peers());
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(2), entered.notified())
            .await
            .unwrap();
    });
    reader.close();
    release.notify_one();
    assert!(query.join().unwrap().unwrap_err().contains("已关闭"));
}

#[test]
fn local_network_reader_does_not_turn_malformed_peer_response_into_zero() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let reader = crate::network::LocalNetworkReader::new();
    reader
        .attach(
            network_module(serde_json::Value::Null),
            runtime.handle().clone(),
        )
        .unwrap();
    assert!(reader.peers().unwrap_err().contains("返回格式无效"));
    reader.close();
}
