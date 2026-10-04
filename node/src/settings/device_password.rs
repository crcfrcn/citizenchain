// 统一封装“设备开机密码校验”能力，供 settings/home 多处敏感操作复用。
#![allow(unsafe_code)]
// 本模块需要调用 libc/PAM 做本机账户校验，unsafe 边界集中在平台 FFI 封装内。
use crate::shared::security;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::SystemTime,
};
use tauri::AppHandle;

const AUTH_FAIL_WINDOW_SECS: u64 = 300;
const AUTH_MAX_FAILURES_IN_WINDOW: usize = 5;
const AUTH_BACKOFF_MS: u64 = 800;
const AUTH_BACKOFF_MAX_MS: u64 = 5000;
const AUTH_RATE_LIMIT_FILE_NAME: &str = "auth-rate-limit.json";

static AUTH_RATE_LIMIT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct AuthRateLimitState {
    recent_failures: VecDeque<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct AuthRateLimitStore {
    #[serde(default)]
    accounts: HashMap<String, AuthRateLimitState>,
}

// 账户名只接受可展示的普通文本，避免控制字符进入命令参数、日志或限速键名。
fn validate_system_username(user: &str) -> Result<&str, String> {
    let trimmed = user.trim();
    if trimmed.is_empty() {
        return Err("系统用户名为空".to_string());
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err("系统用户名包含控制字符".to_string());
    }
    Ok(trimmed)
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|v| v.as_secs())
        .unwrap_or(0)
}

fn auth_rate_limit_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(security::app_data_dir(app)?.join(AUTH_RATE_LIMIT_FILE_NAME))
}

// 设备密码尝试次数与业务配置分开落盘，便于单独清理和审计。
fn load_auth_rate_limit_store(path: &Path) -> Result<AuthRateLimitStore, String> {
    let raw = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(AuthRateLimitStore::default()),
        Err(err) => {
            return Err(format!(
                "read auth rate limit store failed ({}): {err}",
                security::sanitize_path(path)
            ));
        }
    };
    serde_json::from_str::<AuthRateLimitStore>(&raw).map_err(|err| {
        format!(
            "parse auth rate limit store failed ({}): {err}",
            security::sanitize_path(path)
        )
    })
}

fn save_auth_rate_limit_store(path: &Path, store: &AuthRateLimitStore) -> Result<(), String> {
    let raw = serde_json::to_string(store)
        .map_err(|err| format!("encode auth rate limit store failed: {err}"))?;
    security::write_text_atomic_restricted(path, &format!("{raw}\n"))
}

fn compact_auth_rate_limit_state(
    store: &mut AuthRateLimitStore,
    account: &str,
    now: u64,
) -> Option<usize> {
    let state = store.accounts.get_mut(account)?;
    while state
        .recent_failures
        .front()
        .is_some_and(|ts| now.saturating_sub(*ts) > AUTH_FAIL_WINDOW_SECS)
    {
        let _ = state.recent_failures.pop_front();
    }
    let len = state.recent_failures.len();
    if len == 0 {
        let _ = store.accounts.remove(account);
        return Some(0);
    }
    Some(len)
}

fn enforce_auth_rate_limit(app: &AppHandle, account: &str) -> Result<(), String> {
    let lock = AUTH_RATE_LIMIT_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock
        .lock()
        .map_err(|_| "设备密码限速器状态异常".to_string())?;
    let path = auth_rate_limit_path(app)?;
    let mut store = load_auth_rate_limit_store(&path)?;
    let now = now_unix_secs();
    let before = store
        .accounts
        .get(account)
        .map(|state| state.recent_failures.len())
        .unwrap_or(0);
    let count = compact_auth_rate_limit_state(&mut store, account, now).unwrap_or(0);
    if count != before {
        save_auth_rate_limit_store(&path, &store)?;
    }
    if count >= AUTH_MAX_FAILURES_IN_WINDOW {
        return Err("设备密码尝试次数过多，请稍后再试".to_string());
    }
    Ok(())
}

// 认证失败的退避在锁外执行，避免单次慢请求把后续所有认证线程一起阻塞住。
fn record_auth_attempt(app: &AppHandle, account: &str, success: bool) -> Result<(), String> {
    let lock = AUTH_RATE_LIMIT_LOCK.get_or_init(|| Mutex::new(()));
    let guard = lock
        .lock()
        .map_err(|_| "设备密码限速器状态异常".to_string())?;
    let path = auth_rate_limit_path(app)?;
    let mut store = load_auth_rate_limit_store(&path)?;
    let mut sleep_delay_ms: u64 = 0;
    let now = now_unix_secs();
    let _ = compact_auth_rate_limit_state(&mut store, account, now);
    let state = store.accounts.entry(account.to_string()).or_default();
    if success {
        state.recent_failures.clear();
    } else {
        state.recent_failures.push_back(now);
        let over = state
            .recent_failures
            .len()
            .saturating_sub(AUTH_MAX_FAILURES_IN_WINDOW.saturating_sub(1));
        if over > 0 {
            let mut delay = AUTH_BACKOFF_MS.saturating_mul(over as u64);
            if delay > AUTH_BACKOFF_MAX_MS {
                delay = AUTH_BACKOFF_MAX_MS;
            }
            sleep_delay_ms = delay;
        }
    }
    if state.recent_failures.is_empty() {
        let _ = store.accounts.remove(account);
    }
    save_auth_rate_limit_store(&path, &store)?;
    drop(guard);
    if sleep_delay_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(sleep_delay_ms));
    }
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn current_unix_username() -> Result<String, String> {
    use std::{ffi::CStr, mem, ptr};

    // 直接根据当前有效 uid 反查系统账户，避免依赖 USER 环境变量。
    let uid = unsafe { libc::geteuid() };
    let mut pwd: libc::passwd = unsafe { mem::zeroed() };
    let mut result: *mut libc::passwd = ptr::null_mut();
    let suggested = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
    let buf_len = if suggested > 0 {
        suggested as usize
    } else {
        16 * 1024
    };
    let mut buf = vec![0u8; buf_len];

    let rc = unsafe {
        libc::getpwuid_r(
            uid,
            &mut pwd,
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            &mut result,
        )
    };
    if rc != 0 || result.is_null() || pwd.pw_name.is_null() {
        return Err(format!("读取当前系统用户失败: errno={rc}"));
    }

    let username = unsafe { CStr::from_ptr(pwd.pw_name) }
        .to_str()
        .map_err(|_| "当前系统用户名格式无效".to_string())?
        .to_string();
    Ok(validate_system_username(&username)?.to_string())
}

// macOS 和 Linux 统一使用 PAM 校验设备登录密码，避免 macOS dscl 命令将明文密码暴露在进程参数中。
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn verify_device_login_password(app: &AppHandle, password: &str) -> Result<(), String> {
    unix_pam_auth::verify_with_pam(app, password)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod unix_pam_auth {
    use super::current_unix_username;
    use std::{
        ffi::{c_char, c_int, c_void, CString},
        ptr,
    };
    use zeroize::Zeroizing;

    const PAM_SUCCESS: c_int = 0;
    const PAM_BUF_ERR: c_int = 5;
    const PAM_CONV_ERR: c_int = 19;
    const PAM_PROMPT_ECHO_OFF: c_int = 1;
    const PAM_PROMPT_ECHO_ON: c_int = 2;
    const PAM_ERROR_MSG: c_int = 3;
    const PAM_TEXT_INFO: c_int = 4;

    #[repr(C)]
    struct PamMessage {
        msg_style: c_int,
        msg: *const c_char,
    }

    #[repr(C)]
    struct PamResponse {
        resp: *mut c_char,
        resp_retcode: c_int,
    }

    #[repr(C)]
    struct PamConv {
        conv: Option<
            unsafe extern "C" fn(
                num_msg: c_int,
                msg: *mut *const PamMessage,
                resp: *mut *mut PamResponse,
                appdata_ptr: *mut c_void,
            ) -> c_int,
        >,
        appdata_ptr: *mut c_void,
    }

    #[repr(C)]
    struct PamHandle {
        _private: [u8; 0],
    }

    #[link(name = "pam")]
    unsafe extern "C" {
        fn pam_start(
            service_name: *const c_char,
            user: *const c_char,
            pam_conv: *const PamConv,
            pamh: *mut *mut PamHandle,
        ) -> c_int;
        fn pam_end(pamh: *mut PamHandle, pam_status: c_int) -> c_int;
        fn pam_authenticate(pamh: *mut PamHandle, flags: c_int) -> c_int;
        fn pam_acct_mgmt(pamh: *mut PamHandle, flags: c_int) -> c_int;
    }

    unsafe fn free_responses(responses: *mut PamResponse, filled: usize) {
        for idx in 0..filled {
            let response = responses.add(idx);
            if !(*response).resp.is_null() {
                let len = libc::strlen((*response).resp);
                if len > 0 {
                    // `c_char` 在 Rust target `aarch64-unknown-linux-gnu` 上为 `u8`，在
                    // `x86_64-unknown-linux-gnu` 上为 `i8`；直接按原指针元素清零可避免
                    // 同类型裸指针转换触发特定 target 的 Clippy。
                    ptr::write_bytes((*response).resp, 0, len);
                }
                libc::free((*response).resp as *mut c_void);
                (*response).resp = ptr::null_mut();
            }
        }
        libc::free(responses as *mut c_void);
    }

    unsafe extern "C" fn conversation(
        num_msg: c_int,
        msg: *mut *const PamMessage,
        resp: *mut *mut PamResponse,
        appdata_ptr: *mut c_void,
    ) -> c_int {
        if num_msg <= 0 || msg.is_null() || resp.is_null() || appdata_ptr.is_null() {
            return PAM_CONV_ERR;
        }

        let responses =
            libc::calloc(num_msg as usize, std::mem::size_of::<PamResponse>()) as *mut PamResponse;
        if responses.is_null() {
            return PAM_BUF_ERR;
        }

        let password_ptr = appdata_ptr as *const c_char;
        for idx in 0..num_msg as usize {
            let msg_ptr = *msg.add(idx);
            if msg_ptr.is_null() {
                free_responses(responses, idx);
                return PAM_CONV_ERR;
            }
            match (*msg_ptr).msg_style {
                PAM_PROMPT_ECHO_OFF | PAM_PROMPT_ECHO_ON => {
                    let dup = libc::strdup(password_ptr);
                    if dup.is_null() {
                        free_responses(responses, idx);
                        return PAM_BUF_ERR;
                    }
                    (*responses.add(idx)).resp = dup;
                    (*responses.add(idx)).resp_retcode = 0;
                }
                PAM_ERROR_MSG | PAM_TEXT_INFO => {}
                _ => {
                    free_responses(responses, idx);
                    return PAM_CONV_ERR;
                }
            }
        }

        *resp = responses;
        PAM_SUCCESS
    }

    pub(super) fn verify_with_pam(app: &super::AppHandle, password: &str) -> Result<(), String> {
        let user = current_unix_username()?;
        super::enforce_auth_rate_limit(app, &user)?;
        let user_c =
            CString::new(user.as_str()).map_err(|_| "系统用户名包含非法字符".to_string())?;
        let pass_raw = Zeroizing::new(
            CString::new(password)
                .map_err(|_| "密码包含非法字符".to_string())?
                .into_bytes_with_nul(),
        );
        // PAM 回调从这块缓冲区读取密码，Zeroizing 在 drop 时自动清零。
        let conv = PamConv {
            conv: Some(conversation),
            appdata_ptr: pass_raw.as_ptr() as *mut c_void,
        };
        let mut success = false;

        // macOS PAM 服务配置位于 /etc/pam.d/authorization 和 /etc/pam.d/login
        #[cfg(target_os = "macos")]
        let services: &[&str] = &["authorization", "login"];
        #[cfg(target_os = "linux")]
        let services: &[&str] = &["login", "system-auth", "common-auth"];
        for &service in services {
            let service_c = CString::new(service).map_err(|_| "PAM 服务名非法".to_string())?;
            let mut handle: *mut PamHandle = ptr::null_mut();
            let start_status = unsafe {
                pam_start(
                    service_c.as_ptr(),
                    user_c.as_ptr(),
                    &conv,
                    &mut handle as *mut *mut PamHandle,
                )
            };
            if start_status != PAM_SUCCESS {
                continue;
            }

            let auth_status = unsafe { pam_authenticate(handle, 0) };
            if auth_status != PAM_SUCCESS {
                unsafe {
                    pam_end(handle, auth_status);
                }
                continue;
            }

            let acct_status = unsafe { pam_acct_mgmt(handle, 0) };
            unsafe {
                pam_end(handle, acct_status);
            }
            if acct_status == PAM_SUCCESS {
                success = true;
                break;
            }
        }

        drop(pass_raw);
        super::record_auth_attempt(app, &user, success)?;
        if success {
            Ok(())
        } else {
            Err("设备开机密码错误".to_string())
        }
    }
}

#[cfg(target_os = "windows")]
fn current_windows_account() -> Result<(String, String), String> {
    type Bool = i32;
    type Dword = u32;
    const NAME_SAM_COMPATIBLE: Dword = 2;
    const ERROR_MORE_DATA: i32 = 234;

    #[link(name = "Secur32")]
    unsafe extern "system" {
        fn GetUserNameExW(name_format: Dword, name_buffer: *mut u16, n_size: *mut Dword) -> Bool;
    }

    #[link(name = "Advapi32")]
    unsafe extern "system" {
        fn GetUserNameW(buffer: *mut u16, size: *mut Dword) -> Bool;
    }

    fn wide_to_string(buf: &[u16], len: usize) -> Result<String, String> {
        String::from_utf16(&buf[..len]).map_err(|_| "系统用户名编码无效".to_string())
    }

    // 优先取 DOMAIN\USER 形式，方便后续直接传给 LogonUserW。
    let mut len: Dword = 0;
    unsafe {
        let _ = GetUserNameExW(NAME_SAM_COMPATIBLE, std::ptr::null_mut(), &mut len);
    }
    if len > 0 {
        let mut buf = vec![0u16; len as usize];
        let ok = unsafe { GetUserNameExW(NAME_SAM_COMPATIBLE, buf.as_mut_ptr(), &mut len) };
        if ok != 0 {
            let raw = wide_to_string(&buf, len as usize)?
                .trim_end_matches('\0')
                .to_string();
            if let Some((domain, user)) = raw.rsplit_once('\\') {
                return Ok((
                    validate_system_username(user)?.to_string(),
                    domain.to_string(),
                ));
            }
            return Ok((validate_system_username(&raw)?.to_string(), ".".to_string()));
        }
    }

    let mut size: Dword = 256;
    loop {
        let mut buf = vec![0u16; size as usize];
        let ok = unsafe { GetUserNameW(buf.as_mut_ptr(), &mut size) };
        if ok != 0 {
            let raw = wide_to_string(&buf, size as usize)?
                .trim_end_matches('\0')
                .to_string();
            return Ok((validate_system_username(&raw)?.to_string(), ".".to_string()));
        }
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(ERROR_MORE_DATA) {
            size = size.saturating_mul(2);
            continue;
        }
        return Err(format!("读取当前系统用户失败: {err}"));
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn verify_device_login_password(app: &AppHandle, password: &str) -> Result<(), String> {
    type Handle = isize;
    type Bool = i32;
    type Dword = u32;

    const LOGON32_LOGON_INTERACTIVE: Dword = 2;
    const LOGON32_PROVIDER_DEFAULT: Dword = 0;

    #[link(name = "Advapi32")]
    unsafe extern "system" {
        fn LogonUserW(
            user_name: *const u16,
            domain: *const u16,
            password: *const u16,
            logon_type: Dword,
            logon_provider: Dword,
            token: *mut Handle,
        ) -> Bool;
    }

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn CloseHandle(handle: Handle) -> Bool;
    }

    use zeroize::Zeroizing;

    fn wide_null(input: &str) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        std::ffi::OsStr::new(input)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    // 优先通过系统 token API 解析当前账户，再做 LogonUserW 校验，避免依赖 USERNAME/USERDOMAIN 环境变量。
    let (username, domain) = current_windows_account()?;
    enforce_auth_rate_limit(app, &username)?;
    let username_w = wide_null(&username);
    let domain_w = wide_null(&domain);
    let password_w = Zeroizing::new(wide_null(password));

    let mut token: Handle = 0;
    let ok = unsafe {
        LogonUserW(
            username_w.as_ptr(),
            domain_w.as_ptr(),
            password_w.as_ptr(),
            LOGON32_LOGON_INTERACTIVE,
            LOGON32_PROVIDER_DEFAULT,
            &mut token as *mut Handle,
        )
    };
    if ok != 0 {
        record_auth_attempt(app, &username, true)?;
        unsafe {
            let _ = CloseHandle(token);
        }
        return Ok(());
    }

    let dot_w = wide_null(".");
    let mut token_fallback: Handle = 0;
    let ok_fallback = unsafe {
        LogonUserW(
            username_w.as_ptr(),
            dot_w.as_ptr(),
            password_w.as_ptr(),
            LOGON32_LOGON_INTERACTIVE,
            LOGON32_PROVIDER_DEFAULT,
            &mut token_fallback as *mut Handle,
        )
    };
    if ok_fallback != 0 {
        record_auth_attempt(app, &username, true)?;
        unsafe {
            let _ = CloseHandle(token_fallback);
        }
        return Ok(());
    }

    record_auth_attempt(app, &username, false)?;
    Err("设备开机密码错误".to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(crate) fn verify_device_login_password(
    _app: &AppHandle,
    _password: &str,
) -> Result<(), String> {
    Err("当前操作系统暂不支持设备开机密码校验".to_string())
}
