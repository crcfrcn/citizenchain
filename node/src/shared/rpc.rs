// 共享 RPC 请求封装，供 home/settings/mining/network 复用同一套连接池与安全限制。
use serde_json::Value;
use std::{
    env,
    io::Read,
    sync::{Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

pub(crate) const DEFAULT_LOCAL_RPC_PORT: u16 = 9944;
const LOCAL_RPC_PORT_ENV: &str = "CITIZENCHAIN_RPC_PORT";
const RPC_CONNECT_TIMEOUT_MS: u64 = 2500;
/// 各模块 RPC 请求统一超时，避免分散定义导致不一致。
pub(crate) const RPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const GENESIS_HASH_TIMEOUT: Duration = Duration::from_secs(3);
const GENESIS_HASH_MAX_BYTES: u64 = 1024;
const RPC_READY_POLL_INTERVAL: Duration = Duration::from_secs(2);

static RPC_HTTP_CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
static RPC_HTTP_CLIENT_INIT_LOCK: Mutex<()> = Mutex::new(());
static CACHED_GENESIS_HASH: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static LOCAL_RPC_PORT: OnceLock<Mutex<u16>> = OnceLock::new();

fn initial_rpc_port() -> u16 {
    match env::var(LOCAL_RPC_PORT_ENV)
        .ok()
        .and_then(|raw| raw.trim().parse::<u16>().ok())
        .filter(|port| *port > 0)
    {
        Some(port) if port != DEFAULT_LOCAL_RPC_PORT => {
            eprintln!(
                "[安全告警] RPC 端口被环境变量 {LOCAL_RPC_PORT_ENV} 覆盖为 {port}（默认 {DEFAULT_LOCAL_RPC_PORT}）"
            );
            port
        }
        Some(port) => port,
        None => DEFAULT_LOCAL_RPC_PORT,
    }
}

pub(crate) fn current_rpc_port() -> u16 {
    let mutex = LOCAL_RPC_PORT.get_or_init(|| Mutex::new(initial_rpc_port()));
    match mutex.lock() {
        Ok(guard) => *guard,
        Err(err) => *err.into_inner(),
    }
}

// 中文注释：公网使用 Cloudflare Edge，局域网使用受信任 TLS；缺失配置不回退本机明文。
pub(crate) fn local_rpc_http_url() -> Result<String, String> {
    let value = env::var("CITIZENCHAIN_RPC_URL")
        .map_err(|_| "CITIZENCHAIN_RPC_URL HTTPS 入口未配置".to_string())?;
    Ok(secure_rpc_url(&value)?.to_string())
}

fn secure_rpc_url(value: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value.trim()).map_err(|_| "RPC HTTPS 地址无效".to_string())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("RPC 只允许完整 HTTPS 地址".to_string());
    }
    Ok(url)
}

fn normalize_genesis_hash(raw: &str) -> Result<String, String> {
    let value = raw.trim();
    let Some(hex_part) = value.strip_prefix("0x") else {
        return Err("chain_getBlockHash(0) 返回值格式无效，应为 0x + 64 位十六进制".to_string());
    };
    if hex_part.len() != 64 || !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("chain_getBlockHash(0) 返回值格式无效，应为 0x + 64 位十六进制".to_string());
    }
    Ok(format!("0x{}", hex_part.to_ascii_lowercase()))
}

fn rpc_http_client() -> Result<&'static reqwest::blocking::Client, String> {
    if let Some(client) = RPC_HTTP_CLIENT.get() {
        return Ok(client);
    }
    let _guard = RPC_HTTP_CLIENT_INIT_LOCK.lock().unwrap_or_else(|err| {
        eprintln!("RPC_HTTP_CLIENT_INIT_LOCK poisoned; continuing with recovered lock");
        err.into_inner()
    });
    if let Some(client) = RPC_HTTP_CLIENT.get() {
        return Ok(client);
    }
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_millis(RPC_CONNECT_TIMEOUT_MS))
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .pool_max_idle_per_host(8)
        .pool_idle_timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("create RPC HTTP client failed: {e}"))?;
    let _ = RPC_HTTP_CLIENT.set(client);
    RPC_HTTP_CLIENT
        .get()
        .ok_or_else(|| "create RPC HTTP client failed: unset client".to_string())
}

pub(crate) fn rpc_post(
    method: &str,
    params: Value,
    request_timeout: Duration,
    max_response_bytes: u64,
) -> Result<Value, String> {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let client = rpc_http_client()?;
    let rpc_http_url = local_rpc_http_url()?;
    let response = client
        .post(rpc_http_url)
        .timeout(request_timeout)
        .json(&payload)
        .send()
        .map_err(|e| format!("RPC 请求失败: {e}"))?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(format!("RPC HTTP 状态异常: {}", response.status()));
    }
    if let Some(content_length) = response.content_length() {
        if content_length > max_response_bytes {
            return Err(format!(
                "RPC 响应体过大: {} bytes (limit {})",
                content_length, max_response_bytes
            ));
        }
    }

    let mut limited_reader = response.take(max_response_bytes.saturating_add(1));
    let mut body: Vec<u8> = Vec::new();
    limited_reader
        .read_to_end(&mut body)
        .map_err(|e| format!("RPC 读取失败: {e}"))?;
    if body.len() as u64 > max_response_bytes {
        return Err(format!("RPC 响应体超过限制: {} bytes", max_response_bytes));
    }

    let json: Value =
        serde_json::from_slice(&body).map_err(|e| format!("RPC JSON 解析失败: {e}"))?;
    if let Some(err) = json.get("error") {
        return Err(format!("RPC 返回错误: {err}"));
    }
    Ok(json.get("result").cloned().unwrap_or(Value::Null))
}

/// 向指定 URL 发送 JSON-RPC 请求（用于远程 bootnode 查询）。
/// 只允许 HTTPS 且禁止重定向降级；系统证书和主机名验证保持。
pub(crate) fn rpc_post_url(
    url: &str,
    method: &str,
    params: Value,
    request_timeout: Duration,
    max_response_bytes: u64,
) -> Result<Value, String> {
    let url = secure_rpc_url(url)?;
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let client = rpc_http_client()?;
    let response = client
        .post(url.clone())
        .timeout(request_timeout)
        .json(&payload)
        .send()
        .map_err(|e| format!("RPC 请求失败 ({url}): {e}"))?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(format!("RPC HTTP 状态异常 ({url}): {}", response.status()));
    }
    if let Some(content_length) = response.content_length() {
        if content_length > max_response_bytes {
            return Err(format!(
                "RPC 响应体过大 ({url}): {} bytes (limit {})",
                content_length, max_response_bytes
            ));
        }
    }

    let mut limited_reader = response.take(max_response_bytes.saturating_add(1));
    let mut body: Vec<u8> = Vec::new();
    limited_reader
        .read_to_end(&mut body)
        .map_err(|e| format!("RPC 读取失败 ({url}): {e}"))?;
    if body.len() as u64 > max_response_bytes {
        return Err(format!(
            "RPC 响应体超过限制 ({url}): {} bytes",
            max_response_bytes
        ));
    }

    let json: Value =
        serde_json::from_slice(&body).map_err(|e| format!("RPC JSON 解析失败 ({url}): {e}"))?;
    if let Some(err) = json.get("error") {
        return Err(format!("RPC 返回错误 ({url}): {err}"));
    }
    Ok(json.get("result").cloned().unwrap_or(Value::Null))
}

/// 获取本地 RPC 节点的 genesis hash 并缓存。
/// 首次连接时从 `chain_getBlockHash(0)` 获取并存储，后续直接返回缓存。
pub(crate) fn cached_genesis_hash() -> Result<String, String> {
    let mutex = CACHED_GENESIS_HASH.get_or_init(|| Mutex::new(None));
    let mut guard = mutex
        .lock()
        .map_err(|_| "genesis hash cache lock poisoned".to_string())?;
    if let Some(ref hash) = *guard {
        return Ok(hash.clone());
    }
    let hash_str = fetch_local_genesis_hash()?;
    *guard = Some(hash_str.clone());
    Ok(hash_str)
}

fn fetch_local_genesis_hash() -> Result<String, String> {
    let hash = rpc_post(
        "chain_getBlockHash",
        Value::Array(vec![Value::Number(0.into())]),
        GENESIS_HASH_TIMEOUT,
        GENESIS_HASH_MAX_BYTES,
    )?;
    let hash_str = hash
        .as_str()
        .ok_or_else(|| "chain_getBlockHash(0) 返回值不是字符串".to_string())?
        .to_string();
    normalize_genesis_hash(&hash_str)
}

/// 校验当前连接的 RPC 节点 genesis hash 是否与首次缓存的一致。
/// 如果尚无缓存，首次调用会自动缓存。
pub(crate) fn verify_genesis_hash() -> Result<(), String> {
    let expected = cached_genesis_hash()?;
    let current = rpc_post(
        "chain_getBlockHash",
        Value::Array(vec![Value::Number(0.into())]),
        GENESIS_HASH_TIMEOUT,
        GENESIS_HASH_MAX_BYTES,
    )?;
    let current_str = current
        .as_str()
        .ok_or_else(|| "chain_getBlockHash(0) 返回值不是字符串".to_string())?;
    let current_str = normalize_genesis_hash(current_str)?;
    if current_str != expected {
        return Err(format!(
            "RPC 节点 genesis hash 不匹配（期望 {expected}，实际 {current_str}），可能连接到了错误的链"
        ));
    }
    Ok(())
}

/// 等待本机链 RPC 真正可用。
///
/// 进程内节点线程存活不等于创世状态已经物化完成;大创世场景下 RPC 端口可能很久
/// 才会监听。所有 UI 可用态和 OnChina 启动前置检查都以本函数成功读到
/// `chain_getBlockHash(0)` 为准。
pub(crate) fn wait_for_local_rpc_ready<F>(
    timeout: Duration,
    should_continue: F,
) -> Result<String, String>
where
    F: Fn() -> Result<(), String>,
{
    let started_at = Instant::now();
    loop {
        should_continue()?;
        let last_error = match fetch_local_genesis_hash() {
            Ok(hash) => {
                let mutex = CACHED_GENESIS_HASH.get_or_init(|| Mutex::new(None));
                if let Ok(mut guard) = mutex.lock() {
                    *guard = Some(hash.clone());
                }
                return Ok(hash);
            }
            Err(err) => err,
        };
        if started_at.elapsed() >= timeout {
            return Err(format!(
                "节点 RPC 在 {} 秒内未就绪: {last_error}",
                timeout.as_secs()
            ));
        }
        thread::sleep(RPC_READY_POLL_INTERVAL);
    }
}

/// 节点停止后清除 genesis hash 缓存，以便下次启动时重新校验。
pub(crate) fn clear_genesis_hash_cache() {
    if let Some(mutex) = CACHED_GENESIS_HASH.get() {
        if let Ok(mut guard) = mutex.lock() {
            *guard = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_genesis_hash;

    #[test]
    fn normalize_genesis_hash_accepts_valid_hash() {
        let hash = format!("0x{}", "Aa".repeat(32));
        let normalized = normalize_genesis_hash(&hash).unwrap();
        assert_eq!(normalized, format!("0x{}", "aa".repeat(32)));
    }

    #[test]
    fn normalize_genesis_hash_rejects_missing_prefix() {
        let hash = "aa".repeat(32);
        assert!(normalize_genesis_hash(&hash).is_err());
    }

    #[test]
    fn normalize_genesis_hash_rejects_wrong_length() {
        assert!(normalize_genesis_hash("0x1234").is_err());
    }

    #[test]
    fn normalize_genesis_hash_rejects_non_hex() {
        let hash = format!("0x{}zz", "aa".repeat(31));
        assert!(normalize_genesis_hash(&hash).is_err());
    }
}
