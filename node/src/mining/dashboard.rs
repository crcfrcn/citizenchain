use crate::{
    home,
    settings::reward_account,
    shared::{constants, rpc, security},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    cmp,
    collections::{HashMap, VecDeque},
    fs,
    path::PathBuf,
    sync::{Mutex, MutexGuard, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::AppHandle;

const RECENT_RECORD_LIMIT: usize = 20;
const MAX_BLOCKS_PER_REFRESH: u64 = 100;
const DAY_MS: u64 = 86_400_000;
const INCOME_DAY_KEEP: u64 = 400;
const MINING_CACHE_FILENAME: &str = "mining-dashboard-cache.json";
const CACHE_PERSIST_MIN_INTERVAL_MS: u64 = 60_000;
use crate::shared::constants::RPC_RESPONSE_LIMIT_LARGE;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
/// 前端“收益卡片”展示的累计与当日收益。
pub struct MiningIncome {
    pub total_income: String,
    pub total_fee_income: String,
    pub total_reward_income: String,
    pub today_income: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
/// 前端“出块记录”表格的单行数据。
pub struct MiningBlockRecord {
    pub block_height: u64,
    pub timestamp_ms: Option<u64>,
    pub fee: String,
    pub block_reward: String,
    pub author: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
/// 挖矿看板聚合响应。
pub struct MiningDashboard {
    pub income: MiningIncome,
    pub records: Vec<MiningBlockRecord>,
    pub warning: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CachedBlockRecord {
    block_height: u64,
    timestamp_ms: Option<u64>,
    fee_fen: u128,
    block_reward_fen: u128,
    author: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
#[serde(deny_unknown_fields)]
struct MiningComputationCache {
    chain_genesis_hash: Option<String>,
    tracked_miner_account_id: Option<String>,
    last_processed_height: u64,
    last_processed_hash: Option<String>,
    total_fee_fen: u128,
    total_reward_fen: u128,
    income_by_utc_day: HashMap<u64, u128>,
    recent_records: VecDeque<CachedBlockRecord>,
}

#[derive(Clone, Debug, Default)]
struct RefreshStats {
    fee_query_failures: u64,
    timestamp_query_failures: u64,
    processed_blocks: u64,
    pending_blocks: u64,
    local_miner_missing: bool,
    persist_failed: bool,
}

#[derive(Clone, Debug)]
struct ProcessedBlock {
    height: u64,
    hash: String,
    timestamp_ms: Option<u64>,
    fee_fen: u128,
    fee_query_failures: u32,
    timestamp_query_failed: bool,
    reward_fen: u128,
    author: String,
}

static MINING_CACHE: OnceLock<Mutex<MiningComputationCache>> = OnceLock::new();
static MINING_REFRESHING: OnceLock<Mutex<bool>> = OnceLock::new();
static MINING_CACHE_LOADED: OnceLock<Mutex<bool>> = OnceLock::new();
static TIMESTAMP_NOW_STORAGE_KEY_CACHE: OnceLock<String> = OnceLock::new();
static LAST_CACHE_PERSIST_AT_MS: OnceLock<Mutex<u64>> = OnceLock::new();

fn mining_cache() -> &'static Mutex<MiningComputationCache> {
    MINING_CACHE.get_or_init(|| Mutex::new(MiningComputationCache::default()))
}

fn mining_refreshing_flag() -> &'static Mutex<bool> {
    MINING_REFRESHING.get_or_init(|| Mutex::new(false))
}

fn mining_cache_loaded_flag() -> &'static Mutex<bool> {
    MINING_CACHE_LOADED.get_or_init(|| Mutex::new(false))
}

fn last_cache_persist_at_ms() -> &'static Mutex<u64> {
    LAST_CACHE_PERSIST_AT_MS.get_or_init(|| Mutex::new(0))
}

// 该模块会被前端轮询，缓存锁一旦因 panic 污染，就重置成默认值继续服务，
// 避免一次异常导致整个挖矿面板永久不可用。
fn lock_or_reset<'a, T: Default>(mutex: &'a Mutex<T>, name: &str) -> MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(err) => {
            eprintln!("{name} mutex poisoned; reset to default value");
            let mut guard = err.into_inner();
            *guard = T::default();
            guard
        }
    }
}

fn mining_cache_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(security::app_data_dir(app)?.join(MINING_CACHE_FILENAME))
}

fn persist_mining_cache(app: &AppHandle, cache: &MiningComputationCache) -> Result<(), String> {
    let payload =
        serde_json::to_string(cache).map_err(|e| format!("encode mining cache failed: {e}"))?;
    let path = mining_cache_path(app)?;
    security::write_text_atomic_restricted(&path, &format!("{payload}\n")).map_err(|e| {
        format!(
            "write mining cache failed ({}): {e}",
            security::sanitize_path(&path)
        )
    })
}

// 先更新进程内缓存，再按时间窗口/追赶进度决定是否落盘，兼顾数据连续性与写盘频率。
fn commit_working_cache(
    app: &AppHandle,
    working: &MiningComputationCache,
    cache_changed: bool,
    force_persist: bool,
    pending_blocks: u64,
) -> bool {
    if !cache_changed {
        return false;
    }
    {
        let mut cache = lock_or_reset(mining_cache(), "MINING_CACHE");
        *cache = working.clone();
    }

    let now_ms = unix_now_ms().unwrap_or(0);
    let should_persist = {
        let mut last = lock_or_reset(last_cache_persist_at_ms(), "LAST_CACHE_PERSIST_AT_MS");
        let due = force_persist
            || pending_blocks == 0
            || now_ms.saturating_sub(*last) >= CACHE_PERSIST_MIN_INTERVAL_MS;
        if due {
            *last = now_ms;
        }
        due
    };

    if should_persist {
        return persist_mining_cache(app, working).is_err();
    }
    false
}

fn maybe_load_mining_cache(app: &AppHandle) -> Result<Option<MiningComputationCache>, String> {
    let path = mining_cache_path(app)?;
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path).map_err(|e| {
        format!(
            "read mining cache failed ({}): {e}",
            security::sanitize_path(&path)
        )
    })?;
    let cache: MiningComputationCache = serde_json::from_str(&raw).map_err(|e| {
        format!(
            "parse mining cache failed ({}): {e}",
            security::sanitize_path(&path)
        )
    })?;
    Ok(Some(cache))
}

fn ensure_mining_cache_loaded(app: &AppHandle) {
    let mut loaded = lock_or_reset(mining_cache_loaded_flag(), "MINING_CACHE_LOADED");
    if *loaded {
        return;
    }
    match maybe_load_mining_cache(app) {
        Ok(Some(cache)) => {
            let mut guard = lock_or_reset(mining_cache(), "MINING_CACHE");
            *guard = cache;
        }
        Ok(None) => {}
        Err(err) => eprintln!("load mining cache skipped: {err}"),
    }
    *loaded = true;
}

fn unix_now_ms() -> Result<u64, String> {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("system time error: {e}"))?
        .as_millis();
    Ok(u64::try_from(ms).unwrap_or(u64::MAX))
}

fn utc_day(ms: u64) -> u64 {
    ms / DAY_MS
}

fn rpc_post(method: &str, params: Value) -> Result<Value, String> {
    rpc::rpc_post(
        method,
        params,
        rpc::RPC_REQUEST_TIMEOUT,
        RPC_RESPONSE_LIMIT_LARGE,
    )
}

fn ensure_expected_rpc_node_uncached() -> Result<(), String> {
    let properties = rpc_post("system_properties", Value::Array(vec![]))?;
    let ss58 = properties
        .get("ss58Format")
        .and_then(|v| {
            if let Some(raw) = v.as_u64() {
                Some(raw)
            } else {
                v.as_str().and_then(|s| s.parse::<u64>().ok())
            }
        })
        .ok_or_else(|| "RPC 节点缺少 ss58Format".to_string())?;
    if ss58 != constants::EXPECTED_SS58_PREFIX {
        return Err(format!("RPC 链前缀不匹配：expected=2027, got={ss58}"));
    }

    let name = rpc_post("system_name", Value::Array(vec![]))?
        .as_str()
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    if name.is_empty() {
        return Err("RPC 节点名称为空".to_string());
    }
    Ok(())
}

fn ensure_expected_rpc_node() -> Result<(), String> {
    // RPC 目标可能随着节点重启或端口复用而变化，不能跨请求永久信任第一次校验结果。
    ensure_expected_rpc_node_uncached()
}

fn chain_genesis_hash_uncached() -> Result<String, String> {
    rpc_post(
        "chain_getBlockHash",
        Value::Array(vec![Value::String("0x0".to_string())]),
    )?
    .as_str()
    .map(|s| s.to_string())
    .ok_or_else(|| "chain_getBlockHash(0) 返回格式无效".to_string())
}

fn chain_genesis_hash() -> Result<String, String> {
    // genesis hash 用来判断链上下文是否切换；若永久缓存，链切换后会把旧链缓存误用到新链。
    chain_genesis_hash_uncached()
}

fn finalized_block_height() -> Result<u64, String> {
    let hash = rpc_post("chain_getFinalizedHead", Value::Array(vec![]))?
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "chain_getFinalizedHead 返回格式无效".to_string())?;
    let header = rpc_post("chain_getHeader", Value::Array(vec![Value::String(hash)]))?;
    header
        .get("number")
        .and_then(Value::as_str)
        .and_then(hex_to_u64)
        .ok_or_else(|| "chain_getHeader(finalized).number 缺失或格式无效".to_string())
}

fn block_hash_by_height(height: u64) -> Result<String, String> {
    rpc_post(
        "chain_getBlockHash",
        Value::Array(vec![Value::String(format!("0x{height:x}"))]),
    )?
    .as_str()
    .map(|s| s.to_string())
    .ok_or_else(|| format!("chain_getBlockHash({height}) 返回格式无效"))
}

fn maybe_block_timestamp_ms(ts_key: &str, block_hash: &str) -> Result<Option<u64>, String> {
    let raw = rpc_post(
        "state_getStorage",
        Value::Array(vec![
            Value::String(ts_key.to_string()),
            Value::String(block_hash.to_string()),
        ]),
    )?;
    Ok(raw.as_str().and_then(scale_u64_from_storage_hex))
}

fn hex_to_u64(hex: &str) -> Option<u64> {
    let trimmed = hex.strip_prefix("0x")?;
    u64::from_str_radix(trimmed, 16).ok()
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

fn scale_u64_from_storage_hex(hex: &str) -> Option<u64> {
    let bytes = hex_to_bytes(hex)?;
    if bytes.len() < 8 {
        return None;
    }
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&bytes[..8]);
    Some(u64::from_le_bytes(raw))
}

fn decode_hex_account_id_32(hex: &str) -> Option<[u8; 32]> {
    let bytes = hex_to_bytes(hex)?;
    if bytes.len() != 32 {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Some(out)
}

fn timestamp_now_storage_key() -> String {
    TIMESTAMP_NOW_STORAGE_KEY_CACHE
        .get_or_init(|| {
            crate::shared::storage_keys::to_hex(&crate::shared::storage_keys::prefix(
                b"Timestamp",
                b"Now",
            ))
        })
        .clone()
}

fn reward_account_storage_key(miner_account_id: &[u8; 32]) -> String {
    crate::shared::storage_keys::to_hex(&crate::shared::storage_keys::blake2_map(
        b"FullnodeIssuance",
        b"RewardAccountIdByMiner",
        miner_account_id,
    ))
}

fn reward_account_bound_at_block(miner_account_id: &str, block_hash: &str) -> Result<bool, String> {
    let miner_account_id = decode_hex_account_id_32(miner_account_id)
        .ok_or_else(|| format!("矿工账号格式无效：{miner_account_id}"))?;
    let raw = rpc_post(
        "state_getStorage",
        Value::Array(vec![
            Value::String(reward_account_storage_key(&miner_account_id)),
            Value::String(block_hash.to_string()),
        ]),
    )?;
    let Some(encoded) = raw.as_str() else {
        return Ok(false);
    };
    Ok(!encoded.trim_start_matches("0x").is_empty())
}

fn fullnode_fee_income_fen(total_fee_fen: u128) -> u128 {
    let total_percent = primitives::fee_policy::ONCHAIN_FEE_FULLNODE_PERCENT
        .saturating_add(primitives::fee_policy::ONCHAIN_FEE_NRC_PERCENT)
        .saturating_add(primitives::fee_policy::ONCHAIN_FEE_SAFETY_FUND_PERCENT);
    if total_percent == 0 {
        return 0;
    }
    // 与 runtime 的 `Imbalance::ration(80, 20)` 对齐，采用整数向下取整。
    total_fee_fen.saturating_mul(u128::from(
        primitives::fee_policy::ONCHAIN_FEE_FULLNODE_PERCENT,
    )) / u128::from(total_percent)
}

fn format_2_decimals_fen(amount_fen: u128) -> String {
    let major = amount_fen / 100;
    let minor = amount_fen % 100;
    // 千分位格式化
    let major_str = major.to_string();
    let mut result = String::new();
    for (i, ch) in major_str.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    let formatted: String = result.chars().rev().collect();
    format!("{formatted}.{minor:02}")
}

fn block_reward_fen_by_height(height: u64) -> u128 {
    let start = u64::from(primitives::pow_const::FULLNODE_REWARD_START_BLOCK);
    let end = u64::from(primitives::pow_const::FULLNODE_REWARD_END_BLOCK);
    if (start..=end).contains(&height) {
        primitives::pow_const::FULLNODE_BLOCK_REWARD
    } else {
        0
    }
}

fn decode_scale_compact_u32_prefix(bytes: &[u8]) -> Option<(usize, usize)> {
    let first = *bytes.first()?;
    match first & 0b11 {
        0b00 => Some((((first >> 2) as usize), 1)),
        0b01 => {
            if bytes.len() < 2 {
                return None;
            }
            let v = u16::from_le_bytes([bytes[0], bytes[1]]) >> 2;
            Some((v as usize, 2))
        }
        0b10 => {
            if bytes.len() < 4 {
                return None;
            }
            let v = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) >> 2;
            Some((v as usize, 4))
        }
        0b11 => {
            let byte_len = ((first >> 2) as usize) + 4;
            if bytes.len() < 1 + byte_len || byte_len > 4 {
                return None;
            }
            let mut v: usize = 0;
            for (i, b) in bytes[1..1 + byte_len].iter().enumerate() {
                v |= (*b as usize) << (8 * i);
            }
            Some((v, 1 + byte_len))
        }
        _ => None,
    }
}

/// 将 "0x{hex}" 格式的矿工公钥转为 SS58 地址显示，失败时原样返回。
fn hex_author_to_ss58(author_hex: &str) -> String {
    let stripped = author_hex.strip_prefix("0x").unwrap_or(author_hex);
    let Ok(bytes) = hex::decode(stripped) else {
        return author_hex.to_string();
    };
    crate::governance::signing::account_id_to_ss58(&bytes)
        .unwrap_or_else(|_| author_hex.to_string())
}

fn author_from_pow_digest_logs(logs: &[Value]) -> Option<String> {
    for log in logs {
        let Some(s) = log.as_str() else {
            continue;
        };
        let Some(bytes) = hex_to_bytes(s) else {
            continue;
        };
        if bytes.len() < 6 {
            continue;
        }
        if bytes[0] != 0x06 || &bytes[1..5] != b"pow_" {
            continue;
        }

        let payload = &bytes[5..];
        let Some((payload_len, prefix_len)) = decode_scale_compact_u32_prefix(payload) else {
            continue;
        };
        if payload.len() < prefix_len + payload_len || payload_len < 32 {
            continue;
        }
        let author = &payload[prefix_len..prefix_len + 32];
        return Some(format!("0x{}", hex::encode(author)));
    }
    None
}

// 通过自定义 RPC fee_blockFees 读取区块真实手续费（链上 FeePaid 事件累加）。
fn block_fee_fen(block_hash: &str) -> (u128, u32) {
    let params = Value::Array(vec![Value::String(block_hash.to_string())]);
    match rpc_post("fee_blockFees", params) {
        Ok(v) => {
            let fee = v
                .as_u64()
                .map(u128::from)
                .or_else(|| v.as_str().and_then(|s| s.parse::<u128>().ok()))
                .unwrap_or(0);
            (fee, 0)
        }
        Err(_) => (0, 1),
    }
}

// 单块处理负责把链上原始数据转换成统计友好的结构，
// 后续累计逻辑只消费 ProcessedBlock，减少状态更新分散在多处。
fn process_block(height: u64, ts_key: &str) -> Result<ProcessedBlock, String> {
    let block_hash = block_hash_by_height(height)?;
    let block = rpc_post(
        "chain_getBlock",
        Value::Array(vec![Value::String(block_hash.clone())]),
    )
    .map_err(|e| format!("load block {height} failed: {e}"))?;

    let (fee_fen, fee_query_failures) = block_fee_fen(&block_hash);

    let (timestamp_ms, timestamp_query_failed) = match maybe_block_timestamp_ms(ts_key, &block_hash)
    {
        Ok(v) => (v, false),
        Err(_) => (None, true),
    };

    let logs = block
        .get("block")
        .and_then(|b| b.get("header"))
        .and_then(|h| h.get("digest"))
        .and_then(|d| d.get("logs"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let author = author_from_pow_digest_logs(&logs).unwrap_or_else(|| "未知".to_string());

    Ok(ProcessedBlock {
        height,
        hash: block_hash,
        timestamp_ms,
        fee_fen,
        fee_query_failures,
        timestamp_query_failed,
        reward_fen: block_reward_fen_by_height(height),
        author,
    })
}

fn reset_cache_for_chain(
    cache: &mut MiningComputationCache,
    chain_hash: String,
    local_miner_account_id: Option<&str>,
) {
    *cache = MiningComputationCache::default();
    cache.chain_genesis_hash = Some(chain_hash);
    cache.tracked_miner_account_id = local_miner_account_id.map(|v| v.to_ascii_lowercase());
}

fn prune_old_income_days(cache: &mut MiningComputationCache, today_utc: u64) {
    let min_day = today_utc.saturating_sub(INCOME_DAY_KEEP);
    cache.income_by_utc_day.retain(|day, _| *day >= min_day);
}

fn refresh_cache(
    app: &AppHandle,
    finalized_height: u64,
    today_utc: u64,
    local_miner_account_id: Option<&str>,
) -> Result<RefreshStats, String> {
    // 采用“工作副本 -> 整体提交”的方式，避免处理中途失败时把半成品写回全局缓存。
    let mut working = {
        let cache = lock_or_reset(mining_cache(), "MINING_CACHE");
        cache.clone()
    };
    let normalized_local_miner = local_miner_account_id.map(|v| v.to_ascii_lowercase());
    let local_miner = normalized_local_miner.as_deref();
    let mut stats = RefreshStats {
        local_miner_missing: local_miner.is_none(),
        ..RefreshStats::default()
    };
    let mut cache_changed = false;

    let chain_hash = chain_genesis_hash()?;
    if working.chain_genesis_hash.as_deref() != Some(chain_hash.as_str()) {
        reset_cache_for_chain(&mut working, chain_hash.clone(), local_miner);
        cache_changed = true;
    }

    if working.tracked_miner_account_id.as_deref() != local_miner {
        reset_cache_for_chain(&mut working, chain_hash.clone(), local_miner);
        cache_changed = true;
    }

    if working.last_processed_height > finalized_height {
        reset_cache_for_chain(&mut working, chain_hash.clone(), local_miner);
        cache_changed = true;
    }

    if working.last_processed_height > 0 {
        let current_last_hash = match block_hash_by_height(working.last_processed_height) {
            Ok(v) => v,
            Err(err) => {
                let _ = commit_working_cache(app, &working, cache_changed, true, 0);
                return Err(err);
            }
        };
        if working.last_processed_hash.as_deref() != Some(current_last_hash.as_str()) {
            reset_cache_for_chain(&mut working, chain_hash.clone(), local_miner);
            cache_changed = true;
        }
    }

    // 挖矿收益、手续费分成、最近出块记录都属于金额展示，
    // 只处理 finalized 高度内的区块，避免 best 头回滚时收益数字先变后退。
    let ts_key = timestamp_now_storage_key();
    let target_height = cmp::min(
        finalized_height,
        working
            .last_processed_height
            .saturating_add(MAX_BLOCKS_PER_REFRESH),
    );
    let start_height = working.last_processed_height.saturating_add(1);

    if start_height <= target_height {
        for n in start_height..=target_height {
            let block = match process_block(n, &ts_key) {
                Ok(v) => v,
                Err(err) => {
                    prune_old_income_days(&mut working, today_utc);
                    let _ = commit_working_cache(app, &working, cache_changed, true, 0);
                    return Err(format!("处理区块 {n} 失败：{err}"));
                }
            };
            let is_local_author = local_miner
                .map(|local| block.author.eq_ignore_ascii_case(local))
                .unwrap_or(false);

            if is_local_author {
                // 挖矿页要显示“矿工实际到账收益”，
                // 未绑定奖励账户时手续费份额会被销毁，因此这里只在区块当时已绑定时计入 80% 分成。
                let fee_income_fen = if block.fee_fen == 0 {
                    0
                } else {
                    match reward_account_bound_at_block(&block.author, &block.hash) {
                        Ok(true) => fullnode_fee_income_fen(block.fee_fen),
                        Ok(false) => 0,
                        Err(_) => {
                            stats.fee_query_failures = stats.fee_query_failures.saturating_add(1);
                            0
                        }
                    }
                };

                working.total_fee_fen = working.total_fee_fen.saturating_add(fee_income_fen);
                working.total_reward_fen =
                    working.total_reward_fen.saturating_add(block.reward_fen);

                if let Some(ms) = block.timestamp_ms {
                    let day = utc_day(ms);
                    let income = fee_income_fen.saturating_add(block.reward_fen);
                    let entry = working.income_by_utc_day.entry(day).or_insert(0);
                    *entry = entry.saturating_add(income);
                }

                working.recent_records.push_front(CachedBlockRecord {
                    block_height: block.height,
                    timestamp_ms: block.timestamp_ms,
                    fee_fen: fee_income_fen,
                    block_reward_fen: block.reward_fen,
                    author: block.author.clone(),
                });
                while working.recent_records.len() > RECENT_RECORD_LIMIT {
                    let _ = working.recent_records.pop_back();
                }
            }

            stats.fee_query_failures = stats
                .fee_query_failures
                .saturating_add(block.fee_query_failures as u64);
            if block.timestamp_query_failed {
                stats.timestamp_query_failures = stats.timestamp_query_failures.saturating_add(1);
            }
            stats.processed_blocks = stats.processed_blocks.saturating_add(1);
            cache_changed = true;

            working.last_processed_height = block.height;
            working.last_processed_hash = Some(block.hash);
        }
    }

    stats.pending_blocks = finalized_height.saturating_sub(target_height);
    prune_old_income_days(&mut working, today_utc);

    if commit_working_cache(app, &working, cache_changed, false, stats.pending_blocks) {
        stats.persist_failed = true;
    }
    Ok(stats)
}

fn merge_warnings(items: Vec<String>) -> Option<String> {
    let merged: Vec<String> = items
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if merged.is_empty() {
        None
    } else {
        Some(merged.join("；"))
    }
}

fn warning_from_stats(stats: &RefreshStats) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if stats.fee_query_failures > 0 {
        parts.push(format!(
            "{} 个区块的手续费收益统计失败",
            stats.fee_query_failures
        ));
    }
    if stats.timestamp_query_failures > 0 {
        parts.push(format!(
            "{} 个区块时间戳读取失败",
            stats.timestamp_query_failures
        ));
    }
    if stats.pending_blocks > 0 {
        parts.push(format!(
            "区块追赶中，剩余 {} 个区块将在后续刷新中继续统计",
            stats.pending_blocks
        ));
    }
    if stats.local_miner_missing {
        parts.push("未识别本节点矿工账号，收益仅在识别后开始统计".to_string());
    }
    if stats.persist_failed {
        parts.push("本地缓存落盘失败，重启后可能需要重新追赶统计".to_string());
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!("统计数据部分不完整：{}", parts.join("；")))
    }
}

// 资源采样与收益统计解耦，即使 RPC 不可用，也尽量继续返回本地资源面板。
fn dashboard_from_cache(
    cache: &MiningComputationCache,
    warning: Option<String>,
    today_utc: u64,
) -> MiningDashboard {
    let today_income_fen = cache
        .income_by_utc_day
        .get(&today_utc)
        .copied()
        .unwrap_or(0);
    let records: Vec<MiningBlockRecord> = cache
        .recent_records
        .iter()
        .map(|row| MiningBlockRecord {
            block_height: row.block_height,
            timestamp_ms: row.timestamp_ms,
            fee: format_2_decimals_fen(row.fee_fen),
            block_reward: format_2_decimals_fen(row.block_reward_fen),
            author: hex_author_to_ss58(&row.author),
        })
        .collect();

    let total_income_fen = cache.total_fee_fen.saturating_add(cache.total_reward_fen);

    MiningDashboard {
        income: MiningIncome {
            total_income: format_2_decimals_fen(total_income_fen),
            total_fee_income: format_2_decimals_fen(cache.total_fee_fen),
            total_reward_income: format_2_decimals_fen(cache.total_reward_fen),
            today_income: format_2_decimals_fen(today_income_fen),
        },
        records,
        warning,
    }
}

fn empty_dashboard(warning: Option<String>) -> MiningDashboard {
    MiningDashboard {
        income: MiningIncome {
            total_income: "0.00".to_string(),
            total_fee_income: "0.00".to_string(),
            total_reward_income: "0.00".to_string(),
            today_income: "0.00".to_string(),
        },
        records: vec![],
        warning,
    }
}

// 同一时刻只允许一个请求做增量追块，其余请求直接复用最近缓存，避免重复扫链。
fn try_begin_refresh() -> bool {
    let mut refreshing = lock_or_reset(mining_refreshing_flag(), "MINING_REFRESHING");
    if *refreshing {
        false
    } else {
        *refreshing = true;
        true
    }
}

struct RefreshInFlightGuard;

impl Drop for RefreshInFlightGuard {
    fn drop(&mut self) {
        let mut refreshing = lock_or_reset(mining_refreshing_flag(), "MINING_REFRESHING");
        *refreshing = false;
    }
}

#[tauri::command]
pub fn get_mining_dashboard(app: AppHandle) -> Result<MiningDashboard, String> {
    // 先加载最近一次落盘缓存，保证 RPC 短暂异常时仍能返回可展示的旧数据。
    ensure_mining_cache_loaded(&app);
    let today_utc = utc_day(unix_now_ms().unwrap_or(0));
    let mut warnings: Vec<String> = Vec::new();

    if !home::current_status(&app)?.running {
        return Ok(empty_dashboard(None));
    }

    if let Err(err) = ensure_expected_rpc_node() {
        let warning = format!("挖矿统计不可用：{err}");
        eprintln!("{warning}");
        return Ok(empty_dashboard(Some(warning)));
    }

    let finalized_height = match finalized_block_height() {
        Ok(v) => v,
        Err(err) => {
            let warning = format!("读取 finalized 区块高度失败：{err}");
            eprintln!("{warning}");
            return Ok(empty_dashboard(Some(warning)));
        }
    };

    let local_miner_account_id = match reward_account::local_powr_miner_account_id(&app) {
        Ok(v) => v,
        Err(err) => {
            warnings.push(format!("读取本节点矿工账号失败：{err}"));
            let cache = lock_or_reset(mining_cache(), "MINING_CACHE");
            return Ok(dashboard_from_cache(
                &cache,
                merge_warnings(warnings),
                today_utc,
            ));
        }
    };

    if !try_begin_refresh() {
        warnings.push("挖矿统计刷新进行中，返回最近缓存".to_string());
        let cache = lock_or_reset(mining_cache(), "MINING_CACHE");
        return Ok(dashboard_from_cache(
            &cache,
            merge_warnings(warnings),
            today_utc,
        ));
    }
    let _refresh_guard = RefreshInFlightGuard;

    let warning_from_refresh = match refresh_cache(
        &app,
        finalized_height,
        today_utc,
        local_miner_account_id.as_deref(),
    ) {
        Ok(stats) => warning_from_stats(&stats),
        Err(err) => {
            let warning = format!("刷新挖矿统计失败，返回最近缓存：{err}");
            eprintln!("{warning}");
            warnings.push(warning);
            let cache = lock_or_reset(mining_cache(), "MINING_CACHE");
            return Ok(dashboard_from_cache(
                &cache,
                merge_warnings(warnings),
                today_utc,
            ));
        }
    };
    if let Some(w) = warning_from_refresh {
        warnings.push(w);
    }

    let cache = lock_or_reset(mining_cache(), "MINING_CACHE");
    Ok(dashboard_from_cache(
        &cache,
        merge_warnings(warnings),
        today_utc,
    ))
}

#[cfg(test)]
mod tests {
    use super::MiningComputationCache;

    #[test]
    fn mining_cache_rejects_unknown_field() {
        let current = serde_json::to_value(MiningComputationCache::default()).unwrap();

        let mut stale = current;
        stale["unexpected_field"] = serde_json::json!(true);
        assert!(serde_json::from_value::<MiningComputationCache>(stale).is_err());
    }
}
