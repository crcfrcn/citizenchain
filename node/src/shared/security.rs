// 共享安全基础设施：应用数据目录、原子写盘、审计日志。
use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};
const AUDIT_LOG_FILE_NAME: &str = "security-audit.log";
const AUDIT_LOG_MAX_BYTES: u64 = 5 * 1024 * 1024;
const AUDIT_LOG_MAX_BACKUPS: usize = 5;
const DATA_PROFILE_ENV: &str = "CITIZENCHAIN_DATA_PROFILE";
const PROD_APP_DATA_DIR_NAME: &str = "gmb";
const DEV_APP_DATA_DIR_NAME: &str = "gmb.dev";
static AUDIT_LOG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// 对路径做脱敏处理：仅保留文件名，去除父目录信息。
/// 用于返回给前端的错误消息，避免泄露服务器/本地文件系统布局。
pub(crate) fn sanitize_path(path: &Path) -> String {
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

pub(crate) fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
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

pub(crate) fn write_text_atomic(path: &Path, content: &str) -> Result<(), String> {
    write_bytes_atomic(path, content.as_bytes(), false)
}

pub(crate) fn write_secret_text_atomic(path: &Path, content: &str) -> Result<(), String> {
    write_bytes_atomic(path, content.as_bytes(), true)
}

pub(crate) fn write_secret_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    write_bytes_atomic(path, bytes, true)
}

/// 对非密钥但仍需限制读取权限的文件（如缓存、速率限制状态）使用受限写入。
pub(crate) fn write_text_atomic_restricted(path: &Path, content: &str) -> Result<(), String> {
    write_bytes_atomic(path, content.as_bytes(), true)
}

// 所有本地状态/密钥文件统一走“临时文件 -> fsync -> rename”路径，
// 避免掉电或异常退出时把半写入内容留给下次启动读取。
fn write_bytes_atomic(path: &Path, bytes: &[u8], secret_mode: bool) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Err(format!(
            "atomic write failed: no parent for {}",
            path.display()
        ));
    };
    fs::create_dir_all(parent)
        .map_err(|e| format!("create parent dir failed ({}): {e}", parent.display()))?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_nanos())
        .unwrap_or(0);
    let random_suffix = rand::random::<u64>();
    let file_name = path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("atomic-data");
    let temp_path = parent.join(format!(
        ".{file_name}.tmp-{}-{stamp}-{random_suffix}",
        std::process::id()
    ));

    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp_path)
        .map_err(|e| format!("create temp file failed ({}): {e}", temp_path.display()))?;

    #[cfg(unix)]
    if secret_mode {
        use std::os::unix::fs::PermissionsExt;
        // 通过已打开的 File 对象设置权限（fchmod），消除路径级 set_permissions 的 TOCTOU 窗口。
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|e| {
                format!(
                    "set temp file permission failed ({}): {e}",
                    temp_path.display()
                )
            })?;
    }

    #[cfg(target_os = "windows")]
    if secret_mode {
        set_windows_acl_owner_only(&temp_path)
            .map_err(|e| format!("set temp file ACL failed ({}): {e}", temp_path.display()))?;
    }

    file.write_all(bytes)
        .map_err(|e| format!("write temp file failed ({}): {e}", temp_path.display()))?;
    file.sync_all()
        .map_err(|e| format!("sync temp file failed ({}): {e}", temp_path.display()))?;
    drop(file);

    #[cfg(target_os = "windows")]
    if let Err(e) = replace_file_windows(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(e);
    }

    #[cfg(not(target_os = "windows"))]
    if let Err(e) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(format!(
            "rename temp file failed ({} -> {}): {e}",
            temp_path.display(),
            path.display()
        ));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn replace_file_windows(from: &Path, to: &Path) -> Result<(), String> {
    use std::{ffi::OsStr, iter::once, os::windows::ffi::OsStrExt};

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(
            lp_existing_file_name: *const u16,
            lp_new_file_name: *const u16,
            dw_flags: u32,
        ) -> i32;
    }

    fn wide_null(input: &OsStr) -> Vec<u16> {
        input.encode_wide().chain(once(0)).collect()
    }

    let from_w = wide_null(from.as_os_str());
    let to_w = wide_null(to.as_os_str());
    let ok = unsafe {
        MoveFileExW(
            from_w.as_ptr(),
            to_w.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        return Err(format!(
            "replace target file failed ({} -> {}): {}",
            from.display(),
            to.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

/// Windows: 通过 DACL 将文件访问限制为仅当前用户（等效 Unix 0600）。
/// 获取当前进程 token 中的用户 SID，构建仅含该 SID 的 DACL 并应用到文件。
#[cfg(target_os = "windows")]
fn set_windows_acl_owner_only(path: &Path) -> Result<(), String> {
    use std::{ffi::OsStr, iter::once, os::windows::ffi::OsStrExt, ptr};

    type Handle = *mut std::ffi::c_void;
    type Psid = *mut std::ffi::c_void;
    type Pacl = *mut std::ffi::c_void;
    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_USER_INFO: u32 = 1; // TokenUser
    const ACL_REVISION: u32 = 2;
    const FILE_ALL_ACCESS: u32 = 0x001F01FF;
    const SE_FILE_OBJECT: u32 = 1;
    const DACL_SECURITY_INFORMATION: u32 = 0x04;
    const PROTECTED_DACL_SECURITY_INFORMATION: u32 = 0x8000_0000;

    #[repr(C)]
    struct TokenUser {
        user: SidAndAttributes,
    }
    #[repr(C)]
    struct SidAndAttributes {
        sid: Psid,
        attributes: u32,
    }

    #[link(name = "Advapi32")]
    unsafe extern "system" {
        fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
        fn GetTokenInformation(
            token: Handle,
            class: u32,
            info: *mut u8,
            len: u32,
            ret_len: *mut u32,
        ) -> i32;
        fn GetLengthSid(sid: Psid) -> u32;
        fn InitializeAcl(acl: *mut u8, len: u32, revision: u32) -> i32;
        fn AddAccessAllowedAce(acl: *mut u8, revision: u32, mask: u32, sid: Psid) -> i32;
        fn SetNamedSecurityInfoW(
            name: *const u16,
            object_type: u32,
            info: u32,
            owner: Psid,
            group: Psid,
            dacl: *const u8,
            sacl: *const u8,
        ) -> u32;
    }
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> Handle;
        fn CloseHandle(h: Handle) -> i32;
    }

    fn wide_null(input: &OsStr) -> Vec<u16> {
        input.encode_wide().chain(once(0)).collect()
    }

    unsafe {
        // 1. 获取当前进程 token
        let mut token: Handle = ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(format!(
                "OpenProcessToken failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        // 2. 获取 token 中的用户 SID
        let mut needed: u32 = 0;
        GetTokenInformation(token, TOKEN_USER_INFO, ptr::null_mut(), 0, &mut needed);
        let mut buf = vec![0u8; needed as usize];
        if GetTokenInformation(
            token,
            TOKEN_USER_INFO,
            buf.as_mut_ptr(),
            needed,
            &mut needed,
        ) == 0
        {
            CloseHandle(token);
            return Err(format!(
                "GetTokenInformation failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        let token_user = &*(buf.as_ptr() as *const TokenUser);
        let sid = token_user.user.sid;

        // 3. 构建仅含当前用户 FILE_ALL_ACCESS 的 ACL
        let sid_len = GetLengthSid(sid);
        // ACL header (8 bytes) + ACE header (8 bytes) + SID
        let acl_size = 8 + 8 + sid_len;
        let mut acl_buf = vec![0u8; acl_size as usize];
        if InitializeAcl(acl_buf.as_mut_ptr(), acl_size, ACL_REVISION) == 0 {
            CloseHandle(token);
            return Err(format!(
                "InitializeAcl failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        if AddAccessAllowedAce(acl_buf.as_mut_ptr(), ACL_REVISION, FILE_ALL_ACCESS, sid) == 0 {
            CloseHandle(token);
            return Err(format!(
                "AddAccessAllowedAce failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        // 4. 应用 DACL 到文件，PROTECTED 防止从父目录继承
        let path_w = wide_null(path.as_os_str());
        let rc = SetNamedSecurityInfoW(
            path_w.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            acl_buf.as_ptr(),
            ptr::null(),
        );
        CloseHandle(token);
        if rc != 0 {
            return Err(format!("SetNamedSecurityInfoW failed: error code {rc}"));
        }
    }
    Ok(())
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

pub(crate) fn append_audit_log(app: &AppHandle, action: &str, status: &str) -> Result<(), String> {
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

pub(crate) fn ensure_unlock_password(password: &str) -> Result<&str, String> {
    let trimmed = password.trim();
    if trimmed.is_empty() {
        return Err("设备开机密码不能为空".to_string());
    }
    Ok(trimmed)
}
