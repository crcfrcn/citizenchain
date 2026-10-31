//! 节点各功能只读取宿主公开的部署说明；不复制固定地址、不设环境回退或私有 CA。
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};
use tauri::{AppHandle, Manager};

/// 部署信息由产品宿主公开说明提供，节点不复制域名、地址或端口常量。
#[derive(Clone, Debug, Deserialize)]
pub struct PlatformDeployment {
    /// TLS 主机名；连接地址单独使用公开声明的局域网 IP。
    pub host: String,
    /// 产品声明的固定监听地址。
    pub address: std::net::SocketAddr,
    /// 经过 HTTPS 与端点一致性验证的浏览器入口。
    pub url: String,
}
static ONCHINA_DEPLOYMENT: OnceLock<PlatformDeployment> = OnceLock::new();
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
        || url.port_or_known_default() != Some(value.address.port())
        || url.path() != "/"
        || !matches!(value.address.ip(), std::net::IpAddr::V4(ip) if ip.is_private())
        || value.address.port() == 0
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
