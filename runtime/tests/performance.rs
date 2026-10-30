//! 原生pallet直接转账基准，不含签名扩展、调用过滤、交易池、网络和共识成本。
//! 统一环境提供已确认的平均耗时上限；缺失阈值不允许当作性能通过。
use super::*;

#[test]
#[ignore = "第5步固定性能环境并交付CITIZENCHAIN_PERF_TRANSFER_NS阈值后执行"]
fn native_transfer_dispatch_meets_declared_budget_and_preserves_balances() {
    let budget = std::env::var("CITIZENCHAIN_PERF_TRANSFER_NS")
        .expect("必须显式提供性能阈值")
        .parse::<u128>()
        .expect("阈值必须是纳秒整数");
    assert!(budget > 0);
    new_test_ext().execute_with(|| {
        let from = AccountId::new([37; 32]);
        let to = AccountId::new([38; 32]);
        let _ = Balances::make_free_balance_be(&from, 1_000_000);
        let _ = Balances::make_free_balance_be(&to, 1_000_000);
        let issuance = Balances::total_issuance();
        let remark =
            onchain::pallet::TransferRemarkOf::<Runtime>::try_from(b"performance".to_vec())
                .unwrap();
        let count = 1_000u128;
        let started = std::time::Instant::now();
        for _ in 0..count {
            assert_ok!(OnchainTransaction::transfer_with_remark(
                RuntimeOrigin::signed(from.clone()),
                to.clone(),
                1,
                remark.clone()
            ));
        }
        let average = started.elapsed().as_nanos() / count;
        assert_eq!(Balances::free_balance(from), 999_000);
        assert_eq!(Balances::free_balance(to), 1_001_000);
        assert_eq!(Balances::total_issuance(), issuance);
        eprintln!("原生Runtime直接派发：{count}次，平均{average}ns，上限{budget}ns");
        assert!(average <= budget, "原生转账派发超过已声明性能预算");
    });
}

/// 实际整链默认配置的内存初始化成本；不写冻结链规格、不创建磁盘链数据。
#[test]
#[ignore = "第5步固定环境并提供CITIZENCHAIN_PERF_GENESIS_NS阈值后执行"]
fn full_genesis_initialization_meets_budget_and_preserves_fixed_catalog() {
    let budget = std::env::var("CITIZENCHAIN_PERF_GENESIS_NS")
        .expect("必须交付性能阈值")
        .parse::<u128>()
        .expect("阈值必须为纳秒整数");
    assert!(budget > 0);
    let start = std::time::Instant::now();
    let storage = RuntimeGenesisConfig::default()
        .build_storage()
        .expect("完整默认配置可初始化");
    let elapsed = start.elapsed().as_nanos();
    sp_io::TestExternalities::new(storage).execute_with(|| {
        assert_eq!(
            publicentity::Institutions::<Runtime>::iter().count(),
            EXPECTED_PUBLIC_INSTITUTIONS
        );
        assert_eq!(GenesisPallet::phase(), ::genesis::ChainPhase::Genesis);
    });
    eprintln!("完整内存初始化：{elapsed}ns，上限{budget}ns");
    assert!(elapsed <= budget, "完整内存初始化超过声明预算");
}
