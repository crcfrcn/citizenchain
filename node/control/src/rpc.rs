use crate::identity::current_status;
use codec::{Decode, Encode};
use entitybase::RegisteredInstitution;
use nodedesktop::constants::EXPECTED_SS58_PREFIX;
use nodedesktop::constants::RPC_RESPONSE_LIMIT_LARGE;
use primitives::cid::china::china_ch::CHINA_CH;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use tauri::AppHandle;

const RPC_RETRY_COUNT: usize = 3;

pub(crate) fn rpc_post(method: &str, params: Value) -> Result<Value, String> {
    let mut last_err = String::new();
    for attempt in 0..RPC_RETRY_COUNT {
        match nodedesktop::rpc::rpc_post(
            method,
            params.clone(),
            nodedesktop::rpc::RPC_REQUEST_TIMEOUT,
            RPC_RESPONSE_LIMIT_LARGE,
        ) {
            Ok(v) => return Ok(v),
            Err(err) => {
                last_err = err;
                if attempt + 1 < RPC_RETRY_COUNT {
                    thread::sleep(Duration::from_millis(250));
                }
            }
        }
    }
    Err(last_err)
}

/// 保留给节点切换前的快速自检入口，当前主流程改由 genesis 校验直接兜底。
#[allow(dead_code)]
pub(super) fn is_expected_rpc_node() -> bool {
    let Ok(properties) = rpc_post("system_properties", Value::Array(vec![])) else {
        return false;
    };
    let ss58 = properties
        .get("ss58Format")
        .and_then(|v| {
            if let Some(raw) = v.as_u64() {
                Some(raw)
            } else {
                v.as_str().and_then(|s| s.parse::<u64>().ok())
            }
        })
        .unwrap_or(0);
    if ss58 != EXPECTED_SS58_PREFIX {
        return false;
    }

    let has_name = rpc_post("system_name", Value::Array(vec![]))
        .ok()
        .and_then(|v| v.as_str().map(|s| !s.trim().is_empty()))
        .unwrap_or(false);
    if !has_name {
        return false;
    }

    // 补充 genesis hash 校验：首次连接缓存，后续比对。
    if nodedesktop::rpc::verify_genesis_hash().is_err() {
        return false;
    }

    true
}

fn hex_to_u64(hex: &str) -> Option<u64> {
    let trimmed = hex.strip_prefix("0x")?;
    u64::from_str_radix(trimmed, 16).ok()
}

fn header_block_height(header: &Value) -> Option<u64> {
    header
        .get("number")
        .and_then(Value::as_str)
        .and_then(hex_to_u64)
}

fn finalized_block_height() -> Option<u64> {
    // (ADR-017):finalized 钉块哈希统一取自 nodeclient::query 收口。
    let hash = nodeclient::query::fetch_finalized_head().ok()?;
    let header = rpc_post("chain_getHeader", Value::Array(vec![Value::String(hash)])).ok()?;
    header_block_height(&header)
}

fn syncing_flag() -> Option<bool> {
    let health = rpc_post("system_health", Value::Array(vec![])).ok()?;
    if let Some(v) = health.get("isSyncing") {
        if let Some(b) = v.as_bool() {
            return Some(b);
        }
        if let Some(s) = v.as_str() {
            let lowered = s.trim().to_ascii_lowercase();
            if lowered == "true" {
                return Some(true);
            }
            if lowered == "false" {
                return Some(false);
            }
        }
    }
    None
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
/// 首页和设置页使用的链状态。
pub struct ChainStatus {
    pub block_height: Option<u64>,
    pub finalized_height: Option<u64>,
    pub syncing: Option<bool>,
    /// 链上 runtime 的 spec_version（节点运行时可用）。
    pub spec_version: Option<u32>,
    /// 链上 runtime 的交易签名版本。
    pub transaction_version: Option<u32>,
    /// 节点程序版本号（始终可用）。
    pub node_version: String,
}

/// 本平台本次交付的唯一节点产品版本，由产品编译入口统一注入。
pub const NODE_VERSION: &str = env!("CITIZENCHAIN_RELEASE_VERSION");

fn parse_runtime_versions(result: &Value) -> (Option<u32>, Option<u32>) {
    let read_version = |key| {
        result
            .get(key)
            .and_then(Value::as_u64)
            .and_then(|version| u32::try_from(version).ok())
    };
    (
        read_version("specVersion"),
        read_version("transactionVersion"),
    )
}

fn get_chain_status_sync(app: AppHandle) -> Result<ChainStatus, String> {
    let node_version = NODE_VERSION.to_owned();

    if !current_status(&app)?.running {
        return Ok(ChainStatus {
            block_height: None,
            finalized_height: None,
            syncing: None,
            spec_version: None,
            transaction_version: None,
            node_version,
        });
    }

    let block_height = rpc_post("chain_getHeader", Value::Array(vec![]))
        .ok()
        .as_ref()
        .and_then(header_block_height);
    let finalized_height = finalized_block_height();
    let syncing = syncing_flag();
    // 同一份 RPC 响应对应同一个链上 Runtime，升级期间也不会混用两个版本。
    let (spec_version, transaction_version) =
        rpc_post("state_getRuntimeVersion", Value::Array(vec![]))
            .as_ref()
            .map(parse_runtime_versions)
            .unwrap_or_default();

    Ok(ChainStatus {
        block_height,
        finalized_height,
        syncing,
        spec_version,
        transaction_version,
        node_version,
    })
}

#[tauri::command]
pub async fn get_chain_status(app: AppHandle) -> Result<ChainStatus, String> {
    crate::task::join_blocking_task(
        "get_chain_status",
        tauri::async_runtime::spawn_blocking(move || get_chain_status_sync(app)),
    )
    .await
}

// ── 全链发行总额（Balances.TotalIssuance）──

static TOTAL_ISSUANCE_STORAGE_KEY_CACHE: OnceLock<String> = OnceLock::new();

fn total_issuance_storage_key() -> String {
    TOTAL_ISSUANCE_STORAGE_KEY_CACHE
        .get_or_init(|| {
            nodedesktop::storage::to_hex(&nodedesktop::storage::prefix(
                b"Balances",
                b"TotalIssuance",
            ))
        })
        .clone()
}

fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    // 按十六进制字节解码；非ASCII输入返回None，不切片UTF-8字符。
    hex::decode(hex.strip_prefix("0x").unwrap_or(hex)).ok()
}

fn scale_u128_from_storage_hex(hex: &str) -> Option<u128> {
    let bytes = hex_to_bytes(hex)?;
    if bytes.len() < 16 {
        return None;
    }
    let mut raw = [0u8; 16];
    raw.copy_from_slice(&bytes[..16]);
    Some(u128::from_le_bytes(raw))
}

/// 将分（u128）格式化为带千分位分隔符和两位小数的元字符串。
fn format_fen_to_yuan(amount_fen: u128) -> String {
    let major = amount_fen / 100;
    let minor = (amount_fen % 100) as u32;

    // 千分位分隔
    let major_str = major.to_string();
    let mut result = String::with_capacity(major_str.len() + major_str.len() / 3 + 3);
    for (i, ch) in major_str.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    let result: String = result.chars().rev().collect();
    format!("{result}.{minor:02}")
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
/// 全链发行总额。
pub struct TotalIssuance {
    pub total_issuance: Option<String>,
}

fn get_total_issuance_sync(app: AppHandle) -> Result<TotalIssuance, String> {
    if !current_status(&app)?.running {
        return Ok(TotalIssuance {
            total_issuance: None,
        });
    }

    let key = total_issuance_storage_key();
    let finalized_hash = nodeclient::query::fetch_finalized_head()?;
    let raw = rpc_post(
        "state_getStorage",
        Value::Array(vec![Value::String(key), Value::String(finalized_hash)]),
    )?;
    let amount = raw
        .as_str()
        .and_then(scale_u128_from_storage_hex)
        .map(format_fen_to_yuan);

    Ok(TotalIssuance {
        total_issuance: amount,
    })
}

#[tauri::command]
pub async fn get_total_issuance(app: AppHandle) -> Result<TotalIssuance, String> {
    crate::task::join_blocking_task(
        "get_total_issuance",
        tauri::async_runtime::spawn_blocking(move || get_total_issuance_sync(app)),
    )
    .await
}

// ── 永久质押金额（43 个省储行 stake_account 余额之和）──

/// 构造 System.Account 的完整 storage key（Blake2_128Concat hasher，委托 shared 单源）。
fn system_account_storage_key(account_id: &[u8; 32]) -> String {
    nodedesktop::storage::to_hex(&nodedesktop::storage::blake2_map(
        b"System", b"Account", account_id,
    ))
}

/// 从 System.Account SCALE 编码中提取 free balance（u128，小端，偏移 16 字节）。
/// AccountInfo<Nonce, AccountData> 布局：
///   nonce: u32 (4 bytes)
///   consumers: u32 (4 bytes)
///   providers: u32 (4 bytes)
///   sufficients: u32 (4 bytes)
///   data.free: u128 (16 bytes)  ← offset 16
fn extract_free_balance(storage_hex: &str) -> Option<u128> {
    let bytes = hex_to_bytes(storage_hex)?;
    // free balance 起始于偏移 16 字节处
    if bytes.len() < 32 {
        return None;
    }
    let mut raw = [0u8; 16];
    raw.copy_from_slice(&bytes[16..32]);
    Some(u128::from_le_bytes(raw))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
/// 永久质押金额。
pub struct TotalStake {
    pub total_stake: Option<String>,
}

fn get_total_stake_sync(app: AppHandle) -> Result<TotalStake, String> {
    if !current_status(&app)?.running {
        return Ok(TotalStake { total_stake: None });
    }

    let mut total: u128 = 0;

    // 金额类展示统一读取 finalized 块，避免 best 头金额先行变化。
    let finalized_hash = nodeclient::query::fetch_finalized_head()?;

    // 批量构造 43 个存储键，逐个查询。
    for bank in CHINA_CH.iter() {
        let key = system_account_storage_key(&bank.stake_account);
        let raw = match rpc_post(
            "state_getStorage",
            Value::Array(vec![
                Value::String(key),
                Value::String(finalized_hash.clone()),
            ]),
        ) {
            Ok(v) => v,
            Err(_) => continue, // 单个查询失败跳过，不中断
        };

        if let Some(balance) = raw.as_str().and_then(extract_free_balance) {
            total = total.saturating_add(balance);
        }
    }

    Ok(TotalStake {
        total_stake: Some(format_fen_to_yuan(total)),
    })
}

#[tauri::command]
pub async fn get_total_stake(app: AppHandle) -> Result<TotalStake, String> {
    crate::task::join_blocking_task(
        "get_total_stake",
        tauri::async_runtime::spawn_blocking(move || get_total_stake_sync(app)),
    )
    .await
}

// ── 临时冻结金额：尚未达标的市注册局主账户 free 余额合计 ──

const FROZEN_QUERY_PAGE_SIZE: usize = 256;
static TOTAL_FROZEN_CACHE: Mutex<Option<(String, u128)>> = Mutex::new(None);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TotalFrozen {
    pub total_frozen: Option<String>,
}

fn decode_frozen_storage<T: Decode>(hex: &str, label: &str) -> Result<T, String> {
    let bytes = hex_to_bytes(hex).ok_or_else(|| format!("{label} 十六进制无效"))?;
    let mut input = bytes.as_slice();
    let value = T::decode(&mut input).map_err(|error| format!("{label} 解码失败: {error}"))?;
    if !input.is_empty() {
        return Err(format!("{label} 存在多余字节"));
    }
    Ok(value)
}

/// 同一 finalized 快照批量读完整结果；缺项、重复项或错误快照均不能当作零余额。
fn frozen_storage_batch(
    keys: &[String],
    hash: &str,
    rpc: &impl Fn(&str, Value) -> Result<Value, String>,
) -> Result<BTreeMap<String, Option<String>>, String> {
    if keys.is_empty() {
        return Ok(BTreeMap::new());
    }
    let result = rpc("state_queryStorageAt", serde_json::json!([keys, hash]))?;
    let sets = result.as_array().ok_or("冻结金额批量查询结果无效")?;
    if sets.len() != 1 || sets[0].get("block").and_then(Value::as_str) != Some(hash) {
        return Err("冻结金额批量查询区块不一致".into());
    }
    let changes = sets[0]
        .get("changes")
        .and_then(Value::as_array)
        .ok_or("冻结金额批量查询缺少账户数据")?;
    let mut values = BTreeMap::new();
    for change in changes {
        let pair = change
            .as_array()
            .filter(|pair| pair.len() == 2)
            .ok_or("冻结金额批量查询条目无效")?;
        let key = pair[0].as_str().ok_or("冻结金额批量查询键无效")?;
        if !keys.iter().any(|expected| expected == key) {
            return Err("冻结金额批量查询包含未请求的数据".into());
        }
        let value = match &pair[1] {
            Value::Null => None,
            Value::String(value) => Some(value.clone()),
            _ => return Err("冻结金额批量查询值无效".into()),
        };
        if values.insert(key.to_string(), value).is_some() {
            return Err("冻结金额批量查询数据重复".into());
        }
    }
    if values.len() != keys.len() {
        return Err("冻结金额批量查询数据不完整".into());
    }
    Ok(values)
}

/// 与 RuntimeInstitutionAsset 一致：按账户反查中的 CREG + 主账户识别，rank=None 才计入。
/// 遍历链上实际登记表，覆盖创世和后续新增市注册局，不以固定城市名单推算。
fn total_frozen_at(
    hash: &str,
    rpc: &impl Fn(&str, Value) -> Result<Value, String>,
) -> Result<u128, String> {
    let prefix = nodeclient::query::value_key("PublicManage", "AccountRegisteredCid");
    let mut start: Option<String> = None;
    let mut total = 0u128;
    loop {
        let result = rpc(
            "state_getKeysPaged",
            serde_json::json!([prefix, FROZEN_QUERY_PAGE_SIZE, start, hash,]),
        )?;
        let page = result.as_array().ok_or("冻结金额账户分页结果无效")?;
        if page.len() > FROZEN_QUERY_PAGE_SIZE {
            return Err("冻结金额账户分页超出请求数量".into());
        }
        let mut keys = Vec::with_capacity(page.len());
        let mut previous = start.as_deref();
        for value in page {
            let key = value.as_str().ok_or("冻结金额账户存储键无效")?;
            // Blake2_128Concat<AccountId32> = 32 字节前缀 + 16 字节哈希 + 32 字节账户。
            let bytes = hex_to_bytes(key)
                .filter(|bytes| bytes.len() == 80)
                .ok_or("冻结金额账户存储键长度无效")?;
            if !key.starts_with(&prefix)
                || previous.is_some_and(|last| key <= last)
                || nodeclient::query::map_key("PublicManage", "AccountRegisteredCid", &bytes[48..])
                    != key
            {
                return Err("冻结金额账户分页顺序或存储键无效".into());
            }
            previous = Some(key);
            keys.push(key.to_string());
        }
        if keys.is_empty() {
            break;
        }
        let accounts = frozen_storage_batch(&keys, hash, rpc)?;
        let mut cities = Vec::new();
        for key in &keys {
            let raw = accounts[key].as_deref().ok_or("冻结金额账户登记缺失")?;
            let account: RegisteredInstitution<Vec<u8>, Vec<u8>> =
                decode_frozen_storage(raw, "公权机构账户登记")?;
            if account.account_name != primitives::account_derive::RESERVED_NAME_MAIN {
                continue;
            }
            let cid = primitives::cid::number::parse_cid_number_parts_bytes(&account.cid_number)
                .map_err(|_| "冻结金额账户 CID 无效")?;
            if cid.institution != *b"CREG" {
                continue;
            }
            let account_bytes = hex_to_bytes(key).ok_or("冻结金额账户存储键无效")?;
            let balance_key = nodeclient::query::map_key("System", "Account", &account_bytes[48..]);
            let progress_key = nodeclient::query::map_key(
                "CitizenIdentity",
                "CityRegistryProgress",
                &account.cid_number.encode(),
            );
            cities.push((balance_key, progress_key));
        }
        let mut progress_keys: Vec<_> = cities.iter().map(|(_, key)| key.clone()).collect();
        progress_keys.sort();
        progress_keys.dedup();
        let progress = frozen_storage_batch(&progress_keys, hash, rpc)?;
        let mut frozen_keys = Vec::new();
        for (balance_key, progress_key) in cities {
            // CityRegistryProgress 是 ValueQuery，缺失记录等同默认 registered=0、rank=None。
            // SCALE 字段沿用链上顺序 (u64, Option<u32>)，不在 Node 重算达标阈值。
            let (_, rank): (u64, Option<u32>) = match progress[&progress_key].as_deref() {
                Some(raw) => decode_frozen_storage(raw, "市注册局达标记录")?,
                None => (0, None),
            };
            if rank.is_none() {
                frozen_keys.push(balance_key);
            }
        }
        let balances = frozen_storage_batch(&frozen_keys, hash, rpc)?;
        for balance in balances.values() {
            let amount = match balance {
                None => 0, // 未充值的 System.Account 不存在，实际余额为零。
                Some(raw) => extract_free_balance(raw).ok_or("市注册局主账户余额解码失败")?,
            };
            total = total.checked_add(amount).ok_or("冻结金额合计溢出")?;
        }
        start = keys.last().cloned();
        if keys.len() < FROZEN_QUERY_PAGE_SIZE {
            break;
        }
    }
    Ok(total)
}

/// 只缓存完整成功的 finalized 统计；锁同时合并首页并发请求，避免重复扫描全链账户。
fn cached_total_frozen(
    cache: &Mutex<Option<(String, u128)>>,
    hash: &str,
    load: impl FnOnce() -> Result<u128, String>,
) -> Result<u128, String> {
    let mut cached = cache.lock().map_err(|_| "冻结金额缓存锁异常")?;
    if let Some((previous_hash, total)) = cached.as_ref() {
        if previous_hash == hash {
            return Ok(*total);
        }
    }
    let total = load()?;
    *cached = Some((hash.to_string(), total));
    Ok(total)
}

fn get_total_frozen_sync(app: AppHandle) -> Result<TotalFrozen, String> {
    if !current_status(&app)?.running {
        *TOTAL_FROZEN_CACHE
            .lock()
            .map_err(|_| "冻结金额缓存锁异常")? = None;
        return Ok(TotalFrozen { total_frozen: None });
    }
    let hash = nodeclient::query::fetch_finalized_head()?;
    let total = cached_total_frozen(&TOTAL_FROZEN_CACHE, &hash, || {
        total_frozen_at(&hash, &rpc_post)
    })?;
    Ok(TotalFrozen {
        total_frozen: Some(format_fen_to_yuan(total)),
    })
}

#[tauri::command]
pub async fn get_total_frozen(app: AppHandle) -> Result<TotalFrozen, String> {
    crate::task::join_blocking_task(
        "get_total_frozen",
        tauri::async_runtime::spawn_blocking(move || get_total_frozen_sync(app)),
    )
    .await
}

#[cfg(test)]
#[path = "../tests/rpc.rs"]
mod tests;
