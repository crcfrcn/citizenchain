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
            assert_ok!(Onchain::transfer_with_remark(
                RuntimeOrigin::signed(from.clone()),
                to.clone(),
                1,
                remark.clone()
            ));
        }
        let average = started.elapsed().as_nanos() / count;
        assert_eq!(Balances::free_balance(from), 889_000);
        assert_eq!(Balances::free_balance(to), 1_001_000);
        let burnt=System::events().iter().filter_map(|e|match e.event {RuntimeEvent::Onchain(onchain::pallet::Event::FeeShareBurnt {amount,..})=>Some(amount),_=>None}).sum::<u128>();
        assert_eq!(Balances::total_issuance(), issuance - burnt);
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
        assert_eq!(Genesis::phase(), ::genesis::ChainPhase::Genesis);
    });
    eprintln!("完整内存初始化：{elapsed}ns，上限{budget}ns");
    assert!(elapsed <= budget, "完整内存初始化超过声明预算");
}

/// 不替代上面的预算门禁：只交付原始实测，预算未提供时绝不伪称性能通过。
#[test]
#[ignore = "显式执行原生内存实测；没有预算时仅报告，不判定预算通过"]
fn report_native_measurements_without_invented_budget() {
    let mut transfer = Vec::new();
    let mut genesis_times = Vec::new();
    for sample in 0..3 {
        let started = std::time::Instant::now();
        let storage = RuntimeGenesisConfig::default()
            .build_storage()
            .expect("完整创世可初始化");
        let genesis_ns = started.elapsed().as_nanos();
        sp_io::TestExternalities::new(storage).execute_with(|| {
            assert_eq!(
                publicentity::Institutions::<Runtime>::iter().count(),
                EXPECTED_PUBLIC_INSTITUTIONS
            );
            assert_eq!(Genesis::phase(), ::genesis::ChainPhase::Genesis);
        });
        genesis_times.push(genesis_ns);
        new_test_ext().execute_with(|| {
            let from = AccountId::new([37;32]); let to = AccountId::new([38;32]);
            let _ = Balances::make_free_balance_be(&from, 1_000_000);
            let _ = Balances::make_free_balance_be(&to, 1_000_000);
            let issuance = Balances::total_issuance();
            let remark = onchain::pallet::TransferRemarkOf::<Runtime>::try_from(b"performance".to_vec()).unwrap();
            let started = std::time::Instant::now();
            for _ in 0..1000 { assert_ok!(Onchain::transfer_with_remark(RuntimeOrigin::signed(from.clone()),to.clone(),1,remark.clone())); }
            let average = started.elapsed().as_nanos()/1000;
            assert_eq!(Balances::free_balance(from),889_000); assert_eq!(Balances::free_balance(to),1_001_000); let burnt=System::events().iter().filter_map(|e|match e.event {RuntimeEvent::Onchain(onchain::pallet::Event::FeeShareBurnt {amount,..})=>Some(amount),_=>None}).sum::<u128>();
            assert_eq!(Balances::total_issuance(),issuance-burnt);
            transfer.push(average);
            eprintln!("MEASUREMENT_ONLY sample={sample} transfer_avg_ns={average} genesis_build_ns={genesis_ns} debug_assertions={}",cfg!(debug_assertions));
        });
    }
    transfer.sort_unstable();
    genesis_times.sort_unstable();
    eprintln!("MEASUREMENT_SUMMARY transfer_ns={transfer:?} genesis_ns={genesis_times:?} budget_gate=NOT_EVALUATED");
}

#[cfg(feature = "runtime-benchmarks")]
#[test]
#[ignore = "真实Runtime原生登记计时，不是WASM/数据库权重生成"]
fn report_registration_native_measurements() {
    new_test_ext().execute_with(|| {
        for existing in [false,true] {
            for (n,r,p,a) in [(2,1,2,2),(16,2,4,8),(128,8,4,128),(1989,8,4,1989)] {
                for sample in 0..3 {
                    sp_io::storage::start_transaction();
                    let (public_ns,public_work)=publicentity::benchmarks::measure_registration::<Runtime>(n,r,p,a,existing);
                    sp_io::storage::rollback_transaction();
                    sp_io::storage::start_transaction();
                    let (private_ns,private_work)=privateentity::benchmarks::measure_registration::<Runtime>(n,r,p,a,existing);
                    sp_io::storage::rollback_transaction();
                    eprintln!("REGISTRATION_NATIVE existing={existing} n={n} r={r} p={p} a={a} sample={sample} public_ns={public_ns} public_work={public_work:?} private_ns={private_ns} private_work={private_work:?}");
                }
            }
        }
    });
}

#[cfg(feature = "runtime-benchmarks")]
#[test]
#[ignore = "实名管理员运行期基准，逐人验证真实身份绑定；只报告实测"]
fn report_operation_registration_measurements() {
    new_test_ext().execute_with(|| {
        for (n,r,p,a) in [(2,1,2,2),(128,2,4,2),(512,2,4,2)] {
            for sample in 0..3 {
                sp_io::storage::start_transaction();
                let (ns,work)=publicentity::benchmarks::measure_operation_registration::<Runtime>(n,r,p,a);
                assert_eq!(Genesis::phase(),::genesis::ChainPhase::Operation);
                // ABI元组由真实identity存储类型读取，不能只靠手工编码夹具自证。
                let bound=identity::CidByAccountId::<Runtime>::iter().find(|(_,cid)|cid.as_slice().starts_with(b"CN001-CTZN")).unwrap();
                assert_eq!(identity::AccountIdByCid::<Runtime>::get(&bound.1),Some(bound.0.clone()));
                assert!(identity::Pallet::<Runtime>::citizen_subject(&bound.0).is_some());
                sp_io::storage::rollback_transaction();
                eprintln!("REGISTRATION_OPERATION n={n} r={r} p={p} a={a} sample={sample} ns={ns} work={work:?}");
            }
        }
    });
}

#[test]
fn operation_binding_proof_bounds_come_from_current_runtime_metadata() {
    use frame_support::traits::StorageInfoTrait;
    let names: &[&[u8]] = &[
        b"CidByAccountId",
        b"AccountIdByCid",
        b"VotingIdentityByCid",
        b"CidRegistry",
    ];
    let mut sizes = Vec::new();
    for item in identity::Pallet::<Runtime>::storage_info() {
        if names.contains(&item.storage_name.as_slice()) {
            let size = item.max_size.expect("身份绑定值必须有最大编码长度");
            eprintln!(
                "IDENTITY_PROOF_METADATA name={} max_size={size} max_values={:?}",
                String::from_utf8_lossy(&item.storage_name),
                item.max_values
            );
            sizes.push(size);
        }
    }
    assert_eq!(sizes.len(), 4);
    // FRAME writer默认1,000,000项对应5层，每层15个33字节散列；沿用其MEL策略。
    let proof_per_citizen = sizes.into_iter().sum::<u32>() + 4 * 5 * 15 * 33;
    eprintln!("IDENTITY_PROOF_PER_CITIZEN_BYTES={proof_per_citizen}");
    // 直接核对Runtime元数据与正式权重的增长量；元数据变更不得静默低估证明。
    for (low, high) in [
        (<publicentity::weights::SubstrateWeight<Runtime> as publicentity::weights::WeightInfo>::register_institution_roles(2, 2, 4, 2),
         <publicentity::weights::SubstrateWeight<Runtime> as publicentity::weights::WeightInfo>::register_institution_roles(128, 2, 4, 2)),
        (<privateentity::weights::SubstrateWeight<Runtime> as privateentity::weights::WeightInfo>::register_institution_roles(2, 2, 4, 2),
         <privateentity::weights::SubstrateWeight<Runtime> as privateentity::weights::WeightInfo>::register_institution_roles(128, 2, 4, 2)),
    ] {
        assert!(high.proof_size() - low.proof_size() >= u64::from(proof_per_citizen) * 126);
        assert!(high.ref_time() >= 28_494_000_000, "不得低于本轮最慢Operation实测耗时");
    }
}
