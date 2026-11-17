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
