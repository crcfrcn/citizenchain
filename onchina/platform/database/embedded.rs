//! 内嵌私有 PostgreSQL 生命周期(onchina 自管)。
//!
//! (Card 05):去中心化每市自治节点零依赖——onchina 在启动期把随安装包打进来的
//! PostgreSQL 二进制拉成**本机私有实例**(127.0.0.1,不对外),首启 `initdb`,起 `postgres`,
//! 建 `onchina` 库,返回 `DATABASE_URL`;退出期 `pg_ctl stop`。node 不碰 PG,只用 env 告诉
//! onchina 二进制目录/数据目录/端口(见 `node/src/onchina_proc`)。
//!
//! 两种部署:① 桌面/小市内嵌(`ONCHINA_EMBEDDED_PG=1`,本模块);② 大市外部托管 PG(直接给
//! `DATABASE_URL`,不启用本模块)。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// 私有实例默认端口(避开系统 PG 的 5432)。
const DEFAULT_PG_PORT: &str = "5433";
/// onchina 业务库名 / 超级用户名(本机 trust 鉴权,口令为空)。
const PG_DB_NAME: &str = "onchina";
const PG_SUPERUSER: &str = "postgres";
/// 就绪探活:最多等待轮次 × 间隔。
const READY_MAX_ATTEMPTS: u32 = 60;
const READY_INTERVAL: Duration = Duration::from_millis(500);

/// 是否启用内嵌 PG(桌面安装默认开;大市外部托管 PG 时关)。
pub(crate) fn is_enabled() -> bool {
    std::env::var("ONCHINA_EMBEDDED_PG")
        .ok()
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes"
        })
        .unwrap_or(false)
}

fn pg_port() -> String {
    std::env::var("ONCHINA_PG_PORT")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_PG_PORT.to_string())
}

/// PG 二进制目录:`ONCHINA_PG_BIN_DIR`(node 从 Tauri resource 解析后传入);
/// 兜底用 onchina 可执行文件同目录(开发期 `cargo build` 后手动放置或软链)。
fn pg_bin_dir() -> PathBuf {
    if let Some(dir) = std::env::var("ONCHINA_PG_BIN_DIR")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        return PathBuf::from(dir);
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// PG 数据目录:`ONCHINA_PG_DATA_DIR`(node 传 `base_path/pgdata`);兜底 exe 同目录 `pgdata`。
fn pg_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var("ONCHINA_PG_DATA_DIR")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        return PathBuf::from(dir);
    }
    pg_bin_dir().join("pgdata")
}

/// 解析某个 PG 可执行文件(带平台后缀)。
fn pg_tool(name: &str) -> PathBuf {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    pg_bin_dir().join(exe)
}

/// 给 PG 子进程注入保底 locale。`C` 三平台必带(macOS/Linux 无需生成、Windows 无害),
/// 规避 macOS「postmaster became multithreaded during startup」(无有效 locale 时
/// CoreFoundation 触发多线程,postmaster 直接 FATAL 退出)。与集群 `--no-locale`
/// (C 排序)一致,不改语义;UTF8 是编码,中文数据存取不受影响。
///
/// 必须显式设置:双击启动(macOS LaunchServices)与 launchd 都是无 `LANG` 的精简
/// 环境,只有从 Terminal 启动才会带 shell 的 locale——不能依赖用户机器环境。
fn apply_pg_locale(cmd: &mut Command) {
    cmd.env("LC_ALL", "C");
    cmd.env("LANG", "C");
}

fn run(tool: &Path, args: &[&str]) -> Result<(), String> {
    let mut cmd = Command::new(tool);
    cmd.args(args);
    apply_pg_locale(&mut cmd);
    let output = cmd
        .output()
        .map_err(|e| format!("run {} failed: {e}", tool.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} exited with {}: {}",
            tool.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

/// 本机 trust 鉴权下的连接串(无口令);`db` 指定连接库。
fn database_url_for(db: &str) -> String {
    format!("postgres://{PG_SUPERUSER}@127.0.0.1:{}/{db}", pg_port())
}

/// 确保内嵌 PG 已就绪并返回 onchina 业务库的 `DATABASE_URL`。
///
/// 幂等:已 initdb 不重做;postmaster 已在跑则复用;`onchina` 库存在则跳过建库。
pub(crate) fn ensure_started() -> Result<String, String> {
    let data_dir = pg_data_dir();
    let port = pg_port();

    // 1) 首启 initdb(以 `PG_VERSION` 文件判定是否已初始化)。
    if !data_dir.join("PG_VERSION").is_file() {
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| format!("create pg data dir failed: {e}"))?;
        let data = data_dir.to_string_lossy().to_string();
        run(
            &pg_tool("initdb"),
            &[
                "-D",
                data.as_str(),
                "-U",
                PG_SUPERUSER,
                "--auth-local=trust",
                "--auth-host=trust",
                "--encoding=UTF8",
                "--no-locale",
            ],
        )?;
        tracing::info!(data_dir = %data_dir.display(), "embedded postgres initialized");
    }

    // 既有私有集群同样更新所属配置，修复旧生成值而不重建数据。
    let archive_dir = std::env::var("ONCHINA_PG_WAL_ARCHIVE_DIR").ok();
    configure_postgresql_conf(&data_dir, archive_dir.as_deref())?;

    // 2) 未运行则启动；已运行时重载本产品配置，不遗留旧归档命令。
    if is_running(&data_dir) {
        let data = data_dir.to_string_lossy().to_string();
        run(&pg_tool("pg_ctl"), &["reload", "-D", data.as_str()])?;
    } else {
        let data = data_dir.to_string_lossy().to_string();
        let log = data_dir.join("postgres.log");
        let log_arg = log.to_string_lossy().to_string();
        // -o 透传 postmaster 选项:私有端口 + 只监听 127.0.0.1。
        let options = format!("-p {port} -h 127.0.0.1");
        run(
            &pg_tool("pg_ctl"),
            &[
                "start",
                "-D",
                data.as_str(),
                "-l",
                log_arg.as_str(),
                "-o",
                options.as_str(),
                "-w",
                "-t",
                "60",
            ],
        )?;
        tracing::info!(port = %port, "embedded postgres started");
    }

    // 3) 就绪探活(连 postgres 维护库)。
    wait_ready()?;

    // 4) 幂等建 onchina 业务库。
    ensure_database()?;

    Ok(database_url_for(PG_DB_NAME))
}

/// 退出期优雅停 PG(best-effort,不阻塞退出)。
pub(crate) fn stop() {
    let data_dir = pg_data_dir();
    if !is_running(&data_dir) {
        return;
    }
    let data = data_dir.to_string_lossy().to_string();
    match run(
        &pg_tool("pg_ctl"),
        &["stop", "-D", data.as_str(), "-m", "fast", "-w"],
    ) {
        Ok(()) => tracing::info!("embedded postgres stopped"),
        Err(err) => tracing::warn!(error = %err, "stop embedded postgres failed"),
    }
}

fn is_running(data_dir: &Path) -> bool {
    let data = data_dir.to_string_lossy().to_string();
    let mut cmd = Command::new(pg_tool("pg_ctl"));
    cmd.args(["status", "-D", data.as_str()]);
    apply_pg_locale(&mut cmd);
    cmd.output().map(|o| o.status.success()).unwrap_or(false)
}

/// 生成归档命令；路径作为单个Shell参数，保留空格、引号和美元符号的原意。
pub(crate) fn archive_command(directory: &str) -> String {
    if cfg!(windows) {
        format!("copy \"%p\" \"{directory}\\%f\"")
    } else {
        let target = format!("'{}/%f'", directory.replace('\'', "'\\''"));
        format!("test ! -f {target} && cp '%p' {target}")
    }
}

/// 只维护私有集群所属的四个设置，重复启动不追加重复配置、不改其它参数。
pub(crate) fn configure_postgresql_conf(
    data_dir: &Path,
    archive_dir: Option<&str>,
) -> Result<(), String> {
    let conf = data_dir.join("postgresql.conf");
    let original =
        std::fs::read_to_string(&conf).map_err(|e| format!("read postgresql.conf failed: {e}"))?;
    let marker = "# ── onchina 内嵌实例(Card 05)──";
    let retained = original
        .lines()
        .filter(|line| {
            line.trim() != marker
                && !line.split_once('=').is_some_and(|(key, _)| {
                    [
                        "listen_addresses",
                        "wal_level",
                        "archive_mode",
                        "archive_command",
                    ]
                    .contains(&key.trim())
                })
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut extra = format!("\n{marker}\nlisten_addresses = '127.0.0.1'\n");
    if let Some(directory) = archive_dir.map(str::trim).filter(|value| !value.is_empty()) {
        std::fs::create_dir_all(directory)
            .map_err(|e| format!("create wal archive dir failed: {e}"))?;
        // PostgreSQL配置字符串先还原Shell命令，再由Shell解析路径，两个层级分别转义。
        let command = archive_command(directory)
            .replace('\\', "\\\\")
            .replace('\'', "''");
        extra.push_str("wal_level = replica\narchive_mode = on\n");
        extra.push_str(&format!("archive_command = '{command}'\n"));
    } else {
        extra.push_str("archive_mode = off\n");
    }
    let updated = format!("{}{extra}", retained.trim_end());
    if updated != original {
        std::fs::write(&conf, updated).map_err(|e| format!("write postgresql.conf failed: {e}"))?;
    }
    Ok(())
}

fn wait_ready() -> Result<(), String> {
    let admin_url = database_url_for("postgres");
    for _ in 0..READY_MAX_ATTEMPTS {
        if postgres::Client::connect(admin_url.as_str(), postgres::NoTls).is_ok() {
            return Ok(());
        }
        std::thread::sleep(READY_INTERVAL);
    }
    Err("embedded postgres did not become ready in time".to_string())
}

fn ensure_database() -> Result<(), String> {
    let admin_url = database_url_for("postgres");
    let mut client = postgres::Client::connect(admin_url.as_str(), postgres::NoTls)
        .map_err(|e| format!("connect embedded postgres failed: {e}"))?;
    let exists: bool = client
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)",
            &[&PG_DB_NAME],
        )
        .map_err(|e| format!("query onchina database failed: {e}"))?
        .get(0);
    if !exists {
        // 库名为固定常量,无注入风险;CREATE DATABASE 不可在事务内。
        client
            .batch_execute(&format!("CREATE DATABASE \"{PG_DB_NAME}\""))
            .map_err(|e| format!("create onchina database failed: {e}"))?;
        tracing::info!(db = PG_DB_NAME, "embedded postgres database created");
    }
    Ok(())
}
