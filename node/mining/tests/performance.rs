use super::*;

#[test]
#[ignore = "第5步固定机器显式哈希预算"]
fn pow_hash_fixed_workload_budget() {
    let budget: u128 = std::env::var("CITIZENCHAIN_POW_PERF_MS")
        .expect("缺少哈希性能预算")
        .parse()
        .expect("性能预算必须为正整数毫秒");
    assert!(budget > 0);
    let start = std::time::Instant::now();
    let pre_hash = [17u8; 32];
    for nonce in 0..100_000u64 {
        std::hint::black_box(pow_hash(
            std::hint::black_box(&pre_hash),
            std::hint::black_box(nonce),
        ));
    }
    assert!(
        start.elapsed().as_millis() <= budget,
        "固定哈希工作量超过显式预算"
    );
}
