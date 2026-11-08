//! 节点共用宿主的公开 HTTPS 入口与本次子进程回传的实际监听地址。
use serde::Deserialize;
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
};
use tauri::{AppHandle, Manager};

/// 部署信息由产品宿主公开说明提供，节点不复制域名、地址或端口常量。
#[derive(Clone, Debug, Deserialize)]
pub struct PlatformDeployment {
    /// TLS 主机名。
    pub host: String,
    /// 公开 HTTPS 入口端口。
    pub port: u16,
    /// 经过 HTTPS 与端点一致性验证的浏览器入口。
    pub url: String,
}
static ONCHINA_DEPLOYMENT: OnceLock<PlatformDeployment> = OnceLock::new();

struct LocalPlatform {
    pid: u32,
    address: Option<SocketAddr>,
}

static LOCAL_PLATFORM: Mutex<Option<LocalPlatform>> = Mutex::new(None);

/// 为本次宿主进程登记地址回执的接收者。
pub fn begin_local_platform(pid: u32) {
    if let Ok(mut local) = LOCAL_PLATFORM.lock() {
        *local = Some(LocalPlatform { pid, address: None });
    }
}

/// 只接受当前宿主进程的已校验回执。
pub fn record_local_platform_address(pid: u32, address: SocketAddr) {
    if let Ok(mut local) = LOCAL_PLATFORM.lock() {
        if let Some(local) = local.as_mut().filter(|local| local.pid == pid) {
            local.address = Some(address);
        }
    }
}

/// 宿主退出后释放其地址，忽略已被替换的旧进程。
pub fn end_local_platform(pid: u32) {
    if let Ok(mut local) = LOCAL_PLATFORM.lock() {
        if local.as_ref().is_some_and(|local| local.pid == pid) {
            *local = None;
        }
    }
}

/// 当前桌面端拥有的链上中国宿主监听地址。
pub fn local_platform_address() -> Option<SocketAddr> {
    LOCAL_PLATFORM.lock().ok()?.as_ref()?.address
}

fn validate_base_url(value: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(value).map_err(|_| "OnChina地址无效".to_string())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("OnChina地址必须是无凭据、查询或片段的HTTPS基地址".to_string());
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

/// 校验宿主公开说明，不接受含凭据、非 HTTPS 或不一致的入口。
pub fn parse_deployment(bytes: &[u8]) -> Result<PlatformDeployment, String> {
    let description: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "链上中国公开部署说明无效")?;
    let mut value: PlatformDeployment = serde_json::from_value(
        description
            .get("deployment")
            .cloned()
            .ok_or("链上中国未声明部署入口")?,
    )
    .map_err(|_| "链上中国部署信息无效")?;
    let url =
        reqwest::Url::parse(&validate_base_url(&value.url)?).map_err(|_| "链上中国入口无效")?;
    if description["id"] != "platform"
        || url.host_str() != Some(value.host.as_str())
        || url.port_or_known_default() != Some(value.port)
        || url.path() != "/"
        || value.port == 0
    {
        return Err("链上中国部署信息与 HTTPS 入口不一致".into());
    }
    value.url = url.to_string();
    Ok(value)
}
/// 从随包宿主读取并缓存公开部署说明；各节点功能共用同一份结果。
pub fn deployment(app: &AppHandle) -> Result<&'static PlatformDeployment, String> {
    if let Some(value) = ONCHINA_DEPLOYMENT.get() {
        return Ok(value);
    }
    let binary = onchina_binary_path(app).ok_or("未找到链上中国平台二进制")?;
    let output = Command::new(binary)
        .arg("--describe")
        .output()
        .map_err(|_| "读取链上中国公开部署说明失败")?;
    if !output.status.success() || output.stdout.len() > 64 * 1024 {
        return Err("链上中国公开部署说明读取失败".into());
    }
    let value = parse_deployment(&output.stdout)?;
    let _ = ONCHINA_DEPLOYMENT.set(value);
    ONCHINA_DEPLOYMENT
        .get()
        .ok_or_else(|| "链上中国部署信息尚未就绪".into())
}
/// 只返回已读取的正式入口，缺省为空，不提供另一地址回退。
pub fn platform_url() -> String {
    ONCHINA_DEPLOYMENT
        .get()
        .map(|value| value.url.clone())
        .unwrap_or_default()
}

/// 解析 onchina 二进制路径:优先随包 `resources/onchina-bin/`(打包形态),
/// 兜底节点可执行文件同目录(开发期 `cargo build` 后的 `target/<profile>/onchina`)。
pub fn onchina_binary_path(app: &AppHandle) -> Option<PathBuf> {
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

#[cfg(test)]
#[path = "../tests/unit.rs"]
mod tests;
