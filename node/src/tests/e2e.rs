//! 第5步隔离候选的真实HTTPS只读验收；无写链操作。

#[test]
#[ignore = "第5步交付隔离HTTPS候选后显式执行"]
fn isolated_node_https_genesis_and_health() {
    let endpoint = std::env::var("CITIZENCHAIN_NODE_TEST_RPC").expect("缺少隔离候选HTTPS入口");
    let url = reqwest::Url::parse(&endpoint).expect("隔离候选入口格式无效");
    assert!(
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.path() == "/",
        "隔离入口必须是无凭据或参数的HTTPS根地址"
    );
    assert!(
        url.host_str().is_some_and(|host| host.contains("test")),
        "隔离候选必须使用明确测试主机名"
    );
    let request = |method, params| {
        nodedesktop::rpc::rpc_post_url(
            &endpoint,
            method,
            params,
            std::time::Duration::from_secs(10),
            64 * 1024,
        )
        .expect("隔离HTTPS只读请求失败")
    };
    let hash = request("chain_getBlockHash", serde_json::json!([0]));
    assert_eq!(
        hash,
        serde_json::json!(format!(
            "0x{}",
            hex::encode(primitives::genesis::GENESIS_HASH)
        ))
    );
    let health = request("system_health", serde_json::json!([]));
    assert!(health
        .get("isSyncing")
        .is_some_and(serde_json::Value::is_boolean));
}
