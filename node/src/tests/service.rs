//! 验证服务RPC错误链、映射审计及阶段失败反馈。

use super::*;

use crate::service::ethereum_rpc_available;

use crate::service::rpc_stage_error;

use crate::service::mining_ready_transactions;

use runtime;

/// 每个用例独占临时审计目录，不读取或写入正式节点数据。
struct MappingAuditDirectory(std::path::PathBuf);

impl MappingAuditDirectory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "gmb-mapping-audit-node-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn records(&self) -> Vec<serde_json::Value> {
        std::fs::read_to_string(self.0.join("security-audit.log"))
            .unwrap()
            .lines()
            .map(|line| {
                let (prefix, status) = line.split_once("\tstatus=").unwrap();
                assert!(prefix.ends_with("\taction=evm_mapping"));
                serde_json::from_str(status).unwrap()
            })
            .collect()
    }
}

impl Drop for MappingAuditDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

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

#[test]
fn mapping_backfill_requires_a_valid_completion_marker() {
    assert_eq!(mapping_cursor(None, Some(vec![1])).unwrap(), None);
    assert_eq!(
        mapping_cursor(Some(vec![1, 0]), None).unwrap(),
        Some(vec![1, 0])
    );
    for (cursor, marker) in [
        (None, None),
        (None, Some(vec![0])),
        (None, Some(vec![2])),
        (None, Some(vec![1, 0])),
        (Some(vec![]), None),
        (Some(vec![1, 0]), Some(vec![1])),
    ] {
        assert!(mapping_cursor(cursor, marker).is_err());
    }
}

#[test]
fn mapping_backfill_waits_for_sync_finality_and_ordinary_transactions() {
    assert!(mapping_idle(false, true, 0, 0));
    assert!(!mapping_idle(true, true, 0, 0));
    assert!(!mapping_idle(false, false, 0, 0));
    assert!(!mapping_idle(false, true, 1, 0));
    assert!(!mapping_idle(false, true, 0, 1));
    // 原有挖矿门控仍禁止空池构造区块。
    assert_eq!(mining_ready_transactions(0, false), 0);
}

#[test]
fn mapping_backfill_stops_on_a_stuck_cursor_and_finishes_only_at_completion() {
    assert!(mapping_progressed(&[1, 0], Some(vec![1, 0])).is_err());
    assert!(!mapping_progressed(&[1, 0], Some(vec![1, 1])).unwrap());
    assert!(mapping_progressed(&[1, 0], None).unwrap());
}

#[test]
fn mapping_backfill_requires_success_of_its_exact_extrinsic() {
    let record = frame_system::EventRecord::<runtime::RuntimeEvent, runtime::Hash> {
        phase: frame_system::Phase::ApplyExtrinsic(1),
        event: runtime::RuntimeEvent::System(frame_system::Event::ExtrinsicSuccess {
            dispatch_info: Default::default(),
        }),
        topics: vec![],
    };
    let mut bytes = vec![record].encode();
    assert!(mapping_succeeded(&bytes, 1).is_ok());
    assert!(mapping_succeeded(&bytes, 2).is_err());
    bytes.push(0);
    assert!(mapping_succeeded(&bytes, 1).is_err());
    assert!(mapping_succeeded(&[0], 1).is_err());
    assert!(mapping_succeeded(&[255], 1).is_err());
}

#[test]
fn mapping_backfill_remarks_bind_to_the_exact_cursor() {
    let runtime::RuntimeCall::System(frame_system::Call::remark { remark }) =
        mapping_remark(&[1, 0])
    else {
        panic!("维护只能构造既有System.remark");
    };
    assert_eq!(&remark[..20], b"evm-account-mapping:");
    assert_eq!(&remark[20..], &blake2_256(&[1, 0]));
    assert_ne!(
        mapping_remark(&[1, 0]).encode(),
        mapping_remark(&[1, 1]).encode()
    );
}

#[tokio::test]
async fn mapping_backfill_waits_for_finality_instead_of_inclusion() {
    use sc_transaction_pool_api::TransactionStatus;
    let hash = runtime::Hash::repeat_byte(7);
    let stream = futures::stream::iter([
        TransactionStatus::Ready,
        TransactionStatus::Broadcast(vec![]),
        TransactionStatus::InBlock((hash, 1)),
        TransactionStatus::Finalized((hash, 1)),
    ]);
    let directory = MappingAuditDirectory::new();
    let mut audit = MappingAudit::new(directory.0.clone());
    audit.tx = Some(format!("{hash:?}"));
    audit.step("waiting_finality", serde_json::json!({}));
    assert_eq!(
        mapping_finalized(Box::pin(stream), &mut audit)
            .await
            .unwrap(),
        (hash, 1)
    );
    let records = directory.records();
    let statuses = records
        .iter()
        .filter(|r| r["phase"] == "transaction_status")
        .collect::<Vec<_>>();
    assert_eq!(statuses.len(), 4);
    assert_eq!(statuses[0]["lastStatus"], "Ready");
    assert!(statuses[2]["lastStatus"]
        .as_str()
        .unwrap()
        .starts_with("InBlock("));
    assert!(statuses[3]["lastStatus"]
        .as_str()
        .unwrap()
        .starts_with("Finalized("));
    assert!(statuses
        .iter()
        .all(|r| r["at"] == "waiting_finality" && r["tx"] == format!("{hash:?}")));
    assert!(!records.iter().any(|r| r["phase"] == "complete"));
}

#[tokio::test]
async fn mapping_backfill_retracted_transaction_waits_for_its_canonical_finality() {
    use sc_transaction_pool_api::TransactionStatus;
    let old = runtime::Hash::repeat_byte(7);
    let canonical = runtime::Hash::repeat_byte(8);
    let tx = format!("{:?}", runtime::Hash::repeat_byte(9));
    let directory = MappingAuditDirectory::new();
    let mut audit = MappingAudit::new(directory.0.clone());
    audit.tx = Some(tx.clone());
    audit.step("waiting_finality", serde_json::json!({}));
    let stream = futures::stream::iter([
        TransactionStatus::Ready,
        TransactionStatus::InBlock((old, 1)),
        TransactionStatus::Retracted(old),
        TransactionStatus::Future,
        TransactionStatus::Ready,
        TransactionStatus::InBlock((canonical, 2)),
        TransactionStatus::Finalized((canonical, 2)),
    ]);
    assert_eq!(
        mapping_finalized(Box::pin(stream), &mut audit)
            .await
            .unwrap(),
        (canonical, 2),
        "只能返回同一交易在规范块中的最终位置"
    );
    let records = directory.records();
    let statuses = records
        .iter()
        .filter(|r| r["phase"] == "transaction_status")
        .collect::<Vec<_>>();
    assert_eq!(statuses.len(), 7);
    assert!(statuses[2]["lastStatus"]
        .as_str()
        .unwrap()
        .starts_with("Retracted("));
    assert!(statuses[6]["lastStatus"]
        .as_str()
        .unwrap()
        .starts_with("Finalized("));
    assert!(statuses
        .iter()
        .all(|r| r["tx"] == tx && r["at"] == "waiting_finality"));
    assert!(!records
        .iter()
        .any(|r| r["phase"] == "submitting" || r["phase"] == "failed" || r["phase"] == "complete"));
}

#[tokio::test]
async fn mapping_backfill_repeated_retractions_keep_the_same_pending_wait() {
    use futures::StreamExt;
    use sc_transaction_pool_api::TransactionStatus;
    let old = runtime::Hash::repeat_byte(7);
    let next = runtime::Hash::repeat_byte(8);
    let directory = MappingAuditDirectory::new();
    let base = directory.0.clone();
    let pending = async move {
        let mut audit = MappingAudit::new(base);
        audit.tx = Some(format!("{:?}", runtime::Hash::repeat_byte(9)));
        audit.step("waiting_finality", serde_json::json!({}));
        let stream = futures::stream::iter([
            TransactionStatus::InBlock((old, 1)),
            TransactionStatus::Retracted(old),
            TransactionStatus::Ready,
            TransactionStatus::InBlock((next, 1)),
            TransactionStatus::Retracted(next),
        ])
        .chain(futures::stream::pending());
        mapping_finalized(Box::pin(stream), &mut audit).await
    };
    // 同一次外层等待预算仍可取消 Future；撤块不能自行完成、退出或开启新等待。
    assert!(tokio::time::timeout(Duration::from_millis(10), pending)
        .await
        .is_err());
    let records = directory.records();
    assert_eq!(
        records
            .iter()
            .filter(|r| r["phase"] == "transaction_status")
            .count(),
        5
    );
    assert_eq!(records.last().unwrap()["phase"], "cancelled");
    assert!(records.last().unwrap()["lastStatus"]
        .as_str()
        .unwrap()
        .starts_with("Retracted("));
    assert!(!records
        .iter()
        .any(|r| r["phase"] == "submitting" || r["phase"] == "failed" || r["phase"] == "complete"));
}

#[tokio::test]
async fn mapping_backfill_stops_on_terminal_rejection_or_unknown_result() {
    use sc_transaction_pool_api::TransactionStatus;
    let hash = runtime::Hash::repeat_byte(7);
    for event in [
        TransactionStatus::Usurped(hash),
        TransactionStatus::Dropped,
        TransactionStatus::Invalid,
        TransactionStatus::FinalityTimeout(hash),
    ] {
        let directory = MappingAuditDirectory::new();
        let mut audit = MappingAudit::new(directory.0.clone());
        audit.step("waiting_finality", serde_json::json!({}));
        let expected = format!("{event:?}");
        let stream = futures::stream::iter([
            TransactionStatus::InBlock((hash, 1)),
            TransactionStatus::Retracted(hash),
            event,
        ]);
        let result = mapping_finalized(Box::pin(stream), &mut audit).await;
        assert!(result.is_err());
        audit.finish(&result.map(|_| ()));
        drop(audit);
        let records = directory.records();
        let last = records.last().unwrap();
        assert_eq!(last["phase"], "failed");
        assert_eq!(last["at"], "waiting_finality");
        assert_eq!(last["lastStatus"], expected);
        assert!(last["details"]["reason"]
            .as_str()
            .unwrap()
            .contains(&expected));
        assert!(!records
            .iter()
            .any(|r| r["phase"] == "cancelled" || r["phase"] == "complete"));
    }
    for events in [vec![], vec![TransactionStatus::Retracted(hash)]] {
        let directory = MappingAuditDirectory::new();
        let mut audit = MappingAudit::new(directory.0.clone());
        let result = mapping_finalized(Box::pin(futures::stream::iter(events)), &mut audit).await;
        assert!(result.is_err());
        audit.finish(&result.map(|_| ()));
        assert_eq!(
            directory.records().last().unwrap()["details"]["reason"],
            "维护备注状态流结束，结果不明，停止且不重发"
        );
    }
    // 尚未最终确定的流保持等待，不能签发下一笔备注。
    let pending_directory = MappingAuditDirectory::new();
    let base = pending_directory.0.clone();
    let pending = async move {
        let mut audit = MappingAudit::new(base);
        audit.step("waiting_finality", serde_json::json!({}));
        mapping_finalized(Box::pin(futures::stream::pending()), &mut audit).await
    };
    assert!(tokio::time::timeout(Duration::from_millis(1), pending,)
        .await
        .is_err());
    let records = pending_directory.records();
    assert_eq!(records.last().unwrap()["phase"], "cancelled");
    assert_eq!(records.last().unwrap()["at"], "waiting_finality");
    assert!(!records
        .iter()
        .any(|r| r["phase"] == "failed" || r["phase"] == "complete"));
}

#[test]
fn mapping_backfill_audit_keeps_wait_conditions_and_cancellation_distinct() {
    let directory = MappingAuditDirectory::new();
    {
        let mut audit = MappingAudit::new(directory.0.clone());
        let snapshot = serde_json::json!({
            "syncing": false, "best": runtime::Hash::repeat_byte(1),
            "finalized": runtime::Hash::repeat_byte(2), "bestHeight": 127,
            "finalizedHeight": 127, "ready": 0, "future": 1,
        });
        assert!(!mapping_idle(false, false, 0, 1));
        audit.wait(snapshot.clone());
        audit.wait(snapshot.clone());
        let records = directory.records();
        let waits = records
            .iter()
            .filter(|r| r["phase"] == "waiting_idle")
            .collect::<Vec<_>>();
        assert_eq!(waits.len(), 1, "不变等待条件不能刷屏或产生停滞错误");
        assert_eq!(waits[0]["details"], snapshot);
        assert_ne!(
            waits[0]["details"]["best"],
            waits[0]["details"]["finalized"]
        );
        assert_eq!(waits[0]["details"]["future"], 1);
        assert!(!records
            .iter()
            .any(|r| r["phase"] == "failed" || r["phase"] == "complete"));
        audit.last_wait = None;
        audit.wait(snapshot);
        assert_eq!(
            directory
                .records()
                .iter()
                .filter(|r| r["phase"] == "waiting_idle")
                .count(),
            2
        );
    }
    let records = directory.records();
    assert_eq!(records.last().unwrap()["phase"], "cancelled");
    assert_eq!(records.last().unwrap()["at"], "waiting_idle");
}

#[test]
fn mapping_backfill_audit_distinguishes_panic_from_cancellation() {
    let directory = MappingAuditDirectory::new();
    let result = std::panic::catch_unwind(|| {
        let mut audit = MappingAudit::new(directory.0.clone());
        audit.step("checking_result", serde_json::json!({}));
        panic!("合成维护任务panic");
    });
    assert!(result.is_err());
    let records = directory.records();
    assert_eq!(records.last().unwrap()["phase"], "panicked");
    assert_eq!(records.last().unwrap()["at"], "checking_result");
    assert!(!records.iter().any(|r| r["phase"] == "cancelled"));
}

#[test]
fn mapping_backfill_audit_records_completion_and_multiline_failure() {
    for result in [Ok(()), Err("固定错误\n第二行\t字段".to_owned())] {
        let directory = MappingAuditDirectory::new();
        let mut audit = MappingAudit::new(directory.0.clone());
        audit.step(
            "checking_progress",
            serde_json::json!({"complete": result.is_ok()}),
        );
        audit.finish(&result);
        drop(audit);
        let records = directory.records();
        let last = records.last().unwrap();
        assert_eq!(last["at"], "checking_progress");
        match result {
            Ok(()) => assert_eq!(last["phase"], "complete"),
            Err(error) => {
                assert_eq!(last["phase"], "failed");
                assert_eq!(last["details"]["reason"], error);
            }
        }
        assert_eq!(records.len(), 3, "换行和制表符不能注入其它审计记录");
        assert!(!records.iter().any(|r| r["phase"] == "cancelled"));
    }
}

#[tokio::test]
async fn mapping_backfill_audit_write_failure_preserves_original_result() {
    use sc_transaction_pool_api::TransactionStatus;
    let directory = MappingAuditDirectory::new();
    let base = directory.0.join("missing");
    let mut audit = MappingAudit::new(base.clone());
    audit.step("waiting_finality", serde_json::json!({}));
    let hash = runtime::Hash::repeat_byte(9);
    let status = futures::stream::iter([TransactionStatus::Finalized((hash, 1))]);
    assert_eq!(
        mapping_finalized(Box::pin(status), &mut audit)
            .await
            .unwrap(),
        (hash, 1)
    );
    let result = mapping_finalized(
        Box::pin(futures::stream::iter([TransactionStatus::Invalid])),
        &mut audit,
    )
    .await;
    assert_eq!(result.as_ref().unwrap_err(), "维护备注未最终确认: Invalid");
    audit.finish(&result.map(|_| ()));
    assert!(!base.exists(), "审计接口不能创建节点数据目录");
}

#[test]
fn archive_service_rejects_pruning_before_opening_database() {
    use sc_cli::CliConfiguration;
    let cli = crate::cli::Cli::parse_archive_from(["node", "--tmp"]).unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut config = cli
        .run
        .create_configuration(&cli, runtime.handle().clone())
        .unwrap();
    assert!(crate::service::check_archive(&config).is_ok());
    config.state_pruning = Some(sc_service::PruningMode::blocks_pruning(256));
    assert!(crate::service::check_archive(&config).is_err());
    config.state_pruning = Some(sc_service::PruningMode::ArchiveAll);
    config.blocks_pruning = sc_service::BlocksPruning::KeepFinalized;
    assert!(crate::service::check_archive(&config).is_err());
}

#[test]
fn archive_frozen_genesis_is_verified_once_without_chain_database() {
    let first =
        crate::chain::guard_genesis().unwrap_or_else(|error| panic!("冻结输入校验失败:{error}"));
    let second = crate::chain::guard_genesis().unwrap();
    assert!(
        std::sync::Arc::ptr_eq(&first, &second),
        "网络和挖矿必须共享同一只读输入"
    );
}

#[cfg(feature = "runtime-tests")]
#[test]
fn guard_startup_failure_keeps_service_rpc_alive_and_imports_closed() {
    use sc_client_api::StorageProvider;
    use sc_consensus::{BlockImport, ImportResult};
    use sc_rpc_api::chain::ChainApiServer;
    use sp_blockchain::HeaderBackend;
    use std::sync::Arc;
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut config = super::p2p_bad_block_tests::test_config(
        "guard-startup-diagnostics",
        runtime.handle().clone(),
    );
    let root = config.base_path.path().to_path_buf();
    // 只在隔离声明的测试创世中删除宪法记录，验证原先导致整项启动失败的实际分支。
    let mut storage = config
        .chain_spec
        .as_storage_builder()
        .build_storage()
        .unwrap();
    storage
        .top
        .remove(&nodeguard::constitution::storage_key::law(0));
    config.chain_spec.set_storage(storage);
    let partial = {
        let _entered = runtime.enter();
        crate::service::testing::new_partial_for_test(&config).unwrap()
    };
    let genesis = partial.client.info().genesis_hash;
    assert!(partial
        .client
        .storage(
            genesis,
            &sp_storage::StorageKey(nodeguard::constitution::storage_key::law(0))
        )
        .unwrap()
        .is_none());
    let spawn: Arc<dyn sp_core::traits::SpawnNamed> = Arc::new(partial.task_manager.spawn_handle());
    let rpc = sc_rpc::chain::new_full(partial.client.clone(), spawn).into_rpc();
    let header: serde_json::Value = runtime
        .block_on(rpc.call("chain_getHeader", jsonrpsee::rpc_params![genesis]))
        .unwrap();
    assert_eq!(header["number"], "0x0");
    let identity =
        nodeguard::guard::testing::TestGenesisIdentity::from_chain_spec(config.chain_spec.as_ref())
            .unwrap();
    let inner = super::p2p_bad_block_tests::CountingImport::default();
    let imports = inner.imports.clone();
    let guard = nodeguard::guard::NodeGuard::new_for_test(
        inner,
        partial.client.clone(),
        partial.backend.clone(),
        identity,
        Err("固定基准读取故障".into()),
    );
    let params = super::p2p_bad_block_tests::legal_remark_block_params(&partial.client);
    assert_eq!(
        runtime.block_on(guard.import_block(params)).unwrap(),
        ImportResult::KnownBad
    );
    assert_eq!(imports.load(std::sync::atomic::Ordering::SeqCst), 0);
    drop(guard);
    let inner = super::p2p_bad_block_tests::CountingImport::default();
    let imports = inner.imports.clone();
    assert_ne!(
        partial.client.info().genesis_hash,
        sp_core::H256::from(primitives::genesis::GENESIS_HASH)
    );
    // 直接覆盖旧生产构造器的身份硬拒绝分支：输入是真正冻结基准，客户端属于隔离测试链。
    let guard = nodeguard::guard::NodeGuard::new(
        inner,
        partial.client.clone(),
        partial.backend.clone(),
        Ok(crate::chain::guard_genesis().expect("冻结基准必须真实有效")),
    );
    let params = super::p2p_bad_block_tests::legal_remark_block_params(&partial.client);
    assert_eq!(
        runtime.block_on(guard.import_block(params)).unwrap(),
        ImportResult::KnownBad
    );
    assert_eq!(imports.load(std::sync::atomic::Ordering::SeqCst), 0);
    drop(guard);

    let inner = super::p2p_bad_block_tests::CountingImport::default();
    let imports = inner.imports.clone();
    let guard = nodeguard::constitution::guard::ConstitutionGuard::new(
        inner,
        partial.client.clone(),
        partial.backend.clone(),
        nodeguard::guard::testing::TestGenesisIdentity::genesis_state(config.chain_spec.as_ref()),
    );
    let params = super::p2p_bad_block_tests::legal_remark_block_params(&partial.client);
    assert_eq!(
        runtime.block_on(guard.import_block(params)).unwrap(),
        ImportResult::KnownBad
    );
    assert_eq!(imports.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(partial.client.info().best_number, 0);
    let header: serde_json::Value = runtime
        .block_on(rpc.call("chain_getHeader", jsonrpsee::rpc_params![genesis]))
        .unwrap();
    assert_eq!(header["number"], "0x0", "连续拒绝后RPC仍须响应");
    drop((rpc, guard));
    crate::service::testing::stop_partial(partial, &runtime);
    drop(config);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn archive_old_constrained_database_is_not_silently_relabelled() {
    use sc_cli::CliConfiguration;
    use std::sync::Arc;
    let rt = tokio::runtime::Runtime::new().unwrap();
    let cli = crate::cli::Cli::parse_archive_from(["node", "--tmp"]).unwrap();
    let mut config = cli
        .run
        .create_configuration(&cli, rt.handle().clone())
        .unwrap();
    // 仅底层隔离夹具建立已有块0的旧式库，产品启动入口始终固定归档。
    config.state_pruning = Some(sc_service::PruningMode::blocks_pruning(256));
    let executor = sc_service::new_wasm_executor::<sp_io::SubstrateHostFunctions>(&config.executor);
    let (client, backend, keys, manager) = {
        let _enter = rt.enter();
        sc_service::new_full_parts::<runtime::opaque::Block, runtime::apis::RuntimeApi, _>(
            &config,
            None,
            executor,
            Vec::new(),
        )
        .unwrap()
    };
    let released = Arc::downgrade(&backend);
    let tasks = manager.into_task_registry();
    drop((client, backend, keys));
    rt.block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            while released.upgrade().is_some() || !tasks.running_tasks().is_empty() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    });
    config.state_pruning = Some(sc_service::PruningMode::ArchiveAll);
    let error = sc_service::new_db_backend::<runtime::opaque::Block>(config.db_config())
        .err()
        .expect("旧式模式必须被明确拒绝");
    assert!(error.to_string().contains("pruning"), "{error}");
    let mut old = config.db_config();
    old.state_pruning = Some(sc_service::PruningMode::blocks_pruning(256));
    drop(sc_service::new_db_backend::<runtime::opaque::Block>(old).unwrap());
    assert!(
        sc_service::new_db_backend::<runtime::opaque::Block>(config.db_config()).is_err(),
        "不得偷偷改写旧库模式"
    );
}

#[test]
fn ethereum_index_work_and_failure_do_not_block_the_single_native_worker() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let mut manager = sc_service::TaskManager::new(runtime.handle().clone(), None).unwrap();
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let (finished, done) = tokio::sync::oneshot::channel();
    spawn_ethereum_index(&manager, async move {
        started.send(()).unwrap();
        wait.recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        finished.send(()).unwrap();
        Err(rpc_stage_error(
            "固定初始化故障",
            std::io::Error::other("固定收据失败"),
        ))
    });
    runtime.block_on(async {
        ready.await.unwrap();
        let before = std::time::Instant::now();
        let native = tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            7
        });
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(1), native)
                .await
                .unwrap()
                .unwrap(),
            7
        );
        assert!(
            before.elapsed() < std::time::Duration::from_secs(1),
            "Ethereum工作不能占用唯一原生工作线程"
        );
        release.send(()).unwrap();
        done.await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), manager.future())
                .await
                .is_err(),
            "Ethereum失败不能结束节点任务管理器"
        );
    });
    drop(manager.into_task_registry());
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
}
