use tauri::Manager;

/// 本机正式构建只消费公开证书；不导出私钥，也不改变其它平台的启动合同。
#[cfg(target_os = "macos")]
const RPC_CERTIFICATE_FILE: &str = "rpc-certificate.hex";

#[cfg(target_os = "macos")]
fn check_rpc_certificate_acl(output: &str, private: bool) -> Result<(), String> {
    let mut lines = output.lines();
    if lines.next().is_none_or(|line| line.is_empty()) {
        return Err("公开证书路径 ACL 无法确认".into());
    }
    for line in lines {
        let (index, entry) = line
            .trim()
            .split_once(": ")
            .ok_or("公开证书路径 ACL 格式无效")?;
        if index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()) || private {
            return Err("公开证书目录或文件存在附加 ACL".into());
        }
        let (identity, rights) = entry.rsplit_once(' ').ok_or("公开证书路径 ACL 无法确认")?;
        let (principal, decision) = identity
            .rsplit_once(' ')
            .ok_or("公开证书路径 ACL 无法确认")?;
        if !(principal.starts_with("user:") || principal.starts_with("group:")) {
            return Err("公开证书路径 ACL 身份无效".into());
        }
        if decision == "deny" {
            continue;
        }
        if decision != "allow"
            || !rights.split(',').all(|right| {
                matches!(
                    right.trim(),
                    "read"
                        | "list"
                        | "execute"
                        | "search"
                        | "readattr"
                        | "readextattr"
                        | "readsecurity"
                        | "synchronize"
                )
            })
        {
            return Err("公开证书祖先目录允许额外写入".into());
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn inspect_rpc_certificate_acl(path: &std::path::Path, private: bool) -> Result<(), String> {
    // 使用系统固定工具且不经过 shell；不修复 ACL、不将工具输出写入日志。
    let output = std::process::Command::new("/bin/ls")
        .arg("-lde")
        .arg(path)
        .env_clear()
        .env("LC_ALL", "C")
        .output()
        .map_err(|_| "无法检查公开证书 ACL")?;
    if !output.status.success() || output.stdout.len() > 64 * 1024 {
        return Err("无法检查公开证书 ACL".into());
    }
    check_rpc_certificate_acl(
        std::str::from_utf8(&output.stdout).map_err(|_| "公开证书 ACL 编码无效")?,
        private,
    )
}

#[cfg(target_os = "macos")]
fn check_rpc_certificate_directory(directory: &std::path::Path) -> Result<(u64, u64), String> {
    check_rpc_certificate_path(directory, true)
}

#[cfg(target_os = "macos")]
fn check_rpc_certificate_path(
    directory: &std::path::Path,
    private: bool,
) -> Result<(u64, u64), String> {
    use std::os::unix::fs::MetadataExt;
    let uid = unsafe { libc::geteuid() };
    if !directory.is_absolute()
        || directory.canonicalize().map_err(|_| "公开证书目录不可用")? != directory
    {
        return Err("公开证书目录不能经过链接".into());
    }
    for path in directory.ancestors() {
        let metadata = std::fs::symlink_metadata(path).map_err(|_| "公开证书目录不可用")?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || ![0, uid].contains(&metadata.uid())
            || metadata.mode() & 0o022 != 0
            || private
                && path == directory
                && (metadata.uid() != uid || metadata.mode() & 0o7777 != 0o700)
        {
            return Err("公开证书目录所有者或权限无效".into());
        }
        inspect_rpc_certificate_acl(path, private && path == directory)?;
    }
    let metadata = std::fs::symlink_metadata(directory).map_err(|_| "公开证书目录不可用")?;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(target_os = "macos")]
fn formal_rpc_certificate_profile(profile: Option<&str>, debug: bool) -> Result<bool, String> {
    // 仅选择既有正式命名空间的交付，不更改节点自己的数据目录规则。
    match profile.map(str::trim) {
        Some("prod" | "production" | "release") => Ok(true),
        Some("dev" | "development" | "debug") => Ok(false),
        None | Some("") => Ok(!debug),
        Some(_) => Err("公开证书数据命名空间无效".into()),
    }
}

#[cfg(target_os = "macos")]
pub fn publish_desktop_rpc_certificate(app: &tauri::AppHandle) -> Result<(), String> {
    let profile = std::env::var("CITIZENCHAIN_DATA_PROFILE").ok();
    if !formal_rpc_certificate_profile(profile.as_deref(), cfg!(debug_assertions))? {
        return Ok(());
    }
    let configured = app
        .path()
        .app_data_dir()
        .map_err(|_| "无法解析公开证书目录")?;
    let directory = configured.parent().ok_or("公开证书目录无父级")?.join("gmb");
    // 在既有目录工具创建/设权之前检查，避免顺着异常链接修改其它位置的权限。
    match std::fs::symlink_metadata(&directory) {
        Ok(_) => {
            check_rpc_certificate_directory(&directory)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            check_rpc_certificate_path(directory.parent().ok_or("公开证书目录无父级")?, false)?;
        }
        Err(_) => return Err("公开证书目录不可用".into()),
    }
    if crate::security::app_data_dir(app)? != directory {
        return Err("公开证书目录与正式节点不一致".into());
    }
    let certificate = crate::tls::desktop_rpc_certificate().ok_or("桌面 RPC 公开证书尚未准备")?;
    publish_rpc_certificate(&directory, certificate)
}

#[cfg(target_os = "macos")]
fn check_rpc_certificate_file(metadata: &std::fs::Metadata) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o7777 != 0o600
        || metadata.nlink() != 1
        || metadata.len() == 0
        || metadata.len() > 64 * 1024
    {
        return Err("公开证书文件类型、所有者或权限无效".into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn publish_rpc_certificate(directory: &std::path::Path, certificate: &[u8]) -> Result<(), String> {
    publish_rpc_certificate_using(
        directory,
        certificate,
        crate::security::write_text_atomic_restricted,
    )
}

#[cfg(target_os = "macos")]
fn publish_rpc_certificate_using(
    directory: &std::path::Path,
    certificate: &[u8],
    write: impl FnOnce(&std::path::Path, &str) -> Result<(), String>,
) -> Result<(), String> {
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    if certificate.is_empty() || certificate.len() > 32 * 1024 {
        return Err("公开证书大小无效".into());
    }
    let identity = check_rpc_certificate_directory(directory)?;
    let path = directory.join(RPC_CERTIFICATE_FILE);
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) => {
            check_rpc_certificate_file(&metadata)?;
            inspect_rpc_certificate_acl(&path, true)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("公开证书文件不可用".into()),
    }
    let text = hex::encode(certificate);
    write(&path, &text)?;
    // 写后仍按句柄核对，避免把路径检查与实际读取分离；同用户恶意进程不在此边界内。
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&path)
        .map_err(|_| "公开证书回读失败")?;
    let metadata = file.metadata().map_err(|_| "公开证书属性读取失败")?;
    check_rpc_certificate_file(&metadata)?;
    inspect_rpc_certificate_acl(&path, true)?;
    let mut actual = Vec::new();
    file.by_ref()
        .take(64 * 1024 + 1)
        .read_to_end(&mut actual)
        .map_err(|_| "公开证书回读失败")?;
    let current = std::fs::symlink_metadata(&path).map_err(|_| "公开证书回读失败")?;
    check_rpc_certificate_file(&current)?;
    if identity != check_rpc_certificate_directory(directory)?
        || (metadata.dev(), metadata.ino()) != (current.dev(), current.ino())
        || actual != text.as_bytes()
    {
        return Err("公开证书交付期间文件已变化".into());
    }
    Ok(())
}
#[cfg(all(test, target_os = "macos"))]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/publication.rs"]
mod certificate_tests;
