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
#[cfg(test)]
use nodedesktop::cid::parse_deployment;
use nodedesktop::cid::{deployment, onchina_binary_path, platform_url};
use serde::Serialize;
#[cfg(unix)]
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;
use tauri::AppHandle;
use tauri::Manager;

// 节点桌面端拉起 / 停止 onchina 控制台子进程。
//
// 形态:onchina 控制台是独立二进制(`onchina` crate)。节点桌面端不随启动自动拉起,
// 只在设置页二次确认后启动;App 退出时通过父管道通知并等待本进程拉起的服务退出。
//
// Card 05:onchina 二进制 + PostgreSQL 官方二进制 + 前端产物 + china.sqlite 均随安装包
// (Tauri resources)。本模块**不碰 PG**——只把资源/数据路径用环境变量告诉 onchina,
// onchina 自管内嵌 PG 与内网 TLS(见 `onchina/platform/database/embedded.rs 与 onchina/platform/security/tls.rs`)。

/// 进程内唯一的 onchina 子进程句柄。
static ONCHINA_CHILD: Mutex<Option<Child>> = Mutex::new(None);

static ONCHINA_ERROR: Mutex<Option<String>> = Mutex::new(None);
static ONCHINA_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn startup_error() -> Option<String> {
    ONCHINA_ERROR.lock().ok().and_then(|value| value.clone())
}

/// 只读取宿主公开的去敏错误，不把 PostgreSQL、账户、密钥或完整日志送入界面。
fn capture_startup_errors(stdout: std::process::ChildStdout, pid: u32) {
    std::thread::spawn(move || {
        use std::io::Read;
        let mut reader = stdout;
        let mut chunk = [0u8; 1024];
        let mut line = Vec::new();
        let mut oversized = false;
        while let Ok(count) = reader.read(&mut chunk) {
            if count == 0 {
                break;
            }
            for byte in &chunk[..count] {
                if *byte == b'\n' {
                    if !oversized {
                        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&line) {
                            if let Some(error) = value
                                .get("onchina_error")
                                .and_then(|value| value.as_str())
                                .filter(|value| {
                                    value.len() <= 2048 && !value.chars().any(char::is_control)
                                })
                            {
                                if ONCHINA_PID.load(std::sync::atomic::Ordering::Acquire) == pid {
                                    if let Ok(mut slot) = ONCHINA_ERROR.lock() {
                                        *slot = Some(error.to_string());
                                    }
                                }
                            }
                        }
                    }
                    line.clear();
                    oversized = false;
                } else if line.len() < 4096 && !oversized {
                    line.push(*byte);
                } else {
                    line.clear();
                    oversized = true;
                }
            }
        }
    });
}

/// 清理旧进程后等待端口释放的次数。每次 200ms，总等待约 40 秒，覆盖模块退出收尾。
#[cfg(unix)]
const STALE_CLEANUP_ATTEMPTS: usize = 200;

/// 当前平台在 `resources/postgres/<os>/` 下的子目录名。
fn pg_os_subdir() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// 把内嵌 PG / 内网 TLS / 前端产物 / china.sqlite / 链 RPC 的路径用环境变量传给 onchina。
///
/// 仅当随包 PostgreSQL 二进制确实存在(打包构建)才开内嵌 PG + HTTPS;
/// 开发期(cargo build,无随包 PG)继承启动脚本中的数据库与 HTTPS 配置,不强行覆盖。
fn apply_onchina_env(app: &AppHandle, cmd: &mut Command) -> Result<(), String> {
    // 中文注释：沿用同一显式 HTTPS 链入口，仅转换为 WSS；不覆盖成未经 TLS 保护的本机地址。
    let mut ws_url = reqwest::Url::parse(&nodedesktop::rpc::local_rpc_http_url()?)
        .map_err(|_| "链 RPC TLS 入口无效".to_string())?;
    ws_url
        .set_scheme("wss")
        .map_err(|_| "链 RPC WSS 转换失败".to_string())?;
    cmd.env("ONCHAIN_WS_URL", ws_url.as_str());
    // 子进程只收到本次桌面会话证书的公开DER；私钥始终留在节点进程内。
    let certificate = nodedesktop::tls::desktop_rpc_certificate()
        .ok_or_else(|| "桌面 RPC 证书尚未准备".to_string())?;
    cmd.env("ONCHAIN_LOCAL_RPC_CERTIFICATE", hex::encode(certificate));

    // 只有**打包形态**(随包 PostgreSQL 官方二进制存在)才用随包资源路径覆盖
    // china.sqlite / 前端产物 / 内嵌 PG / TLS;开发期(cargo tauri dev,无随包 PG)这些全
    // 由启动脚本 run.sh/clean-run.sh 的环境变量提供,节点启动入口不覆盖,避免拿占位资源
    // 路径把 dev 配置盖掉。
    let Some(res) = app.path().resource_dir().ok() else {
        return Ok(());
    };
    let pg_bin_dir = res.join("postgres").join(pg_os_subdir()).join("bin");
    let initdb = pg_bin_dir.join(if cfg!(windows) {
        "initdb.exe"
    } else {
        "initdb"
    });
    if !initdb.exists() {
        return Ok(());
    }
    cmd.env("ONCHINA_CHINA_DB", res.join("china.sqlite"));
    cmd.env(
        "ONCHINA_FRONTEND_DIST",
        res.join("onchina-frontend").join("dist"),
    );
    cmd.env("ONCHINA_EMBEDDED_PG", "1");
    cmd.env("ONCHINA_PG_BIN_DIR", &pg_bin_dir);
    if let Ok(base) = nodedesktop::security::app_data_dir(app) {
        apply_data_dir_env(&base, cmd);
    }
    Ok(())
}

/// 数据目录派生的环境变量:PG 数据目录 / TLS 证书目录 / WAL 归档目录(与节点数据同根)。
fn apply_data_dir_env(base: &Path, cmd: &mut Command) {
    cmd.env("ONCHINA_PG_DATA_DIR", base.join("pgdata"));
    cmd.env("ONCHINA_TLS_DIR", base.join("onchina-tls"));
    // 默认归档到节点数据目录；机房部署可由运维将归档目录指向 NAS。
    cmd.env("ONCHINA_PG_WAL_ARCHIVE_DIR", base.join("pg-wal-archive"));
}

/// 清理已经退出的 onchina 子进程句柄,避免状态误判为运行中。
fn reap_finished_child(child: &mut Option<Child>) {
    let Some(running) = child.as_mut() else {
        return;
    };
    match running.try_wait() {
        Ok(None) => {}
        Ok(Some(status)) => {
            eprintln!("[onchina] onchina 控制台子进程已退出,status={status}");
            *child = None;
        }
        Err(err) => {
            eprintln!("[onchina] 检查 onchina 控制台子进程状态失败:{err}");
            // 查询失败不能丢弃仍可能存活的进程所有权。
        }
    }
}

#[cfg(unix)]
fn command_stdout(program: &str, args: &[String]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(unix)]
fn pids_from_stdout(stdout: &str) -> BTreeSet<u32> {
    stdout
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .filter(|pid| *pid != std::process::id())
        .collect()
}

#[cfg(unix)]
fn listener_pids_on_onchina_port(port: u16) -> BTreeSet<u32> {
    let port = format!("-iTCP:{port}");
    command_stdout(
        "lsof",
        &[
            "-nP".to_string(),
            "-t".to_string(),
            port,
            "-sTCP:LISTEN".to_string(),
        ],
    )
    .map(|stdout| pids_from_stdout(&stdout))
    .unwrap_or_default()
}

#[cfg(unix)]
fn onchina_named_pids() -> BTreeSet<u32> {
    command_stdout("pgrep", &["-x".to_string(), "onchina".to_string()])
        .map(|stdout| pids_from_stdout(&stdout))
        .unwrap_or_default()
}

#[cfg(unix)]
fn process_command(pid: u32) -> String {
    command_stdout(
        "ps",
        &[
            "-p".to_string(),
            pid.to_string(),
            "-o".to_string(),
            "comm=".to_string(),
            "-o".to_string(),
            "args=".to_string(),
        ],
    )
    .unwrap_or_default()
}

#[cfg(unix)]
fn is_onchina_process(pid: u32) -> bool {
    process_command(pid)
        .split_whitespace()
        .any(|part| part == "onchina" || part.ends_with("/onchina"))
}

#[cfg(unix)]
fn signal_pid(pid: u32, signal: &str) {
    let _ = Command::new("kill")
        .arg(signal)
        .arg(pid.to_string())
        .status();
}

#[cfg(unix)]
fn stale_onchina_processes(port: u16) -> BTreeSet<u32> {
    let mut pids = onchina_named_pids();
    for pid in listener_pids_on_onchina_port(port) {
        if is_onchina_process(pid) {
            pids.insert(pid);
        }
    }
    pids
}

#[cfg(unix)]
fn foreign_onchina_port_listeners(port: u16) -> Vec<u32> {
    listener_pids_on_onchina_port(port)
        .into_iter()
        .filter(|pid| !is_onchina_process(*pid))
        .collect()
}

#[cfg(unix)]
fn wait_until_no_stale_onchina_processes(port: u16) -> Result<(), String> {
    for _ in 0..STALE_CLEANUP_ATTEMPTS {
        let foreign_port_pids = foreign_onchina_port_listeners(port);
        if !foreign_port_pids.is_empty() {
            return Err(format!(
                "链上中国平台端口 {port} 已被非 OnChina 进程占用,pid={foreign_port_pids:?}"
            ));
        }
        if stale_onchina_processes(port).is_empty() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(format!(
        "旧链上中国平台进程仍未退出,pid={:?}",
        stale_onchina_processes(port)
    ))
}

/// 启动前清理旧 OnChina 孤儿进程。
///
/// 节点桌面端只持有“本次启动出来”的 child handle；如果上一次节点异常退出，
/// OnChina 可能留成 PPID=1 的孤儿进程并继续占用产品声明的固定端口。这里在用户点击“启动”
/// 时先按进程名和固定端口清理旧 OnChina，避免新进程连到陈旧数据库连接池后崩溃。
#[cfg(unix)]
fn clean_stale_onchina_processes_before_start(port: u16) -> Result<(), String> {
    let foreign_port_pids = foreign_onchina_port_listeners(port);
    if !foreign_port_pids.is_empty() {
        return Err(format!(
            "链上中国平台端口 {port} 已被非 OnChina 进程占用,pid={foreign_port_pids:?}"
        ));
    }

    let pids = stale_onchina_processes(port);
    if pids.is_empty() {
        return Ok(());
    }

    for pid in &pids {
        signal_pid(*pid, "-TERM");
    }
    // 等待宿主自行回收模块；超时拒绝重启，不强杀宿主后留下仍占用文件的模块。
    wait_until_no_stale_onchina_processes(port)?;
    eprintln!("[onchina] 已清理旧链上中国平台进程 pid={pids:?}");
    Ok(())
}

#[cfg(not(unix))]
fn clean_stale_onchina_processes_before_start(_port: u16) -> Result<(), String> {
    Ok(())
}

/// 返回由本节点桌面端拉起的 onchina 子进程是否仍在运行。
pub(crate) fn is_onchina_running() -> bool {
    let mut guard = match ONCHINA_CHILD.lock() {
        Ok(guard) => guard,
        Err(err) => {
            eprintln!("[onchina] 获取子进程锁失败:{err}");
            return false;
        }
    };
    reap_finished_child(&mut guard);
    guard.is_some()
}

/// 启动 onchina 子进程;已在运行则直接返回当前状态。
///
/// 找不到二进制时返回明确错误,由设置页展示给用户
/// (开发期先 `cargo build -p onchina` 生成二进制)。
pub(crate) fn start_onchina(app: &AppHandle) -> Result<(), String> {
    let mut guard = match ONCHINA_CHILD.lock() {
        Ok(guard) => guard,
        Err(err) => return Err(format!("获取链上中国平台子进程锁失败:{err}")),
    };
    reap_finished_child(&mut guard);
    if guard.is_some() {
        return Ok(());
    }
    let deployment = deployment(app)?;
    clean_stale_onchina_processes_before_start(deployment.address.port())?;
    if let Ok(mut error) = ONCHINA_ERROR.lock() {
        *error = None;
    }
    // OnChina 启动入口依赖链 RPC 读取 genesis hash 和链上投影。节点线程存在但
    // 创世 state 尚未物化完成时,这里必须 fail-fast,避免子进程启动后 panic。
    nodedesktop::rpc::wait_for_local_rpc_ready(Duration::from_secs(3), || Ok(()))
        .map_err(|err| format!("链上中国平台启动前检查失败: 本机区块链 RPC 尚未就绪。{err}"))?;
    let Some(binary) = onchina_binary_path(app) else {
        return Err("未找到链上中国平台二进制;开发期请先执行 cargo build -p onchina".to_string());
    };
    let mut cmd = Command::new(&binary);
    cmd.env("ONCHINA_PARENT_PIPE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    apply_onchina_env(app, &mut cmd)?;
    match cmd.spawn() {
        Ok(mut child) => {
            ONCHINA_PID.store(child.id(), std::sync::atomic::Ordering::Release);
            if let Some(stdout) = child.stdout.take() {
                capture_startup_errors(stdout, child.id());
            }
            eprintln!("[onchina] 已启动 onchina 控制台子进程 pid={}", child.id());
            *guard = Some(child);
            Ok(())
        }
        Err(err) => Err(format!("启动链上中国平台子进程失败:{err}")),
    }
}

/// 仅在退出码和进程回收已确认后成功；超时仍由调用方保留原句柄。
fn wait_onchina_exit(child: &mut Child, timeout: Duration) -> Result<(), String> {
    let start = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("等待链上中国退出失败:{error}"))?
        {
            return if status.success() {
                Ok(())
            } else {
                Err(format!("链上中国退出异常:{status}"))
            };
        }
        if start.elapsed() >= timeout {
            return Err("链上中国及模块尚未确认退出，禁止替换运行文件".into());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// 关闭父管道，等待宿主排空、模块停止及宿主退出；不直接强杀宿主而遗留模块。
pub(crate) fn stop_onchina_checked() -> Result<(), String> {
    let mut guard = ONCHINA_CHILD
        .lock()
        .map_err(|error| format!("获取链上中国平台子进程锁失败:{error}"))?;
    if let Some(child) = guard.as_mut() {
        drop(child.stdin.take());
        // 覆盖HTTPS排空10秒、模块排空12秒及启动中说明读取的退出收尾。
        wait_onchina_exit(child, Duration::from_secs(40))?;
        *guard = None;
        eprintln!("[onchina] 已确认宿主及模块退出");
    }
    ONCHINA_PID.store(0, std::sync::atomic::Ordering::Release);
    if let Ok(mut error) = ONCHINA_ERROR.lock() {
        *error = None;
    }
    Ok(())
}

/// 停止 onchina 子进程(App 退出时调用)。
pub(crate) fn stop_onchina() {
    if let Err(err) = stop_onchina_checked() {
        eprintln!("[onchina] 停止 onchina 控制台子进程失败:{err}");
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnChinaPlatformState {
    running: bool,
    status: &'static str,
    status_label: &'static str,
    url: String,
    detail: Option<String>,
}

// 与浏览器同样验证公信 HTTPS；仅把连接指向产品声明的主机地址，保持域名 SNI。
fn onchina_health_ok(app: &AppHandle) -> Result<(), String> {
    let deployment = deployment(app)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(900))
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .resolve(&deployment.host, deployment.address)
        .build()
        .map_err(|e| format!("创建链上中国健康检查客户端失败:{e}"))?;
    let response = client
        .get(format!("{}api/health", deployment.url))
        .send()
        .map_err(|_| "链上中国可信 HTTPS 健康检查失败".to_string())?;
    if !response.status().is_success() {
        return Err("链上中国健康检查状态无效".to_string());
    }
    let payload = response
        .json::<serde_json::Value>()
        .map_err(|_| "链上中国健康检查响应无效".to_string())?;
    if payload
        .get("data")
        .and_then(|data| data.get("status"))
        .and_then(|value| value.as_str())
        == Some("UP")
    {
        Ok(())
    } else {
        Err("链上中国平台健康检查未通过".to_string())
    }
}

fn current_state(app: &AppHandle) -> OnChinaPlatformState {
    if let Err(error) = deployment(app) {
        return failed_state(false, error);
    }
    let process_running = crate::platform::is_onchina_running();
    if !process_running && ONCHINA_PID.load(std::sync::atomic::Ordering::Acquire) != 0 {
        return failed_state(
            false,
            startup_error().unwrap_or_else(|| "链上中国子进程已退出".into()),
        );
    }
    let health = process_running.then(|| onchina_health_ok(app));
    let (status, status_label, detail) = match health {
        None => ("stopped", "未开启", None),
        Some(Ok(())) => ("enabled", "已开启", None),
        // 进程已启动但健康检查暂未通过时，只能表达“启动中”。
        // 退出后从公开诊断通道报告具体失败；活跃但未通过 HTTPS 的进程保持启动中。
        Some(Err(error)) => ("starting", "启动中", Some(error)),
    };
    OnChinaPlatformState {
        running: process_running,
        status,
        status_label,
        url: platform_url(),
        detail,
    }
}

fn failed_state(running: bool, detail: String) -> OnChinaPlatformState {
    OnChinaPlatformState {
        running,
        status: "error",
        status_label: "启动失败",
        url: platform_url(),
        detail: Some(detail),
    }
}

#[tauri::command]
pub(crate) async fn get_onchina_platform(app: AppHandle) -> Result<OnChinaPlatformState, String> {
    tauri::async_runtime::spawn_blocking(move || current_state(&app))
        .await
        .map_err(|_| "查询链上中国状态失败".into())
}

#[tauri::command]
pub(crate) async fn start_onchina_platform(app: AppHandle) -> Result<OnChinaPlatformState, String> {
    tauri::async_runtime::spawn_blocking(move || start_platform(&app))
        .await
        .map_err(|_| "启动链上中国任务失败".into())
}
fn start_platform(app: &AppHandle) -> OnChinaPlatformState {
    if let Err(err) =
        nodedesktop::security::append_audit_log(&app, "start_onchina_platform", "attempt")
    {
        eprintln!("[审计] start_onchina_platform attempt 日志写入失败:{err}");
    }
    if let Err(err) = crate::platform::start_onchina(&app) {
        if let Err(log_err) =
            nodedesktop::security::append_audit_log(&app, "start_onchina_platform", "failed")
        {
            eprintln!("[审计] start_onchina_platform failed 日志写入失败:{log_err}");
        }
        return failed_state(false, err);
    }
    if let Err(err) =
        nodedesktop::security::append_audit_log(&app, "start_onchina_platform", "success")
    {
        eprintln!("[审计] start_onchina_platform success 日志写入失败:{err}");
    }
    current_state(app)
}

#[tauri::command]
pub(crate) async fn stop_onchina_platform(app: AppHandle) -> Result<OnChinaPlatformState, String> {
    tauri::async_runtime::spawn_blocking(move || stop_platform(&app))
        .await
        .map_err(|_| "关闭链上中国任务失败".to_string())?
}
fn stop_platform(app: &AppHandle) -> Result<OnChinaPlatformState, String> {
    if let Err(err) =
        nodedesktop::security::append_audit_log(&app, "stop_onchina_platform", "attempt")
    {
        eprintln!("[审计] stop_onchina_platform attempt 日志写入失败:{err}");
    }
    crate::platform::stop_onchina_checked()?;
    if let Err(err) =
        nodedesktop::security::append_audit_log(&app, "stop_onchina_platform", "success")
    {
        eprintln!("[审计] stop_onchina_platform success 日志写入失败:{err}");
    }
    Ok(current_state(&app))
}
#[cfg(test)]
#[allow(clippy::expect_used)]
#[path = "tests/unit.rs"]
mod startup_tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
#[path = "tests/platform.rs"]
mod lifecycle_tests;
