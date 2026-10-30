use super::*;

#[test]
#[ignore = "第5步固定机器显式账本预算"]
fn ledger_deposit_and_snapshot_fixed_workload_budget() {
    let budget: u128 = std::env::var("CITIZENCHAIN_LEDGER_PERF_MS")
        .expect("缺少账本性能预算")
        .parse()
        .expect("性能预算必须为正整数毫秒");
    assert!(budget > 0);
    // 不持久化、不创建目录；只测本包内存余额/汇总路径。
    let ledger = OffchainLedger::new(&std::env::temp_dir());
    let account = AccountId32::new([17u8; 32]);
    let start = std::time::Instant::now();
    for _ in 0..100_000 {
        ledger.on_deposited(std::hint::black_box(&account), 1);
    }
    assert_eq!(ledger.confirmed_sum_snapshot(), 100_000);
    assert_eq!(ledger.available_balance(&account), 100_000);
    assert!(
        start.elapsed().as_millis() <= budget,
        "固定账本工作量超过显式预算"
    );
}
