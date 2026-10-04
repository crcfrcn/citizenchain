// RPC 子模块：节点 RPC 调用、链同步状态查询。

use crate::governance::chain_query;
use crate::shared::{constants::EXPECTED_SS58_PREFIX, rpc};
use primitives::cid::china::china_ch::CHINA_CH;
use serde::Serialize;
use serde_json::Value;
use std::sync::OnceLock;
use std::{thread, time::Duration};
use tauri::AppHandle;

use super::identity::current_status;

use crate::shared::constants::RPC_RESPONSE_LIMIT_LARGE;
const RPC_RETRY_COUNT: usize = 3;

pub(super) fn rpc_post(method: &str, params: Value) -> Result<Value, String> {
    let mut last_err = String::new();
    for attempt in 0..RPC_RETRY_COUNT {
        match rpc::rpc_post(
            method,
            params.clone(),
            rpc::RPC_REQUEST_TIMEOUT,
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
    if rpc::verify_genesis_hash().is_err() {
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
    // (ADR-017):finalized 钉块哈希统一取自 governance::chain_query 收口。
    let hash = chain_query::fetch_finalized_head().ok()?;
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
/// 首页展示的链同步状态。
pub struct ChainStatus {
    pub block_height: Option<u64>,
    pub finalized_height: Option<u64>,
    pub syncing: Option<bool>,
    /// 链上 runtime 的 spec_version（节点运行时可用）。
    pub spec_version: Option<u32>,
    /// 节点程序版本号（始终可用）。
    pub node_version: String,
}

/// 从 Cargo.toml 编译时嵌入的节点程序版本号。
fn cargo_pkg_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// 从 state_getRuntimeVersion RPC 获取 spec_version。
fn fetch_spec_version() -> Option<u32> {
    let result = rpc_post("state_getRuntimeVersion", Value::Array(vec![])).ok()?;
    result
        .get("specVersion")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
}

fn get_chain_status_sync(app: AppHandle) -> Result<ChainStatus, String> {
    let node_version = cargo_pkg_version();

    if !current_status(&app)?.running {
        return Ok(ChainStatus {
            block_height: None,
            finalized_height: None,
            syncing: None,
            spec_version: None,
            node_version,
        });
    }

    let block_height = rpc_post("chain_getHeader", Value::Array(vec![]))
        .ok()
        .as_ref()
        .and_then(header_block_height);
    let finalized_height = finalized_block_height();
    let syncing = syncing_flag();
    let spec_version = fetch_spec_version();

    Ok(ChainStatus {
        block_height,
        finalized_height,
        syncing,
        spec_version,
        node_version,
    })
}

#[tauri::command]
pub async fn get_chain_status(app: AppHandle) -> Result<ChainStatus, String> {
    super::join_blocking_task(
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
            crate::shared::storage_keys::to_hex(&crate::shared::storage_keys::prefix(
                b"Balances",
                b"TotalIssuance",
            ))
        })
        .clone()
}

fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    let trimmed = hex.strip_prefix("0x").unwrap_or(hex);
    if !trimmed.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(trimmed.len() / 2);
    for i in (0..trimmed.len()).step_by(2) {
        let byte = u8::from_str_radix(&trimmed[i..i + 2], 16).ok()?;
        out.push(byte);
    }
    Some(out)
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
    let finalized_hash = chain_query::fetch_finalized_head()?;
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
    super::join_blocking_task(
        "get_total_issuance",
        tauri::async_runtime::spawn_blocking(move || get_total_issuance_sync(app)),
    )
    .await
}

// ── 永久质押金额（43 个省储行 stake_account 余额之和）──

/// 构造 System.Account 的完整 storage key（Blake2_128Concat hasher，委托 shared 单源）。
fn system_account_storage_key(account_id: &[u8; 32]) -> String {
    crate::shared::storage_keys::to_hex(&crate::shared::storage_keys::blake2_map(
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
    let finalized_hash = chain_query::fetch_finalized_head()?;

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
    super::join_blocking_task(
        "get_total_stake",
        tauri::async_runtime::spawn_blocking(move || get_total_stake_sync(app)),
    )
    .await
}
