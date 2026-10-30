use nodeclient::handle::NodeHandle;

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;

use tauri::AppHandle;
use tauri::Manager;

// 串行化节点启停，避免并发命令冲突。
static NODE_LIFECYCLE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

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
            node_state: crate::state::NodeLifecycleState::Stopped,
            last_error: None,
        }
    }
}

/// Tauri 全局状态，供节点相关命令共享。
pub struct AppState(pub Mutex<RuntimeState>);

pub fn lock_node_lifecycle() -> std::sync::MutexGuard<'static, ()> {
    NODE_LIFECYCLE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|err| err.into_inner())
}

pub fn node_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    nodedesktop::keystore::node_data_dir(app)
}

pub fn set_runtime_state(
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

#[cfg(test)]
#[path = "../tests/state.rs"]
mod tests;
