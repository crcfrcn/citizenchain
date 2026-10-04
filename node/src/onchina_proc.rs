//! 节点桌面端拉起 / 停止 onchina 控制台子进程。
//!
//! 形态:onchina 控制台是独立二进制(`onchina` crate)。节点桌面端不随启动自动拉起,
//! 只在设置页二次确认后启动;App 退出时统一杀掉已由本进程拉起的子进程。
//!
//! Card 05:onchina 二进制 + PostgreSQL 官方二进制 + 前端产物 + china.sqlite 均随安装包
//! (Tauri resources)。本模块**不碰 PG**——只把资源/数据路径用环境变量告诉 onchina,
//! onchina 自管内嵌 PG 与内网 TLS(见 `onchina/src/core/{embedded_pg,tls}.rs`)。

#[cfg(unix)]
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager};

/// 进程内唯一的 onchina 子进程句柄。
static ONCHINA_CHILD: Mutex<Option<Child>> = Mutex::new(None);

/// OnChina 平台固定监听端口；节点启动按钮只能管理这一套本机平台服务。
#[cfg(unix)]
const ONCHINA_PORT: u16 = 8964;

/// 清理旧进程后等待端口释放的次数。每次 200ms，总等待约 4 秒。
#[cfg(unix)]
const STALE_CLEANUP_ATTEMPTS: usize = 20;

/// 解析 onchina 二进制路径:优先随包 `resources/onchina-bin/`(打包形态),
/// 兜底节点可执行文件同目录(开发期 `cargo build` 后的 `target/<profile>/onchina`)。
fn onchina_binary_path(app: &AppHandle) -> Option<PathBuf> {
    let name = if cfg!(target_os = "windows") {
        "onchina.exe"
    } else {
        "onchina"
    };
    if let Ok(res) = app.path().resource_dir() {
        let packaged = res.join("onchina-bin").join(name);
        if packaged.exists() {
            ensure_executable(&packaged);
            return Some(packaged);
        }
    }
    let exe = std::env::current_exe().ok()?;
    let dev = exe.parent()?.join(name);
    if dev.exists() {
        Some(dev)
    } else {
        None
    }
}

/// Unix:随包资源可能丢失可执行位,拉起前补上(best-effort)。
#[cfg(unix)]
fn ensure_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perms = meta.permissions();
        perms.set_mode(perms.mode() | 0o755);
        let _ = std::fs::set_permissions(path, perms);
    }
}

#[cfg(not(unix))]
fn ensure_executable(_path: &Path) {}

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
    let mut ws_url = reqwest::Url::parse(&crate::shared::rpc::local_rpc_http_url()?)
        .map_err(|_| "链 RPC TLS 入口无效".to_string())?;
    ws_url
        .set_scheme("wss")
        .map_err(|_| "链 RPC WSS 转换失败".to_string())?;
    cmd.env("ONCHAIN_WS_URL", ws_url.as_str());

    // 只有**打包形态**(随包 PostgreSQL 官方二进制存在)才用随包资源路径覆盖
    // china.sqlite / 前端产物 / 内嵌 PG / TLS;开发期(cargo tauri dev,无随包 PG)这些全
    // 由启动脚本 run.sh/clean-run.sh 的环境变量提供,onchina_proc 不覆盖,避免拿占位资源
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
    cmd.env("ONCHINA_ENABLE_TLS", "1");
    cmd.env("ONCHINA_PG_BIN_DIR", &pg_bin_dir);
    if let Ok(base) = crate::shared::security::app_data_dir(app) {
        apply_data_dir_env(&base, cmd);
    }
    Ok(())
}

/// 数据目录派生的环境变量:PG 数据目录 / TLS 证书目录 / WAL 归档目录(与节点数据同根)。
fn apply_data_dir_env(base: &Path, cmd: &mut Command) {
    cmd.env("ONCHINA_PG_DATA_DIR", base.join("pgdata"));
    cmd.env("ONCHINA_TLS_DIR", base.join("onchina-tls"));
    // 默认本地 WAL 归档;大市部署由运维把 ONCHINA_PG_WAL_ARCHIVE_DIR 指向 NAS(见 citizenchain/scripts/onchina-{backup,restore}.sh)。
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
            *child = None;
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
fn listener_pids_on_onchina_port() -> BTreeSet<u32> {
    let port = format!("-iTCP:{ONCHINA_PORT}");
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
fn stale_onchina_processes() -> BTreeSet<u32> {
    let mut pids = onchina_named_pids();
    for pid in listener_pids_on_onchina_port() {
        if is_onchina_process(pid) {
            pids.insert(pid);
        }
    }
    pids
}

#[cfg(unix)]
fn foreign_onchina_port_listeners() -> Vec<u32> {
    listener_pids_on_onchina_port()
        .into_iter()
        .filter(|pid| !is_onchina_process(*pid))
        .collect()
}

#[cfg(unix)]
fn wait_until_no_stale_onchina_processes() -> Result<(), String> {
    for _ in 0..STALE_CLEANUP_ATTEMPTS {
        let foreign_port_pids = foreign_onchina_port_listeners();
        if !foreign_port_pids.is_empty() {
            return Err(format!(
                "链上中国平台端口 {ONCHINA_PORT} 已被非 OnChina 进程占用,pid={foreign_port_pids:?}"
            ));
        }
        if stale_onchina_processes().is_empty() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(format!(
        "旧链上中国平台进程仍未退出,pid={:?}",
        stale_onchina_processes()
    ))
}

/// 启动前清理旧 OnChina 孤儿进程。
///
/// 节点桌面端只持有“本次启动出来”的 child handle；如果上一次节点异常退出，
/// OnChina 可能留成 PPID=1 的孤儿进程并继续占用 8964。这里在用户点击“启动”
/// 时先按进程名和固定端口清理旧 OnChina，避免新进程连到陈旧数据库连接池后崩溃。
#[cfg(unix)]
fn clean_stale_onchina_processes_before_start() -> Result<(), String> {
    let foreign_port_pids = foreign_onchina_port_listeners();
    if !foreign_port_pids.is_empty() {
        return Err(format!(
            "链上中国平台端口 {ONCHINA_PORT} 已被非 OnChina 进程占用,pid={foreign_port_pids:?}"
        ));
    }

    let pids = stale_onchina_processes();
    if pids.is_empty() {
        return Ok(());
    }

    for pid in &pids {
        signal_pid(*pid, "-TERM");
    }
    if wait_until_no_stale_onchina_processes().is_err() {
        for pid in stale_onchina_processes() {
            signal_pid(pid, "-KILL");
        }
    }
    wait_until_no_stale_onchina_processes()?;
    eprintln!("[onchina] 已清理旧链上中国平台进程 pid={pids:?}");
    Ok(())
}

#[cfg(not(unix))]
fn clean_stale_onchina_processes_before_start() -> Result<(), String> {
    Ok(())
}

/// 返回由本节点桌面端拉起的 onchina 子进程是否仍在运行。
pub fn is_onchina_running() -> bool {
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
pub fn start_onchina(app: &AppHandle) -> Result<(), String> {
    let mut guard = match ONCHINA_CHILD.lock() {
        Ok(guard) => guard,
        Err(err) => return Err(format!("获取链上中国平台子进程锁失败:{err}")),
    };
    reap_finished_child(&mut guard);
    if guard.is_some() {
        return Ok(());
    }
    clean_stale_onchina_processes_before_start()?;
    // OnChina 启动入口依赖链 RPC 读取 genesis hash 和链上投影。节点线程存在但
    // 创世 state 尚未物化完成时,这里必须 fail-fast,避免子进程启动后 panic。
    crate::shared::rpc::wait_for_local_rpc_ready(Duration::from_secs(3), || Ok(()))
        .map_err(|err| format!("链上中国平台启动前检查失败: 本机区块链 RPC 尚未就绪。{err}"))?;
    let Some(binary) = onchina_binary_path(app) else {
        return Err("未找到链上中国平台二进制;开发期请先执行 cargo build -p onchina".to_string());
    };
    let mut cmd = Command::new(&binary);
    apply_onchina_env(app, &mut cmd)?;
    match cmd.spawn() {
        Ok(child) => {
            eprintln!("[onchina] 已启动 onchina 控制台子进程 pid={}", child.id());
            *guard = Some(child);
            Ok(())
        }
        Err(err) => Err(format!("启动链上中国平台子进程失败:{err}")),
    }
}

/// 停止 onchina 子进程;未运行时视为成功。
pub fn stop_onchina_checked() -> Result<(), String> {
    let mut guard = match ONCHINA_CHILD.lock() {
        Ok(guard) => guard,
        Err(err) => return Err(format!("获取链上中国平台子进程锁失败:{err}")),
    };
    reap_finished_child(&mut guard);
    if let Some(mut child) = guard.take() {
        child
            .kill()
            .map_err(|err| format!("停止链上中国平台子进程失败:{err}"))?;
        let _ = child.wait();
        eprintln!("[onchina] 已停止 onchina 控制台子进程");
    }
    Ok(())
}

/// 停止 onchina 子进程(App 退出时调用)。
pub fn stop_onchina() {
    if let Err(err) = stop_onchina_checked() {
        eprintln!("[onchina] 停止 onchina 控制台子进程失败:{err}");
    }
}
