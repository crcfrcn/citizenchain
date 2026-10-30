use nodeclient::handle::NodeHandle;
use nodeclient::network::LocalNetworkReader;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;

use std::time::Duration;
use tokio::sync::oneshot;

const NODE_RUNTIME_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);

const NODE_SERVICE_START_TIMEOUT: Duration = Duration::from_secs(15 * 60);

struct NodeThreadAliveGuard(Arc<AtomicBool>, LocalNetworkReader);

fn startup_disconnect_error(exit_error: &Arc<Mutex<Option<String>>>) -> String {
    exit_error
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
        .unwrap_or_else(|| "节点服务构建线程提前断开，且未返回结果".to_string())
}

fn essential_failure_message(failure: &Arc<Mutex<Option<String>>>, fallback: &str) -> String {
    failure
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
        .unwrap_or_else(|| fallback.to_string())
}

// 首个业务错误是诊断真源，清理故障只能追加，不能覆盖它。
fn remember_first_failure(failure: &Arc<Mutex<Option<String>>>, reason: String) {
    let mut saved = failure
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if saved.is_none() {
        *saved = Some(reason);
    }
}

fn remember_panic(
    failure: &Arc<Mutex<Option<String>>>,
    rpc_failure: &Arc<Mutex<Option<String>>>,
    payload: Box<dyn std::any::Any + Send>,
) {
    let detail = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&str>()
                .map(|value| (*value).to_string())
        })
        .unwrap_or_else(|| "未知 panic payload".to_string());
    if let Some(reason) = rpc_failure
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
    {
        remember_first_failure(failure, format!("节点必要后台任务退出: {reason}"));
    }
    let mut saved = failure
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match saved.as_mut() {
        Some(reason) => reason.push_str(&format!("；节点线程或清理 panic: {detail}")),
        None => *saved = Some(format!("节点后台线程 panic: {detail}")),
    }
}

// TaskManager 持有独立 RPC Runtime；只借用它做异步等待，在 block_on 外销毁。
// 与官方 CLI 一样，先通知并回收服务任务，再关闭仍存活的宿主 Runtime。
fn run_node_tasks(
    runtime: tokio::runtime::Runtime,
    mut task_manager: sc_service::TaskManager,
    shutdown_rx: oneshot::Receiver<()>,
    rpc_failure: &Arc<Mutex<Option<String>>>,
    exit_error: &Arc<Mutex<Option<String>>>,
) {
    let waited = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.block_on(async {
            tokio::select! {
                result = task_manager.future() => format!("节点必要后台任务退出: {}",
                    essential_failure_message(rpc_failure, &format!("{result:?}"))),
                result = shutdown_rx => format!("节点后台线程收到停机信号: {result:?}"),
            }
        })
    }));
    match waited {
        Ok(reason) => remember_first_failure(exit_error, reason),
        Err(payload) => remember_panic(exit_error, rpc_failure, payload),
    }
    let registry = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        task_manager.into_task_registry()
    })) {
        Ok(registry) => Some(registry),
        Err(payload) => {
            remember_panic(exit_error, rpc_failure, payload);
            None
        }
    };
    runtime.shutdown_timeout(NODE_RUNTIME_SHUTDOWN_TIMEOUT);
    if let Some(registry) = registry {
        let remaining = registry.running_tasks();
        if !remaining.is_empty() {
            let mut saved = exit_error
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let reason = saved.get_or_insert_with(|| "节点停机失败".into());
            reason.push_str(&format!(
                "；关闭执行器后仍有 {} 类任务未退出",
                remaining.len()
            ));
        }
    }
}

impl Drop for NodeThreadAliveGuard {
    fn drop(&mut self) {
        self.1.close();
        self.0.store(false, Ordering::Release);
    }
}

/// 在进程内启动 Substrate 节点。
pub(crate) fn start_node_in_process(
    base_path: PathBuf,
    chain_spec: Option<String>,
    rpc_port: u16,
    node_name: Option<String>,
    validator: bool,
    mining_threads: usize,
    gpu_device: Option<usize>,
) -> Result<NodeHandle, String> {
    let rpc_tls = nodedesktop::tls::desktop_rpc_tls()?;
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
    let thread_rpc_task_failure = Arc::new(Mutex::new(None));
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let alive = Arc::new(AtomicBool::new(true));
    let thread_alive = Arc::clone(&alive);
    let network = nodeclient::network::LocalNetworkReader::new();
    let thread_network = network.clone();

    let thread = std::thread::Builder::new()
        .name("substrate-node".into())
        .spawn(move || {
            // 存活标记必须在线程最终退出时才清除，确保 panic 原因先写入通道。
            let _alive_guard = NodeThreadAliveGuard(thread_alive, thread_network.clone());
            let thread_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                use clap::Parser;
                use sc_cli::CliConfiguration;

                // 设置 SS58 地址前缀。
                sp_core::crypto::set_default_ss58_version(
                    sp_core::crypto::Ss58AddressFormat::custom(primitives::core_const::SS58_FORMAT),
                );

                let mut cli = match crate::cli::Cli::try_parse_from(&args) {
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

                // 同步构造只进入 Runtime 句柄上下文，避免失败清理发生在异步作用域。
                // UI 启动路径暂不支持清算行角色,
                // 全部透传 None(bank CID / role / password / reserve interval);生产用户
                // 通过 CLI 的 `--clearing-bank` 进入无 UI 模式启动清算行节点。
                let service = {
                    let _entered = tokio_runtime.enter();
                    crate::service::new_full_with_network_reader(
                        config,
                        mining_threads,
                        gpu_device,
                        Some(rpc_tls),
                        Some(Arc::clone(&thread_rpc_task_failure)),
                        None,
                        None,
                        None,
                        None,
                        Some(thread_network),
                    )
                };
                match service {
                    Ok(task_manager) => {
                        let _ = startup_tx.send(Ok(()));
                        run_node_tasks(
                            tokio_runtime,
                            task_manager,
                            shutdown_rx,
                            &thread_rpc_task_failure,
                            &thread_exit_error,
                        );
                    }
                    Err(error) => {
                        let reason = format!("节点启动失败: {error}");
                        remember_first_failure(&thread_exit_error, reason.clone());
                        let _ = startup_tx.send(Err(reason));
                        tokio_runtime.shutdown_timeout(NODE_RUNTIME_SHUTDOWN_TIMEOUT);
                    }
                }
            }));
            if let Err(payload) = thread_result {
                remember_panic(&thread_exit_error, &thread_rpc_task_failure, payload);
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
            network,
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
            // 线程已保存panic详情；先join再取原因，避免首页只能看到断开通道的包装错误。
            Err(startup_disconnect_error(&exit_error))
        }
    }
}
#[cfg(test)]
#[path = "tests/runner.rs"]
mod tests;
