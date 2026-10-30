use super::*;

use crate::runner::essential_failure_message;
use crate::runner::remember_panic;
use crate::runner::run_node_tasks;
use crate::runner::startup_disconnect_error;
use crate::runner::NodeThreadAliveGuard;
use nodeclient::handle::NodeHandle;

#[test]
fn stopping_node_revokes_network_reader_before_shutdown_notification() {
    let network = nodeclient::network::LocalNetworkReader::new();
    let observer = network.clone();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let worker = std::thread::spawn(move || {
        shutdown_rx.blocking_recv().unwrap();
        assert!(observer.peers().unwrap_err().contains("已关闭"));
    });
    let alive = Arc::new(AtomicBool::new(true));
    let handle = NodeHandle {
        shutdown_tx: Some(shutdown_tx),
        thread: Some(worker),
        alive: alive.clone(),
        exit_error: Arc::new(Mutex::new(None)),
        network: network.clone(),
    };
    drop(handle);
    assert!(!alive.load(Ordering::Acquire));
    assert!(network.peers().unwrap_err().contains("已关闭"));
}

#[test]
fn unexpected_thread_exit_also_revokes_network_reader() {
    let alive = Arc::new(AtomicBool::new(true));
    let network = nodeclient::network::LocalNetworkReader::new();
    drop(NodeThreadAliveGuard(alive.clone(), network.clone()));
    assert!(!alive.load(Ordering::Acquire));
    assert!(network.peers().unwrap_err().contains("已关闭"));
}

#[test]
fn startup_disconnect_reports_saved_thread_failure() {
    let reason = Arc::new(Mutex::new(Some("节点后台线程 panic: 测试故障".to_string())));
    assert_eq!(
        startup_disconnect_error(&reason),
        "节点后台线程 panic: 测试故障"
    );
    assert_eq!(
        startup_disconnect_error(&reason),
        "节点服务构建线程提前断开，且未返回结果"
    );
}

#[test]
fn essential_task_failure_reports_rpc_reason_when_available() {
    let reason = Arc::new(Mutex::new(Some("TLS 监听失败".to_string())));
    assert_eq!(
        essential_failure_message(&reason, "Essential task failed"),
        "TLS 监听失败"
    );
    assert_eq!(
        essential_failure_message(&reason, "Essential task failed"),
        "Essential task failed"
    );
}

fn lifecycle_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn task_manager_runtime_shutdown_releases_database_before_reopen() {
    use clap::Parser;
    use sc_cli::CliConfiguration;
    let runtime = lifecycle_runtime();
    // 只取普通绝对路径，不把canonicalize产生的Windows扩展路径交给RocksDB。
    let work = std::path::absolute(
        std::env::var_os("CITIZENCHAIN_WORK_DIR").expect("生命周期测试必须归本轮产品现场"),
    )
    .unwrap();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = work.join(format!("node-lifecycle-db-{}-{unique}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    let cli = crate::cli::Cli::try_parse_from([
        "node",
        "--base-path",
        path.to_str().unwrap(),
        "--chain",
        "citizenchain",
        "--database",
        "rocksdb",
        "--no-prometheus",
    ])
    .unwrap();
    let config = cli
        .run
        .create_configuration(&cli, runtime.handle().clone())
        .unwrap();
    let backend = sc_service::new_db_backend::<runtime::opaque::Block>(config.db_config()).unwrap();
    let weak = Arc::downgrade(&backend);
    let mut manager = sc_service::TaskManager::new(runtime.handle().clone(), None).unwrap();
    manager.keep_alive(backend);
    // 真实嵌套Runtime会在旧的异步drop路径panic，不能用计数器替代该回归。
    manager.keep_alive(lifecycle_runtime());
    let (stop, stopped) = oneshot::channel();
    stop.send(()).unwrap();
    let rpc_failure = Arc::new(Mutex::new(None));
    let exit_error = Arc::new(Mutex::new(None));
    run_node_tasks(runtime, manager, stopped, &rpc_failure, &exit_error);
    let reason = exit_error.lock().unwrap().clone().unwrap();
    assert!(reason.starts_with("节点后台线程收到停机信号:"));
    assert!(!reason.contains("panic"), "{reason}");
    assert!(weak.upgrade().is_none(), "Backend必须在停机返回前释放");
    // 同进程直接重新打开真实RocksDB，验证实际LOCK释放而非只读生命周期标记。
    drop(sc_service::new_db_backend::<runtime::opaque::Block>(config.db_config()).unwrap());
    drop(config);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn essential_failure_keeps_original_reason_after_runtime_shutdown() {
    let runtime = lifecycle_runtime();
    let mut manager = sc_service::TaskManager::new(runtime.handle().clone(), None).unwrap();
    manager.keep_alive(lifecycle_runtime());
    let original = "Ethereum RPC/TLS 必要任务失败: Ethereum客户端初始化: 固定原始错误(4003)";
    let rpc_failure = Arc::new(Mutex::new(Some(original.to_string())));
    manager
        .spawn_essential_handle()
        .spawn("lifecycle-failure", None, async {});
    let (_stop, stopped) = oneshot::channel();
    let exit_error = Arc::new(Mutex::new(None));
    run_node_tasks(runtime, manager, stopped, &rpc_failure, &exit_error);
    assert_eq!(
        exit_error.lock().unwrap().as_deref(),
        Some(format!("节点必要后台任务退出: {original}").as_str())
    );
}

#[test]
fn cleanup_panic_is_appended_without_overwriting_first_error() {
    struct FailingCleanup;
    impl Drop for FailingCleanup {
        fn drop(&mut self) {
            panic!("固定清理故障");
        }
    }
    let runtime = lifecycle_runtime();
    let mut manager = sc_service::TaskManager::new(runtime.handle().clone(), None).unwrap();
    manager.keep_alive(lifecycle_runtime());
    manager.keep_alive(FailingCleanup);
    let rpc_failure = Arc::new(Mutex::new(Some("固定原始错误".to_string())));
    manager
        .spawn_essential_handle()
        .spawn("lifecycle-cleanup-failure", None, async {});
    let (_stop, stopped) = oneshot::channel();
    let exit_error = Arc::new(Mutex::new(None));
    run_node_tasks(runtime, manager, stopped, &rpc_failure, &exit_error);
    let reason = exit_error.lock().unwrap().clone().unwrap();
    assert!(
        reason.starts_with("节点必要后台任务退出: 固定原始错误"),
        "{reason}"
    );
    assert!(reason.contains("固定清理故障"), "{reason}");
    let missing = Arc::new(Mutex::new(None));
    remember_panic(&missing, &rpc_failure, Box::new("独立panic"));
    assert_eq!(
        missing.lock().unwrap().as_deref(),
        Some("节点后台线程 panic: 独立panic")
    );
}
