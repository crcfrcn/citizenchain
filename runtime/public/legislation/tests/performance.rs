//! 隔离状态下原生列表读取成本，不代表真实RPC、磁盘数据库或整链性能。
use super::*;

#[test]
#[ignore = "第5步固定环境并提供CITIZENCHAIN_PERF_LAWS_NS阈值后执行"]
fn native_scope_query_over_legacy_capacity_is_read_only_and_meets_budget() {
    let budget = std::env::var("CITIZENCHAIN_PERF_LAWS_NS")
        .expect("必须交付性能阈值")
        .parse::<u128>()
        .expect("阈值必须为纳秒整数");
    assert!(budget > 0);
    new_test_ext().execute_with(|| {
        let region = municipal_region();
        // 当前协议不设每地域法律总数上限，1001不能回退到旧BoundedVec容量。
        let ids = (1..=1001u64).collect::<Vec<_>>();
        LawsByScope::<Test>::insert(Tier::Municipal, region, ids.clone());
        let state = sp_io::storage::root(sp_runtime::StateVersion::V1);
        let start = std::time::Instant::now();
        for _ in 0..50 {
            let actual = std::hint::black_box(LegislationYuan::list_laws(Tier::Municipal, region));
            assert_eq!(actual, ids);
        }
        let average = start.elapsed().as_nanos() / 50;
        assert_eq!(sp_io::storage::root(sp_runtime::StateVersion::V1), state);
        eprintln!("1001项地域索引原生查询及校验：平均{average}ns，上限{budget}ns");
        assert!(average <= budget, "地域列表读取超过声明预算");
    });
}
