#[test]
#[ignore = "第5步固定机器显式服务装配预算"]
fn native_partial_service_assembly_budget() {
    let budget: u128 = std::env::var("CITIZENCHAIN_SERVICE_PERF_MS")
        .expect("缺少服务性能预算")
        .parse()
        .expect("性能预算必须为正整数毫秒");
    assert!(budget > 0);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let config = super::p2p_bad_block_tests::test_config("performance", runtime.handle().clone());
    let path = config.base_path.path().to_path_buf();
    let start = std::time::Instant::now();
    let partial = super::testing::new_partial_for_test(&config).expect("真实原生partial服务装配");
    assert!(
        start.elapsed().as_millis() <= budget,
        "真实服务装配超过显式预算"
    );
    drop(partial);
    drop(config);
    drop(runtime);
    std::fs::remove_dir_all(path).unwrap();
}
