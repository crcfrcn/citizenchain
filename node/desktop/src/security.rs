//! A collection of node-specific RPC methods.
//! Substrate provides the `sc-rpc` crate, which defines the core RPC layer
//! used by Substrate nodes. This file extends those RPC definitions with
//! capabilities that are specific to this project's runtime configuration.

#![warn(missing_docs)]
use std::fs;

use sp_keystore::Keystore;

use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use tauri::AppHandle;
use tauri::Manager;

static MINER_TRANSFER_TOKENS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// 签发一次性矿工热钱包转账令牌。
///
/// 令牌只在当前进程内保存，供 Tauri 命令在完成设备密码校验后调用本机 RPC。
pub fn issue_miner_transfer_token() -> Result<String, String> {
    let token = hex::encode(rand::random::<[u8; 32]>());
    let tokens = MINER_TRANSFER_TOKENS.get_or_init(|| Mutex::new(HashSet::new()));
    let mut guard = tokens
        .lock()
        .map_err(|_| "矿工热钱包令牌状态异常".to_string())?;
    guard.insert(token.clone());
    Ok(token)
}

/// 回收尚未被 RPC 消费的一次性矿工热钱包转账令牌。
pub fn revoke_miner_transfer_token(token: &str) {
    let Some(tokens) = MINER_TRANSFER_TOKENS.get() else {
        return;
    };
    if let Ok(mut guard) = tokens.lock() {
        guard.remove(token);
    }
}

pub fn consume_miner_transfer_token(token: &str) -> bool {
    let Some(tokens) = MINER_TRANSFER_TOKENS.get() else {
        return false;
    };
    tokens
        .lock()
        .map(|mut guard| guard.remove(token))
        .unwrap_or(false)
}

const AUDIT_LOG_FILE_NAME: &str = "security-audit.log";

const AUDIT_LOG_MAX_BYTES: u64 = 5 * 1024 * 1024;

const AUDIT_LOG_MAX_BACKUPS: usize = 5;

const DATA_PROFILE_ENV: &str = "CITIZENCHAIN_DATA_PROFILE";

const PROD_APP_DATA_DIR_NAME: &str = "gmb";

const DEV_APP_DATA_DIR_NAME: &str = "gmb.dev";

static AUDIT_LOG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// 对路径做脱敏处理：仅保留文件名，去除父目录信息。
/// 用于返回给前端的错误消息，避免泄露服务器/本地文件系统布局。
pub fn sanitize_path(path: &Path) -> String {
    path.file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("<unknown>")
        .to_string()
}

fn app_data_dir_name() -> Result<&'static str, String> {
    let profile = std::env::var(DATA_PROFILE_ENV).ok();
    match profile.as_deref().map(str::trim) {
        Some("prod" | "production" | "release") => Ok(PROD_APP_DATA_DIR_NAME),
        Some("dev" | "development" | "debug") => Ok(DEV_APP_DATA_DIR_NAME),
        Some(other) if !other.is_empty() => Err(format!("invalid {DATA_PROFILE_ENV}: {other}")),
        _ if cfg!(debug_assertions) => Ok(DEV_APP_DATA_DIR_NAME),
        _ => Ok(PROD_APP_DATA_DIR_NAME),
    }
}

pub fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let default_app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir failed: {e}"))?;
    let app_data_parent = default_app_data.parent().ok_or_else(|| {
        format!(
            "resolve app data parent failed: {}",
            default_app_data.display()
        )
    })?;
    // Tauri identifier 只作为应用身份，正式版/开发版数据命名空间统一使用短目录。
    let app_data = app_data_parent.join(app_data_dir_name()?);
    fs::create_dir_all(&app_data).map_err(|e| format!("create app data dir failed: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&app_data, fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("set app data dir permission failed: {e}"))?;
    }
    Ok(app_data)
}

pub fn write_text_atomic(path: &Path, content: &str) -> Result<(), String> {
    private::write_bytes_atomic(path, content.as_bytes())
}

pub fn write_secret_text_atomic(path: &Path, content: &str) -> Result<(), String> {
    write_secret_bytes_atomic(path, content.as_bytes())
}

pub fn write_secret_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    private::write_secret_bytes_atomic(path, bytes)
}

/// 对非密钥但仍需限制读取权限的文件（如缓存、速率限制状态）使用受限写入。
pub fn write_text_atomic_restricted(path: &Path, content: &str) -> Result<(), String> {
    write_secret_bytes_atomic(path, content.as_bytes())
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs())
        .unwrap_or(0)
}

fn audit_log_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join(AUDIT_LOG_FILE_NAME))
}

fn audit_log_backup_path(path: &Path, index: usize) -> PathBuf {
    let mut backup = path.as_os_str().to_os_string();
    backup.push(format!(".{index}"));
    PathBuf::from(backup)
}

fn rotate_audit_log_if_needed(path: &Path) -> Result<(), String> {
    let size = match fs::metadata(path) {
        Ok(meta) => meta.len(),
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(format!(
                "read audit log metadata failed ({}): {err}",
                path.display()
            ));
        }
    };
    if size < AUDIT_LOG_MAX_BYTES {
        return Ok(());
    }

    let oldest = audit_log_backup_path(path, AUDIT_LOG_MAX_BACKUPS);
    match fs::remove_file(&oldest) {
        Ok(_) => {}
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => {
            return Err(format!(
                "remove oldest audit log backup failed ({}): {err}",
                oldest.display()
            ));
        }
    }

    for idx in (1..AUDIT_LOG_MAX_BACKUPS).rev() {
        let src = audit_log_backup_path(path, idx);
        let dst = audit_log_backup_path(path, idx + 1);
        match fs::rename(&src, &dst) {
            Ok(_) => {}
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => {
                return Err(format!(
                    "rotate audit log backup failed ({} -> {}): {err}",
                    src.display(),
                    dst.display()
                ));
            }
        }
    }

    let first = audit_log_backup_path(path, 1);
    fs::rename(path, &first).map_err(|err| {
        format!(
            "rotate current audit log failed ({} -> {}): {err}",
            path.display(),
            first.display()
        )
    })?;
    Ok(())
}

pub fn append_audit_log(app: &AppHandle, action: &str, status: &str) -> Result<(), String> {
    let path = audit_log_path(app)?;
    let lock = AUDIT_LOG_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock
        .lock()
        .map_err(|_| "audit log state poisoned".to_string())?;
    rotate_audit_log_if_needed(&path)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("open audit log failed ({}): {e}", path.display()))?;
    let ts = now_unix_secs();
    let line = format!("{ts}\taction={action}\tstatus={status}\n");
    file.write_all(line.as_bytes())
        .map_err(|e| format!("write audit log failed ({}): {e}", path.display()))
}

pub fn ensure_unlock_password(password: &str) -> Result<&str, String> {
    let trimmed = password.trim();
    if trimmed.is_empty() {
        return Err("设备开机密码不能为空".to_string());
    }
    Ok(trimmed)
}
