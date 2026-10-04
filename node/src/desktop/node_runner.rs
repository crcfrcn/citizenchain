//! 进程内 Substrate 节点管理。
//!
//! 替代旧的子进程模式。在 Tauri 进程内直接启动 Substrate 节点服务。
//!
//! 关键约束：drop `NodeHandle` 必须真正终结后台 substrate 线程并释放
//! Backend（含 RocksDB LOCK）。否则同进程内"停 → 启"会撞
//! `lock hold by current process` 错误。
//! 实现办法：握有 shutdown oneshot + JoinHandle，Drop 时先发信号再 join。

use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::sync::oneshot;

const NODE_RUNTIME_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);
const NODE_SERVICE_START_TIMEOUT: Duration = Duration::from_secs(15 * 60);

struct NodeThreadAliveGuard(Arc<AtomicBool>);

impl Drop for NodeThreadAliveGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// 节点运行句柄。drop 时会通知后台线程退出并 join。
pub struct NodeHandle {
    shutdown_tx: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
    alive: Arc<AtomicBool>,
    exit_error: Arc<Mutex<Option<String>>>,
}

impl NodeHandle {
    /// 返回后台 Substrate 线程是否仍存活，用于把异常退出同步到首页状态。
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    /// 取出后台必要任务退出时保留的真实原因。
    pub fn take_exit_error(&self) -> Option<String> {
        self.exit_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }
}

impl Drop for NodeHandle {
    fn drop(&mut self) {
        // 发送停机信号；接收端可能已被 task_manager 退出路径丢弃，忽略错误。
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        // join 线程，等待 task_manager + tokio runtime 完整 drop，
        // 确保 Substrate Backend 释放 RocksDB LOCK 后再返回。
        if let Some(handle) = self.thread.take() {
            if let Err(err) = handle.join() {
                eprintln!("[节点] Substrate 后台线程退出异常: {err:?}");
            }
        }
        self.alive.store(false, Ordering::Release);
    }
}

/// 在进程内启动 Substrate 节点。
pub fn start_node_in_process(
    base_path: PathBuf,
    chain_spec: Option<String>,
    rpc_port: u16,
    node_name: Option<String>,
    validator: bool,
    mining_threads: usize,
    gpu_device: Option<usize>,
) -> Result<NodeHandle, String> {
    let mut args: Vec<String> = vec![
        "node".into(),
        "--base-path".into(),
        base_path.display().to_string(),
        "--chain".into(),
        chain_spec.unwrap_or_else(|| "citizenchain".into()),
        "--listen-addr".into(),
        "/ip4/0.0.0.0/tcp/30333/wss".into(),
        "--listen-addr".into(),
        "/ip6/::/tcp/30333/wss".into(),
        "--rpc-port".into(),
        rpc_port.to_string(),
        "--rpc-methods".into(),
        "Unsafe".into(),
        "--rpc-cors".into(),
        "all".into(),
        "--no-prometheus".into(),
    ];

    let node_key_file = base_path.join("node-key").join("secret_ed25519");
    if node_key_file.is_file() {
        args.push("--node-key-file".into());
        args.push(node_key_file.display().to_string());
    }

    if let Some(name) = node_name {
        args.push("--name".into());
        args.push(name);
    }

    if validator {
        args.push("--validator".into());
    }

    let (startup_tx, startup_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let exit_error = Arc::new(Mutex::new(None));
    let thread_exit_error = Arc::clone(&exit_error);
    let task_exit_error = Arc::clone(&exit_error);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let alive = Arc::new(AtomicBool::new(true));
    let thread_alive = Arc::clone(&alive);

    let thread = std::thread::Builder::new()
        .name("substrate-node".into())
        .spawn(move || {
            // 存活标记必须在线程最终退出时才清除，确保 panic 原因先写入通道。
            let _alive_guard = NodeThreadAliveGuard(thread_alive);
            let thread_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                use clap::Parser;
                use sc_cli::CliConfiguration;

                // 设置 SS58 地址前缀。
                sp_core::crypto::set_default_ss58_version(
                    sp_core::crypto::Ss58AddressFormat::custom(primitives::core_const::SS58_FORMAT),
                );

                let mut cli = match crate::core::cli::Cli::try_parse_from(&args) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = startup_tx.send(Err(format!("解析节点参数失败: {e}")));
                        return;
                    }
                };

                // 桌面路径不经过 core::command::run，必须在这里采用同一交易池基线。
                // fork-aware 后台任务会在本链普通启动时提前结束并关闭整个服务。
                cli.run.pool_config.pool_type = sc_cli::TransactionPoolType::SingleState;

                // 构建 tokio runtime（不用 create_runner，因为它会初始化日志导致冲突）。
                let tokio_runtime = match tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = startup_tx.send(Err(format!("创建 tokio runtime 失败: {e}")));
                        return;
                    }
                };

                // 直接构造 Configuration，跳过日志初始化。
                let config = match cli
                    .run
                    .create_configuration(&cli, tokio_runtime.handle().clone())
                {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = startup_tx.send(Err(format!("创建配置失败: {e}")));
                        return;
                    }
                };

                // 在 tokio runtime 中启动节点服务。
                // UI 启动路径暂不支持清算行角色,
                // 全部透传 None(bank CID / role / password / reserve interval);生产用户
                // 通过 CLI 的 `--clearing-bank` 进入无 UI 模式启动清算行节点。
                tokio_runtime.block_on(async {
                    match crate::core::service::new_full(
                        config,
                        mining_threads,
                        gpu_device,
                        None,
                        None,
                        None,
                        None,
                    ) {
                        Ok(mut task_manager) => {
                            // 明确通知调用线程服务已构建完成，禁止用固定 sleep 猜测启动结果。
                            let _ = startup_tx.send(Ok(()));
                            // 退出条件二选一：essential task 失败 / 收到外部 shutdown 信号。
                            tokio::select! {
                            result = task_manager.future() => {
                                *task_exit_error
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                                    Some(format!("节点必要后台任务退出: {result:?}"));
                                },
                            result = shutdown_rx => {
                                *task_exit_error
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(
                                    format!("节点后台线程收到停机信号: {result:?}")
                                );
                            },
                            }
                            // 显式 drop task_manager 触发 Backend 释放（含 RocksDB LOCK）。
                            // 必须发生在 tokio_runtime 仍存活时，否则 drop 内的异步 cleanup 无法执行。
                            drop(task_manager);
                        }
                        Err(e) => {
                            let _ = startup_tx.send(Err(format!("节点启动失败: {e}")));
                        }
                    }
                });
                // 等待 tokio runtime 内残余任务退出（包括 Backend flush）。
                tokio_runtime.shutdown_timeout(NODE_RUNTIME_SHUTDOWN_TIMEOUT);
            }));
            if let Err(payload) = thread_result {
                let detail = payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| {
                        payload
                            .downcast_ref::<&str>()
                            .map(|value| (*value).to_string())
                    })
                    .unwrap_or_else(|| "未知 panic payload".to_string());
                *thread_exit_error
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                    Some(format!("节点后台线程 panic: {detail}"));
            }
        })
        .map_err(|e| format!("启动节点线程失败: {e}"))?;

    // 等待节点服务明确完成构建。旧实现固定等待 5 秒，会丢失耗时更长的真实错误。
    match startup_rx.recv_timeout(NODE_SERVICE_START_TIMEOUT) {
        Ok(Err(err)) => {
            let _ = thread.join();
            Err(err)
        }
        Ok(Ok(())) => Ok(NodeHandle {
            shutdown_tx: Some(shutdown_tx),
            thread: Some(thread),
            alive,
            exit_error,
        }),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            let _ = shutdown_tx.send(());
            let _ = thread.join();
            Err(format!(
                "节点服务在 {} 秒内未完成构建",
                NODE_SERVICE_START_TIMEOUT.as_secs()
            ))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            let _ = thread.join();
            Err("节点服务构建线程提前断开，且未返回结果".to_string())
        }
    }
}
