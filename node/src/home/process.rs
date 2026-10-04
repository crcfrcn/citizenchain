//! 节点进程管理子模块：在进程内启动/停止 Substrate 节点。
//!
//! 直接在 Tauri 进程内运行 Substrate 节点，不启动外部二进制。
//!
//! 节点生命周期与 App 进程绑定：
//! - App 启动 → `start_node_blocking`（在 `desktop::run_desktop` 的 setup 后台线程中触发）
//! - App 退出 → `cleanup_on_exit`（`RunEvent::Exit` 触发）
//! - 关窗退出 App 并停止节点；macOS 黄色横线只是最小化，不影响节点
//! - 首页暴露 `start_node` / `stop_node` Tauri command，作为手动启停入口。
//! - `start_node_blocking` 仍被设置页 `set_grandpa_key` / `set_bootnode_key` 复用做"保存即重启"。

use crate::desktop::node_runner::{self, NodeHandle};
use crate::{
    settings::grandpa_address,
    shared::{keystore, rpc, security},
};
use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    thread,
    time::Duration,
};
use tauri::{AppHandle, Manager};

use super::identity::{current_status, NodeStatus};

// 串行化节点启停，避免并发命令冲突。
static NODE_LIFECYCLE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
const NODE_RESTART_SETTLE_DELAY: Duration = Duration::from_secs(2);
const NODE_LOCK_RETRY_DELAY: Duration = Duration::from_secs(5);
const NODE_LOCK_RETRY_LIMIT: usize = 2;
const NODE_RPC_READY_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const GENESIS_STATE_ENV: &str = "CITIZENCHAIN_GENESIS_STATE_DIR";
const GENESIS_STATE_RESOURCE_DIR: &str = "genesis-state";
const DEFAULT_CHAIN_ID: &str = "citizenchain";

/// 首页可见的节点生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeLifecycleState {
    Stopped,
    Starting,
    Initializing,
    Running,
    Stopping,
    Restarting,
    Failed,
    LockHeld,
    Exited,
}

impl NodeLifecycleState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::Initializing => "initializing",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::Restarting => "restarting",
            Self::Failed => "failed",
            Self::LockHeld => "lock_held",
            Self::Exited => "exited",
        }
    }
}

/// 进程托管状态。
pub struct RuntimeState {
    /// 进程内运行的 Substrate 节点句柄。
    pub node_handle: Option<NodeHandle>,
    /// 首页展示与生命周期判断共用的状态。
    pub node_state: NodeLifecycleState,
    /// 最近一次启动/停止错误，供诊断日志与状态转换保留上下文。
    pub last_error: Option<String>,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            node_handle: None,
            node_state: NodeLifecycleState::Stopped,
            last_error: None,
        }
    }
}

/// Tauri 全局状态，供节点相关命令共享。
pub struct AppState(pub Mutex<RuntimeState>);

pub(super) fn lock_node_lifecycle() -> std::sync::MutexGuard<'static, ()> {
    NODE_LIFECYCLE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|err| err.into_inner())
}

pub(super) fn node_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    keystore::node_data_dir(app)
}

fn is_database_lock_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("lock hold by current process")
        || lower.contains("no locks available")
        || (lower.contains("/db/full/lock") && lower.contains("lock"))
}

fn state_for_start_error(error: &str) -> NodeLifecycleState {
    if is_database_lock_error(error) {
        NodeLifecycleState::LockHeld
    } else {
        NodeLifecycleState::Failed
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

fn local_chain_db_dir(base_path: &std::path::Path) -> PathBuf {
    base_path.join("chains").join(DEFAULT_CHAIN_ID).join("db")
}

fn dir_has_entries(path: &std::path::Path) -> Result<bool, String> {
    let mut entries =
        fs::read_dir(path).map_err(|e| format!("读取目录失败({}): {e}", path.display()))?;
    Ok(entries
        .next()
        .transpose()
        .map_err(|e| format!("读取目录项失败({}): {e}", path.display()))?
        .is_some())
}

fn genesis_state_source(app: &AppHandle) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(GENESIS_STATE_ENV).map(PathBuf::from) {
        if path.is_dir() {
            return Some(path);
        }
    }
    app.path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join(GENESIS_STATE_RESOURCE_DIR))
        .filter(|path| is_genesis_state_package(path) || has_genesis_state_marker(path))
}

fn is_genesis_state_package(path: &std::path::Path) -> bool {
    path.join("manifest.json").is_file()
        && path
            .join("chains")
            .join(DEFAULT_CHAIN_ID)
            .join("db")
            .is_dir()
}

fn has_genesis_state_marker(path: &std::path::Path) -> bool {
    path.join("manifest.json").exists() || path.join("chains").exists()
}

fn validate_genesis_state_manifest(source_root: &std::path::Path) -> Result<(), String> {
    let manifest = source_root.join("manifest.json");
    if !manifest.is_file() {
        return Err(format!(
            "创世链状态包缺少 manifest.json: {}",
            manifest.display()
        ));
    }
    let text = fs::read_to_string(&manifest)
        .map_err(|e| format!("读取创世状态包 manifest 失败({}): {e}", manifest.display()))?;
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("解析创世状态包 manifest 失败({}): {e}", manifest.display()))?;
    if json.get("package_format").and_then(|v| v.as_str()) != Some("citizenchain-genesis-state") {
        return Err("创世状态包 manifest.package_format 无效".to_string());
    }
    if json.get("chain_id").and_then(|v| v.as_str()) != Some(DEFAULT_CHAIN_ID) {
        return Err("创世状态包 manifest.chain_id 无效".to_string());
    }
    Ok(())
}

fn copy_dir_recursive(source: &std::path::Path, target: &std::path::Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|e| format!("创建目录失败({}): {e}", target.display()))?;
    for entry in
        fs::read_dir(source).map_err(|e| format!("读取目录失败({}): {e}", source.display()))?
    {
        let entry = entry.map_err(|e| format!("读取目录项失败({}): {e}", source.display()))?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let metadata = entry
            .metadata()
            .map_err(|e| format!("读取文件元数据失败({}): {e}", source_path.display()))?;
        if metadata.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else if metadata.is_file() {
            fs::copy(&source_path, &target_path).map_err(|e| {
                format!(
                    "复制创世状态文件失败({} -> {}): {e}",
                    source_path.display(),
                    target_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn install_genesis_state_if_available(
    app: &AppHandle,
    base_path: &std::path::Path,
) -> Result<(), String> {
    let target_db = local_chain_db_dir(base_path);
    if target_db.exists() {
        if dir_has_entries(&target_db)? {
            return Ok(());
        }
        fs::remove_dir_all(&target_db)
            .map_err(|e| format!("清理空创世数据库目录失败({}): {e}", target_db.display()))?;
    }
    let Some(source_root) = genesis_state_source(app) else {
        return Ok(());
    };
    let source_db = source_root.join("chains").join(DEFAULT_CHAIN_ID).join("db");
    if !source_db.is_dir() {
        return Err(format!(
            "创世链状态包缺少数据库目录: {}",
            source_db.display()
        ));
    }
    validate_genesis_state_manifest(&source_root)?;
    let target_chain_dir = base_path.join("chains").join(DEFAULT_CHAIN_ID);
    fs::create_dir_all(&target_chain_dir)
        .map_err(|e| format!("创建本地链目录失败({}): {e}", target_chain_dir.display()))?;

    // 只复制链数据库,不复制 keystore / network key。节点身份仍由本机生成和管理。
    let tmp_db = target_chain_dir.join("db.genesis-copying");
    if tmp_db.exists() {
        fs::remove_dir_all(&tmp_db)
            .map_err(|e| format!("清理临时创世数据库目录失败({}): {e}", tmp_db.display()))?;
    }
    copy_dir_recursive(&source_db, &tmp_db)?;
    fs::rename(&tmp_db, &target_db).map_err(|e| {
        let _ = fs::remove_dir_all(&tmp_db);
        format!(
            "安装创世状态数据库失败({} -> {}): {e}",
            tmp_db.display(),
            target_db.display()
        )
    })?;

    let source_manifest = source_root.join("manifest.json");
    if source_manifest.is_file() {
        let target_manifest = target_chain_dir.join("genesis-state-manifest.json");
        fs::copy(&source_manifest, &target_manifest).map_err(|e| {
            format!(
                "复制创世状态包 manifest 失败({} -> {}): {e}",
                source_manifest.display(),
                target_manifest.display()
            )
        })?;
    }
    eprintln!(
        "[节点] 已从创世链状态包初始化本地链数据库: {}",
        source_root.display()
    );
    Ok(())
}

fn needs_local_chain_initialization(base_path: &std::path::Path) -> Result<bool, String> {
    let target_db = local_chain_db_dir(base_path);
    if !target_db.exists() {
        return Ok(true);
    }
    dir_has_entries(&target_db).map(|has_entries| !has_entries)
}

fn set_runtime_state(
    app: &AppHandle,
    node_state: NodeLifecycleState,
    last_error: Option<String>,
) -> Result<(), String> {
    let app_state = app.state::<AppState>();
    let mut state = app_state
        .0
        .lock()
        .map_err(|_| "acquire process state failed".to_string())?;
    state.node_state = node_state;
    state.last_error = last_error;
    Ok(())
}

fn start_node_in_process_with_retry(
    base_path: PathBuf,
    chain_spec: Option<String>,
    rpc_port: u16,
    enable_grandpa_validator: bool,
    mining_threads: usize,
) -> Result<NodeHandle, String> {
    for attempt in 0..=NODE_LOCK_RETRY_LIMIT {
        match node_runner::start_node_in_process(
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
    rpc::clear_genesis_hash_cache();
}

/// 退出时清理：停止进程内节点。
/// 句柄 drop 内部会发送 shutdown 信号 + join 后台线程，
/// 并等待后台线程退出；若 RocksDB 同进程锁释放滞后，启动路径会转入 lock_held。
pub(crate) fn cleanup_on_exit(app: &AppHandle) {
    // 先停同步守护，避免退出时守护线程同时触发节点重启。
    super::sync_guard::stop_sync_guard();
    let _lifecycle_guard = lock_node_lifecycle();
    // 先把 handle 取出再 drop，避免在持有 state 锁期间 join 线程。
    let old_handle = match app.state::<AppState>().0.lock() {
        Ok(mut state) => {
            state.node_state = NodeLifecycleState::Stopping;
            state.last_error = None;
            state.node_handle.take()
        }
        Err(_) => None,
    };
    drop(old_handle);
    let _ = set_runtime_state(app, NodeLifecycleState::Stopped, None);
}

fn start_node_with_policy(app: AppHandle, restart_existing: bool) -> Result<NodeStatus, String> {
    let _lifecycle_guard = lock_node_lifecycle();
    if let Err(e) = security::append_audit_log(&app, "start_node", "attempt") {
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
                    .map(NodeHandle::is_alive)
                    .unwrap_or(false);
                if running && state.node_state == NodeLifecycleState::Running {
                    state.last_error = None;
                }
                running
            };
            if already_running {
                return current_status(&app);
            }
        }

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
                    NodeLifecycleState::Restarting
                } else {
                    NodeLifecycleState::Stopping
                }
            } else if restart_existing {
                NodeLifecycleState::Restarting
            } else {
                NodeLifecycleState::Starting
            };
            state.last_error = None;
            state.node_handle.take()
        };
        drop(old_handle);

        rpc::clear_genesis_hash_cache();
        thread::sleep(NODE_RESTART_SETTLE_DELAY);
        set_runtime_state(
            &app,
            if restart_existing {
                NodeLifecycleState::Restarting
            } else {
                NodeLifecycleState::Starting
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
            set_runtime_state(&app, NodeLifecycleState::Initializing, None)?;
        }
        install_genesis_state_if_available(&app, &base_path)?;
        // clean-run 本机重新创世时注入 fresh plain chainspec；普通启动仍用冻结主网 spec。
        let chain_spec = std::env::var("CITIZENCHAIN_CHAIN_SPEC")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let rpc_port = rpc::current_rpc_port();
        let enable_grandpa_validator = grandpa_address::prepare_grandpa_for_start(&app)?;
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
                NodeLifecycleState::Initializing
            } else if restart_existing {
                NodeLifecycleState::Restarting
            } else {
                NodeLifecycleState::Starting
            };
            state.last_error = None;
        }

        // 等待 RPC 就绪:只有能读到 chain_getBlockHash(0),首页和 OnChina 才能认为链可用。
        rpc::wait_for_local_rpc_ready(NODE_RPC_READY_TIMEOUT, || {
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
                    state.node_state = NodeLifecycleState::Failed;
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
            state.node_state = NodeLifecycleState::Running;
            state.last_error = None;
        }

        // 验证 GRANDPA 配置。
        if let Err(err) = grandpa_address::verify_grandpa_after_start(&app) {
            // 回滚：停止节点。
            let bad_handle = match app.state::<AppState>().0.lock() {
                Ok(mut state) => {
                    state.node_state = NodeLifecycleState::Failed;
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
    if let Err(e) = security::append_audit_log(
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
    if let Err(e) = security::append_audit_log(&app, "stop_node", "attempt") {
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
            state.node_state = NodeLifecycleState::Stopping;
            state.last_error = None;
            state.node_handle.take()
        };
        drop(old_handle);

        rpc::clear_genesis_hash_cache();
        thread::sleep(NODE_RESTART_SETTLE_DELAY);
        set_runtime_state(&app, NodeLifecycleState::Stopped, None)?;
        current_status(&app)
    })();
    if let Err(err) = &result {
        let _ = set_runtime_state(&app, NodeLifecycleState::Failed, Some(err.clone()));
    }
    if let Err(e) = security::append_audit_log(
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
pub async fn start_node(app: AppHandle) -> Result<NodeStatus, String> {
    super::join_blocking_task(
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
pub async fn stop_node(app: AppHandle) -> Result<NodeStatus, String> {
    super::join_blocking_task(
        "stop_node",
        tauri::async_runtime::spawn_blocking(move || stop_node_sync(app)),
    )
    .await
}
