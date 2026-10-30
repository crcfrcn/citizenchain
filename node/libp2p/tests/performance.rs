use super::*;

#[test]
#[ignore = "第5步固定机器显式地址解析预算"]
fn wss_address_parser_fixed_workload_budget() {
    let budget: u128 = std::env::var("CITIZENCHAIN_WSS_PERF_MS")
        .expect("缺少WSS解析预算")
        .parse()
        .expect("预算必须是正整数毫秒");
    assert!(budget > 0);
    let address: Multiaddr = "/dns4/example.test/tcp/443/wss".parse().unwrap();
    let start = std::time::Instant::now();
    for _ in 0..100_000 {
        let parsed =
            parse_ws_dial_addr::<std::io::Error>(std::hint::black_box(address.clone())).unwrap();
        assert!(parsed.use_tls);
        std::hint::black_box(parsed);
    }
    assert!(
        start.elapsed().as_millis() <= budget,
        "WSS地址解析固定工作量超过预算"
    );
}
