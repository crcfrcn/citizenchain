use crate::initialization::install_genesis_state_if_available;
use crate::initialization::needs_local_chain_initialization;
use nodeclient::handle::NodeHandle;
use nodecontrol::identity::current_status;
use nodecontrol::identity::NodeStatus;
use nodecontrol::state::lock_node_lifecycle;
use nodecontrol::state::node_data_dir;
use nodecontrol::state::set_runtime_state;
use nodecontrol::state::AppState;
use nodecontrol::state::NodeLifecycleState;

use std::path::PathBuf;

use std::thread;
use std::time::Duration;
use tauri::AppHandle;
use tauri::Manager;

const NODE_RESTART_SETTLE_DELAY: Duration = Duration::from_secs(2);

const NODE_LOCK_RETRY_DELAY: Duration = Duration::from_secs(5);

const NODE_LOCK_RETRY_LIMIT: usize = 2;

const NODE_RPC_READY_TIMEOUT: Duration = Duration::from_secs(15 * 60);

fn is_database_lock_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("lock hold by current process")
        || lower.contains("no locks available")
        || (lower.contains("/db/full/lock") && lower.contains("lock"))
}

fn state_for_start_error(error: &str) -> NodeLifecycleState {
    if is_database_lock_error(error) {
        nodecontrol::state::NodeLifecycleState::LockHeld
    } else {
        nodecontrol::state::NodeLifecycleState::Failed
    }
}

fn start_error_for_user(error: &str) -> String {
    let raw = error.trim();
    if is_database_lock_error(error) {
        return format!(
            "节点启动失败：数据库锁仍被当前进程占用，请完全退出软件后重新打开。原始错误: {raw}"
        );
    }
    if raw.starts_with("节点启动失败") {
        raw.to_string()
    } else {
        format!("节点启动失败: {raw}")
    }
}

fn start_node_in_process_with_retry(
    base_path: PathBuf,
    chain_spec: Option<String>,
    rpc_port: u16,
    enable_grandpa_validator: bool,
    mining_threads: usize,
) -> Result<NodeHandle, String> {
    for attempt in 0..=NODE_LOCK_RETRY_LIMIT {
        match crate::runner::start_node_in_process(
            base_path.clone(),
            chain_spec.clone(),
            rpc_port,
            None, // 节点名称已移除，不再使用
            enable_grandpa_validator,
            mining_threads,
            None, // gpu_device
        ) {
            Ok(handle) => return Ok(handle),
            Err(err) if is_database_lock_error(&err) && attempt < NODE_LOCK_RETRY_LIMIT => {
                // RocksDB 同进程锁释放偶发滞后，启动失败时只做有限重试，
                // 失败后把状态标记为 lock_held，避免前端继续误认为是普通停止。
                thread::sleep(NODE_LOCK_RETRY_DELAY);
            }
            Err(err) => return Err(start_error_for_user(&err)),
        }
    }
    Err("节点启动失败: database lock retry exhausted".to_string())
}

/// 启动时清理（进程内模式下只需清理 RPC 缓存）。
pub(crate) fn cleanup_on_startup(app: &AppHandle) {
    let _ = app; // 不再需要杀孤儿进程或清理临时文件
    nodedesktop::rpc::clear_genesis_hash_cache();
}

/// 退出时清理：停止进程内节点。
/// 句柄 drop 内部会发送 shutdown 信号 + join 后台线程，
/// 并等待后台线程退出；若 RocksDB 同进程锁释放滞后，启动路径会转入 lock_held。
pub(crate) fn cleanup_on_exit(app: &AppHandle) {
    // 先停同步守护，避免退出时守护线程同时触发节点重启。
    nodecontrol::sync::stop_sync_guard();
    let _lifecycle_guard = lock_node_lifecycle();
    // 先把 handle 取出再 drop，避免在持有 state 锁期间 join 线程。
    let old_handle = match app.state::<AppState>().0.lock() {
        Ok(mut state) => {
            state.node_state = nodecontrol::state::NodeLifecycleState::Stopping;
            state.last_error = None;
            state.node_handle.take()
        }
        Err(_) => None,
    };
    drop(old_handle);
    let _ = set_runtime_state(app, nodecontrol::state::NodeLifecycleState::Stopped, None);
}

fn start_node_with_policy(app: AppHandle, restart_existing: bool) -> Result<NodeStatus, String> {
    let _lifecycle_guard = lock_node_lifecycle();
    if let Err(e) = nodedesktop::security::append_audit_log(&app, "start_node", "attempt") {
        eprintln!("[审计] start_node attempt 日志写入失败: {e}");
    }
    let result = (|| -> Result<NodeStatus, String> {
        if !restart_existing {
            let already_running = {
                let app_state = app.state::<AppState>();
                let mut state = app_state
                    .0
                    .lock()
                    .map_err(|_| "acquire process state failed".to_string())?;
                let running = state
                    .node_handle
                    .as_ref()
                    .map(nodeclient::handle::NodeHandle::is_alive)
                    .unwrap_or(false);
                if running && state.node_state == nodecontrol::state::NodeLifecycleState::Running {
                    state.last_error = None;
                }
                running
            };
            if already_running {
                return current_status(&app);
            }
        }

        // 三平台桌面节点在创建服务前准备一次会话证书；失败保留首页错误状态。
        nodedesktop::tls::prepare_desktop_rpc_tls()?;

        // 停止已有节点（如果在运行）。
        // 取出后立即释放 state 锁再 drop，避免 drop（join 线程）期间阻塞 get_node_status 等查询。
        let old_handle = {
            let app_state = app.state::<AppState>();
            let mut state = app_state
                .0
                .lock()
                .map_err(|_| "acquire process state failed".to_string())?;
            state.node_state = if state.node_handle.is_some() {
                if restart_existing {
                    nodecontrol::state::NodeLifecycleState::Restarting
                } else {
                    nodecontrol::state::NodeLifecycleState::Stopping
                }
            } else if restart_existing {
                nodecontrol::state::NodeLifecycleState::Restarting
            } else {
                nodecontrol::state::NodeLifecycleState::Starting
            };
            state.last_error = None;
            state.node_handle.take()
        };
        drop(old_handle);

        nodedesktop::rpc::clear_genesis_hash_cache();
        thread::sleep(NODE_RESTART_SETTLE_DELAY);
        set_runtime_state(
            &app,
            if restart_existing {
                nodecontrol::state::NodeLifecycleState::Restarting
            } else {
                nodecontrol::state::NodeLifecycleState::Starting
            },
            None,
        )?;

        // UI 模式下 `node_runner.rs` 传 `None,None,None` 到 `new_full`,不启动清算行
        // 组件,启动路径仅承担 PoW + GRANDPA 基础职能;生产清算行节点通过 CLI 的
        // `--clearing-bank` 启动。

        // 准备启动参数。
        let base_path = node_data_dir(&app)?;
        let initializing = needs_local_chain_initialization(&base_path)?;
        if initializing {
            set_runtime_state(
                &app,
                nodecontrol::state::NodeLifecycleState::Initializing,
                None,
            )?;
        }
        install_genesis_state_if_available(&app, &base_path)?;
        // clean-run 本机重新创世时注入 fresh plain chainspec；普通启动仍用冻结主网 spec。
        let chain_spec = std::env::var("CITIZENCHAIN_CHAIN_SPEC")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let rpc_port = nodedesktop::rpc::current_rpc_port();
        let enable_grandpa_validator = nodecontrol::grandpa::prepare_grandpa_for_start(&app)?;
        let mining_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);

        // 在进程内启动 Substrate 节点。
        let handle = start_node_in_process_with_retry(
            base_path,
            chain_spec,
            rpc_port,
            enable_grandpa_validator,
            mining_threads,
        )?;

        // 存储句柄。首次准备本地数据保留“初始化中”，已有数据库保持“启动中”。
        {
            let app_state = app.state::<AppState>();
            let mut state = app_state
                .0
                .lock()
                .map_err(|_| "acquire process state failed".to_string())?;
            state.node_handle = Some(handle);
            state.node_state = if initializing {
                nodecontrol::state::NodeLifecycleState::Initializing
            } else if restart_existing {
                nodecontrol::state::NodeLifecycleState::Restarting
            } else {
                nodecontrol::state::NodeLifecycleState::Starting
            };
            state.last_error = None;
        }

        // 等待 RPC 就绪:只有能读到 chain_getBlockHash(0),首页和 OnChina 才能认为链可用。
        nodedesktop::rpc::wait_for_local_rpc_ready(NODE_RPC_READY_TIMEOUT, || {
            let app_state = app.state::<AppState>();
            let state = app_state
                .0
                .lock()
                .map_err(|_| "acquire process state failed".to_string())?;
            let Some(handle) = state.node_handle.as_ref() else {
                return Err(state
                    .last_error
                    .clone()
                    .unwrap_or_else(|| "节点句柄在 RPC 就绪前丢失，且未保留退出详情".to_string()));
            };
            if handle.is_alive() {
                Ok(())
            } else {
                Err(handle
                    .take_exit_error()
                    .unwrap_or_else(|| "节点线程在 RPC 就绪前退出，但未返回退出详情".to_string()))
            }
        })
        .map_err(|err| {
            let bad_handle = match app.state::<AppState>().0.lock() {
                Ok(mut state) => {
                    state.node_state = nodecontrol::state::NodeLifecycleState::Failed;
                    state.last_error = Some(err.clone());
                    state.node_handle.take()
                }
                Err(_) => None,
            };
            drop(bad_handle);
            format!("节点初始化或启动失败: {err}")
        })?;

        {
            let app_state = app.state::<AppState>();
            let mut state = app_state
                .0
                .lock()
                .map_err(|_| "acquire process state failed".to_string())?;
            state.node_state = nodecontrol::state::NodeLifecycleState::Running;
            state.last_error = None;
        }

        // 验证 GRANDPA 配置。
        if let Err(err) = nodecontrol::grandpa::verify_grandpa_after_start(&app) {
            // 回滚：停止节点。
            let bad_handle = match app.state::<AppState>().0.lock() {
                Ok(mut state) => {
                    state.node_state = nodecontrol::state::NodeLifecycleState::Failed;
                    state.last_error = Some(err.clone());
                    state.node_handle.take()
                }
                Err(_) => None,
            };
            drop(bad_handle);
            return Err(format!("verify grandpa after start failed: {err}"));
        }

        current_status(&app)
    })();
    if let Err(err) = &result {
        let _ = set_runtime_state(&app, state_for_start_error(err), Some(err.clone()));
    }
    if let Err(e) = nodedesktop::security::append_audit_log(
        &app,
        "start_node",
        if result.is_ok() { "success" } else { "failed" },
    ) {
        eprintln!("[审计] start_node 结果日志写入失败: {e}");
    }
    result
}

fn start_node_sync(app: AppHandle) -> Result<NodeStatus, String> {
    start_node_with_policy(app, true)
}

fn start_node_if_stopped_sync(app: AppHandle) -> Result<NodeStatus, String> {
    start_node_with_policy(app, false)
}

fn stop_node_sync(app: AppHandle) -> Result<NodeStatus, String> {
    let _lifecycle_guard = lock_node_lifecycle();
    if let Err(e) = nodedesktop::security::append_audit_log(&app, "stop_node", "attempt") {
        eprintln!("[审计] stop_node attempt 日志写入失败: {e}");
    }
    let result = (|| -> Result<NodeStatus, String> {
        // UI 模式不启动清算行组件(通过 CLI `--clearing-bank` 启动)。
        // CLI 模式下的 graceful shutdown + pending 检查需要把 OffchainComponents 挂到
        // task_manager 的 spawn_essential_handle 生命周期而非全局 static。
        let old_handle = {
            let app_state = app.state::<AppState>();
            let mut state = app_state
                .0
                .lock()
                .map_err(|_| "acquire process state failed".to_string())?;
            state.node_state = nodecontrol::state::NodeLifecycleState::Stopping;
            state.last_error = None;
            state.node_handle.take()
        };
        drop(old_handle);

        nodedesktop::rpc::clear_genesis_hash_cache();
        thread::sleep(NODE_RESTART_SETTLE_DELAY);
        set_runtime_state(&app, nodecontrol::state::NodeLifecycleState::Stopped, None)?;
        current_status(&app)
    })();
    if let Err(err) = &result {
        let _ = set_runtime_state(
            &app,
            nodecontrol::state::NodeLifecycleState::Failed,
            Some(err.clone()),
        );
    }
    if let Err(e) = nodedesktop::security::append_audit_log(
        &app,
        "stop_node",
        if result.is_ok() { "success" } else { "failed" },
    ) {
        eprintln!("[审计] stop_node 结果日志写入失败: {e}");
    }
    result
}

/// 启动节点（同步阻塞调用）。供 `desktop::run_desktop` 的 setup 自动启动以及
/// 设置页 `set_grandpa_key` / `set_bootnode_key` 的"保存即重启"复用。
pub(crate) fn start_node_blocking(app: AppHandle) -> Result<NodeStatus, String> {
    start_node_sync(app)
}

/// 手动启动节点。若自动启动已经完成，则直接返回当前运行状态，避免重复重启。
#[tauri::command]
pub(crate) async fn start_node(app: AppHandle) -> Result<NodeStatus, String> {
    nodecontrol::task::join_blocking_task(
        "start_node",
        tauri::async_runtime::spawn_blocking(move || start_node_if_stopped_sync(app)),
    )
    .await
}

/// 停止节点（同步阻塞调用）。供设置页"保存即重启"流程复用。
pub(crate) fn stop_node_blocking(app: AppHandle) -> Result<NodeStatus, String> {
    stop_node_sync(app)
}

/// 手动停止节点。停止后 App 继续运行，用户可在首页再次手动启动。
#[tauri::command]
pub(crate) async fn stop_node(app: AppHandle) -> Result<NodeStatus, String> {
    nodecontrol::task::join_blocking_task(
        "stop_node",
        tauri::async_runtime::spawn_blocking(move || stop_node_sync(app)),
    )
    .await
}
