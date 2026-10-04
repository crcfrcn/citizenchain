// 首页模块入口，按职责拆分为进程管理、RPC 和身份管理三个子模块。
// 钱包/链上转账已下沉到 `transaction::onchain`，对齐 runtime/transaction/onchain 边界。

pub(crate) mod identity;
pub(crate) mod process;
pub(crate) mod rpc;
pub(crate) mod sync_guard;

// 公共类型与 Tauri 命令（Tauri command 注册在 desktop.rs 中使用子模块全路径）。
pub(crate) use process::{cleanup_on_exit, cleanup_on_startup, AppState, RuntimeState};

// crate 内部使用的阻塞版本函数。
pub(crate) use identity::{current_status, get_node_identity_blocking};
pub(crate) use process::{start_node_blocking, stop_node_blocking};

async fn join_blocking_task<T>(
    task: &'static str,
    result: tauri::async_runtime::JoinHandle<Result<T, String>>,
) -> Result<T, String> {
    result
        .await
        .map_err(|e| format!("{task} join failed: {e}"))?
}
