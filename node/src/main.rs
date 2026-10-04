//! 公民链节点 — 区块链节点 + 桌面界面合一。
//!
//! 自动适应环境：
//! - 有显示器：打开桌面窗口 + 区块链节点
//! - 无显示器（服务器）：直接运行区块链节点
//! - 有子命令（build-spec 等）：运行工具命令后退出

// release 构建走 windows subsystem,Windows 双击 exe 不会附带弹控制台,
// 也不会因为关闭控制台触发 CTRL_CLOSE_EVENT 把进程杀掉;
// dev 构建保留 console subsystem,便于看 eprintln!/log 输出。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// 测试断言失败必须立即终止；保留带上下文的 unwrap/expect，生产构建不受此豁免影响。
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]
#![warn(missing_docs)]

mod admins;
mod core;
mod desktop;
mod governance;
mod home;
mod mining;
mod onchina_proc;
mod other;
mod settings;
mod shared;
mod transaction;

fn main() {
    // 有子命令（build-spec、purge-chain 等）→ CLI 工具模式
    let args: Vec<String> = std::env::args().collect();
    let has_subcommand = args.len() > 1 && !args[1].starts_with('-');
    if has_subcommand {
        if let Err(e) = crate::core::command::run() {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }

    // 调试用逃生口：CITIZENCHAIN_HEADLESS=1 强制无头模式（绕过 GUI），
    // 用来在另一个端口/数据目录跑诊断节点，不影响桌面 GUI 实例。
    if std::env::var("CITIZENCHAIN_HEADLESS").is_ok() {
        eprintln!("CITIZENCHAIN_HEADLESS 已设置，以无头模式运行节点...");
        if let Err(e) = crate::core::command::run() {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }

    // 检测是否有显示环境（Linux 看 DISPLAY/WAYLAND_DISPLAY，macOS/Windows 始终有）
    if has_display() {
        // 有显示器 → 桌面窗口 + 内嵌节点
        desktop::run_desktop();
    } else {
        // 无显示器（服务器）→ 直接运行节点
        eprintln!("未检测到显示环境，以无窗口模式运行节点...");
        if let Err(e) = crate::core::command::run() {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

/// 检测当前环境是否有显示器。
fn has_display() -> bool {
    if cfg!(target_os = "macos") || cfg!(target_os = "windows") {
        return true;
    }
    // Linux：检查 DISPLAY 或 WAYLAND_DISPLAY
    std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok()
}
