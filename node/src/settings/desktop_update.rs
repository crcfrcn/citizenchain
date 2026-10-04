//! 桌面端更新前置准备。
//!
//! 更新安装由 Tauri updater 插件负责；本模块只负责在安装前停掉进程内节点，
//! 确保 RocksDB 文件锁和后台线程先释放。

use crate::home;
use tauri::AppHandle;

#[tauri::command]
pub async fn prepare_desktop_update(app: AppHandle) -> Result<(), String> {
    // downloadAndInstall 会触发安装器和进程重启，安装前先停节点，避免节点数据目录仍被占用。
    tauri::async_runtime::spawn_blocking(move || home::stop_node_blocking(app).map(|_| ()))
        .await
        .map_err(|err| format!("prepare desktop update task failed: {err}"))?
}
