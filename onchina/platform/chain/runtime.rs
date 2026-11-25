//! 读取可信公民链Runtime、机构和治理状态，提供强类型链数据适配。

use codec::{Decode, DecodeAll};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, hash::Hasher, sync::OnceLock};
use subxt::backend::legacy::LegacyRpcMethods;
use subxt::{dynamic, OnlineClient, PolkadotConfig};
use twox_hash::XxHash64;

use crate::authentication::login::parse_account_id_bytes;

// 机构操作(登记/创建/治理/自定义账户关闭)统一为「任职管理员使用签名钱包直接冷签一笔普通 extrinsic」,
// 由链端在 origin 处按机构 CID、岗位码和人员CID当前绑定三者鉴权，OnChina 后端不签发链上凭证,
// 也不持有任何平台签名钥。
static CHAIN_GENESIS_HASH: OnceLock<[u8; 32]> = OnceLock::new();
const TRUSTED_PRODUCTION_CHAINS: &[TrustedProductionChain] = &[
    // 正式链创世哈希在这里做源码级白名单绑定；新增正式链时只允许在此处追加。
    // TrustedProductionChain { name: "mainnet", genesis_hash_hex: "0x<正式链创世哈希>" },
];

#[derive(Debug, Clone, Copy)]
struct TrustedProductionChain {
    name: &'static str,
    genesis_hash_hex: &'static str,
}

fn is_production_mode() -> bool {
    std::env::var("ONCHINA_ENV")
        .ok()
        .map(|v| v.eq_ignore_ascii_case("prod") || v.eq_ignore_ascii_case("production"))
        .unwrap_or(false)
}

pub fn normalize_account_id(account_id: &str) -> Option<String> {
    crate::crypto::pubkey::normalize_account_id(account_id)
}

/// 返回已经通过启动校验缓存的链创世哈希。
pub(crate) fn cached_chain_genesis_hash_hex() -> Option<String> {
    CHAIN_GENESIS_HASH
        .get()
        .map(|hash| format!("0x{}", hex::encode(hash)))
}

fn trusted_production_chain_by_hash(
    hash: &[u8; 32],
) -> Result<Option<TrustedProductionChain>, String> {
    for chain in TRUSTED_PRODUCTION_CHAINS {
        let parsed = parse_hex_hash32(chain.genesis_hash_hex).map_err(|_| {
            format!(
                "trusted production chain `{}` has invalid genesis hash literal",
                chain.name
            )
        })?;
        if &parsed == hash {
            return Ok(Some(*chain));
        }
    }
    Ok(None)
}

async fn fetch_chain_genesis_hash_from_rpc() -> Result<[u8; 32], String> {
    if let Ok(http_url) = super::chain_url::chain_http_url() {
        return fetch_chain_genesis_hash_via_http(http_url.as_str()).await;
    }
    let ws_url = super::chain_url::chain_ws_url()?;
    fetch_chain_genesis_hash_via_ws(ws_url.as_str()).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChainGetBlockHashResponse {
    result: Option<String>,
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChainRpcValueResponse {
    id: u64,
    result: Option<serde_json::Value>,
    error: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub(crate) struct ChainFinalizedAnchor {
    pub(crate) block_hash: String,
    pub(crate) block_number: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChainHeaderResponse {
    result: Option<ChainHeader>,
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChainHeader {
    number: String,
}

async fn fetch_chain_genesis_hash_via_http(http_url: &str) -> Result<[u8; 32], String> {
    let client = super::chain_url::rpc_http_client()?;
    let response = client
        .post(http_url)
        .json(&serde_json::json!({
            "id": 1,
            "jsonrpc": "2.0",
            "method": "chain_getBlockHash",
            "params": [0]
        }))
        .send()
        .await
        .map_err(|e| format!("connect chain http rpc for genesis hash failed: {e}"))?;
    let status = response.status();
    let payload: ChainGetBlockHashResponse = response
        .json()
        .await
        .map_err(|e| format!("decode chain http rpc genesis hash response failed: {e}"))?;
    if !status.is_success() {
        return Err(format!("chain http rpc returned status {status}"));
    }
    if let Some(error) = payload.error {
        return Err(format!("chain http rpc returned error: {error}"));
    }
    let Some(hash_hex) = payload.result else {
        return Err("chain http rpc missing result for genesis hash".to_string());
    };
    parse_hex_hash32(hash_hex.as_str())
}

async fn fetch_chain_genesis_hash_via_ws(ws_url: &str) -> Result<[u8; 32], String> {
    let client = super::chain_url::online_client(ws_url)
        .await
        .map_err(|e| format!("connect chain websocket for genesis hash failed: {e}"))?;
    Ok(client.genesis_hash().0)
}

async fn fetch_finalized_head_via_http(
    client: &reqwest::Client,
    http_url: &str,
) -> Result<String, String> {
    let response = client
        .post(http_url)
        .json(&serde_json::json!({
            "id": 1,
            "jsonrpc": "2.0",
            "method": "chain_getFinalizedHead",
            "params": []
        }))
        .send()
        .await
        .map_err(|e| format!("connect chain http rpc for finalized head failed: {e}"))?;
    let status = response.status();
    let payload: ChainGetBlockHashResponse = response
        .json()
        .await
        .map_err(|e| format!("decode chain http rpc finalized head response failed: {e}"))?;
    if !status.is_success() {
        return Err(format!("chain http rpc returned status {status}"));
    }
    if let Some(error) = payload.error {
        return Err(format!("chain http rpc returned error: {error}"));
    }
    payload
        .result
        .ok_or_else(|| "chain http rpc missing finalized head result".to_string())
}

async fn fetch_header_via_http(
    client: &reqwest::Client,
    http_url: &str,
    block_hash: &str,
) -> Result<ChainHeader, String> {
    let response = client
        .post(http_url)
        .json(&serde_json::json!({
            "id": 1,
            "jsonrpc": "2.0",
            "method": "chain_getHeader",
            "params": [block_hash]
        }))
        .send()
        .await
        .map_err(|e| format!("connect chain http rpc for header failed: {e}"))?;
    let status = response.status();
    let payload: ChainHeaderResponse = response
        .json()
        .await
        .map_err(|e| format!("decode chain http rpc header response failed: {e}"))?;
    if !status.is_success() {
        return Err(format!("chain http rpc returned status {status}"));
    }
    if let Some(error) = payload.error {
        return Err(format!("chain http rpc returned error: {error}"));
    }
    payload
        .result
        .ok_or_else(|| "chain http rpc missing header result".to_string())
}

fn parse_header_number(raw: &str) -> Result<i64, String> {
    let hex = raw
        .strip_prefix("0x")
        .or_else(|| raw.strip_prefix("0X"))
        .ok_or_else(|| "chain header number must be hex".to_string())?;
    i64::from_str_radix(hex, 16).map_err(|e| format!("parse chain header number failed: {e}"))
}

/// 读取当前 finalized head 作为链投影版本锚点。
pub(crate) async fn fetch_finalized_anchor() -> Result<ChainFinalizedAnchor, String> {
    let http_url = super::chain_url::chain_http_url()?;
    let client = super::chain_url::rpc_http_client()?;
    let block_hash = fetch_finalized_head_via_http(&client, http_url.as_str()).await?;
    let header = fetch_header_via_http(&client, http_url.as_str(), block_hash.as_str()).await?;
    let block_number = parse_header_number(header.number.as_str())?;
    Ok(ChainFinalizedAnchor {
        block_hash,
        block_number,
    })
}

fn twox_128(input: &[u8]) -> [u8; 16] {
    let mut h0 = XxHash64::with_seed(0);
    h0.write(input);
    let r0 = h0.finish();

    let mut h1 = XxHash64::with_seed(1);
    h1.write(input);
    let r1 = h1.finish();

    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&r0.to_le_bytes());
    out[8..].copy_from_slice(&r1.to_le_bytes());
    out
}

fn twox_64(input: &[u8]) -> [u8; 8] {
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(input);
    hasher.finish().to_le_bytes()
}

fn storage_value_key(pallet: &[u8], item: &[u8]) -> Vec<u8> {
    let mut key = Vec::with_capacity(32);
    key.extend_from_slice(&twox_128(pallet));
    key.extend_from_slice(&twox_128(item));
    key
}

fn twox64_concat_storage_map_key(pallet: &[u8], item: &[u8], encoded_key: &[u8]) -> Vec<u8> {
    let mut key = storage_value_key(pallet, item);
    key.extend_from_slice(&twox_64(encoded_key));
    key.extend_from_slice(encoded_key);
    key
}

/// finalized `Square` 平台价格快照。价格单位固定为分，CID 和价格均不落本地副本。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformMembershipSnapshot {
    pub block_hash: String,
    pub platform_cid_number: Option<String>,
    pub freedom_price_fen: Option<u128>,
    pub democracy_price_fen: Option<u128>,
    pub spark_price_fen: Option<u128>,
}

fn decode_scale_u128(storage_hex: &str) -> Result<u128, String> {
    let clean = storage_hex
        .strip_prefix("0x")
        .or_else(|| storage_hex.strip_prefix("0X"))
        .unwrap_or(storage_hex);
    let bytes = hex::decode(clean).map_err(|e| format!("decode storage hex failed: {e}"))?;
    if bytes.len() != 16 {
        return Err("platform price storage must be exactly 16 bytes".to_string());
    }
    let mut value = [0_u8; 16];
    value.copy_from_slice(&bytes);
    Ok(u128::from_le_bytes(value))
}

/// 一次批量 RPC 读取同一 finalized 区块的公民链基金会 CID 与三档平台价格。
///
/// 不读取 best head，不使用 PostgreSQL 缓存；任一 RPC 错误由上层 fail-closed 处理。
pub async fn fetch_platform_membership_snapshot() -> Result<PlatformMembershipSnapshot, String> {
    let http_url = super::chain_url::chain_http_url()?;
    let client = super::chain_url::rpc_http_client()?;
    let block_hash = fetch_finalized_head_via_http(&client, http_url.as_str()).await?;
    // 平台机构 CID 为创世固定常量，不再从链上存储读取；仅批量读取三档 finalized 价格。
    let keys = [
        twox64_concat_storage_map_key(b"Square", b"PlatformPrice", &[0]),
        twox64_concat_storage_map_key(b"Square", b"PlatformPrice", &[1]),
        twox64_concat_storage_map_key(b"Square", b"PlatformPrice", &[2]),
    ];
    let requests = keys
        .iter()
        .enumerate()
        .map(|(index, key)| {
            serde_json::json!({
                "id": (index + 1) as u64,
                "jsonrpc": "2.0",
                "method": "state_getStorage",
                "params": [format!("0x{}", hex::encode(key)), block_hash.clone()]
            })
        })
        .collect::<Vec<_>>();
    let response = client
        .post(http_url)
        .json(&requests)
        .send()
        .await
        .map_err(|e| format!("connect chain http rpc for platform membership failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "chain http rpc returned status {}",
            response.status()
        ));
    }
    let payload = response
        .json::<Vec<ChainRpcValueResponse>>()
        .await
        .map_err(|e| format!("decode platform membership RPC response failed: {e}"))?;
    let mut values = BTreeMap::<u64, Option<String>>::new();
    for item in payload {
        if let Some(error) = item.error {
            return Err(format!("chain http rpc returned error: {error}"));
        }
        let value = match item.result {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(value)) => Some(value),
            Some(_) => return Err("platform membership storage result is not hex".to_string()),
        };
        values.insert(item.id, value);
    }
    let decode_price = |id: u64, values: &BTreeMap<u64, Option<String>>| {
        values
            .get(&id)
            .cloned()
            .flatten()
            .map(|value| decode_scale_u128(&value))
            .transpose()
    };
    Ok(PlatformMembershipSnapshot {
        block_hash,
        // 平台机构永久固定为公民链基金会，CID 单源自创世常量，不读链上存储。
        platform_cid_number: Some(
            primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION
                .cid_number
                .to_string(),
        ),
        freedom_price_fen: decode_price(1, &values)?,
        democracy_price_fen: decode_price(2, &values)?,
        spark_price_fen: decode_price(3, &values)?,
    })
}

fn system_account_storage_key(account_id: &[u8; 32]) -> String {
    let pallet_hash = twox_128(b"System");
    let storage_hash = twox_128(b"Account");
    let account_hash = sp_crypto_hashing::blake2_128(account_id);
    let mut key = Vec::with_capacity(16 + 16 + 16 + 32);
    key.extend_from_slice(&pallet_hash);
    key.extend_from_slice(&storage_hash);
    key.extend_from_slice(&account_hash);
    key.extend_from_slice(account_id);
    format!("0x{}", hex::encode(key))
}

fn decode_account_free_balance_fen(storage_hex: &str) -> Result<Option<String>, String> {
    let clean = storage_hex
        .strip_prefix("0x")
        .or_else(|| storage_hex.strip_prefix("0X"))
        .unwrap_or(storage_hex);
    let data = hex::decode(clean).map_err(|e| format!("decode System.Account hex failed: {e}"))?;
    if data.len() < 32 {
        return Ok(None);
    }
    // System.Account AccountInfo 前 16 字节为 nonce/consumers/providers/sufficients,
    // AccountData.free 是随后 16 字节 little-endian u128,单位为分。
    let mut free = [0_u8; 16];
    free.copy_from_slice(&data[16..32]);
    Ok(Some(u128::from_le_bytes(free).to_string()))
}

/// 批量读取账户 finalized free 余额，返回 key 为规范 `account_id`。
///
/// 管理员卡片只展示链上真实余额;查询失败或账户不存在时返回 None,
/// 由 UI 保留“余额”标签但不渲染余额值。0 余额是有效值,必须返回 Some("0")。
pub async fn fetch_account_balances_onchain(
    account_ids: &[String],
) -> Result<BTreeMap<String, Option<String>>, String> {
    let mut result = BTreeMap::new();
    let mut unique_accounts: BTreeMap<String, [u8; 32]> = BTreeMap::new();
    for account_id in account_ids {
        let Some(account_id) = normalize_account_id(account_id) else {
            continue;
        };
        result.entry(account_id.clone()).or_insert(None);
        if let Some(account_bytes) = parse_account_id_bytes(&account_id) {
            unique_accounts.insert(account_id, account_bytes);
        }
    }
    if unique_accounts.is_empty() {
        return Ok(result);
    }

    let http_url = super::chain_url::chain_http_url()?;
    let client = super::chain_url::rpc_http_client()?;
    let finalized_hash = fetch_finalized_head_via_http(&client, http_url.as_str()).await?;
    let mut id_to_account = BTreeMap::new();
    let requests = unique_accounts
        .iter()
        .enumerate()
        .map(|(index, (account_key, account_id))| {
            let id = (index + 1) as u64;
            id_to_account.insert(id, account_key.clone());
            serde_json::json!({
                "id": id,
                "jsonrpc": "2.0",
                "method": "state_getStorage",
                "params": [system_account_storage_key(account_id), finalized_hash.clone()]
            })
        })
        .collect::<Vec<_>>();
    let response = client
        .post(http_url.as_str())
        .json(&requests)
        .send()
        .await
        .map_err(|e| format!("connect chain http rpc for account balances failed: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("chain http rpc returned status {status}"));
    }
    let payload = response
        .json::<Vec<ChainRpcValueResponse>>()
        .await
        .map_err(|e| format!("decode chain http rpc account balance response failed: {e}"))?;

    for item in payload {
        let Some(account_id) = id_to_account.get(&item.id) else {
            continue;
        };
        if item.error.is_some() {
            result.insert(account_id.clone(), None);
            continue;
        }
        let balance = match item.result {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(storage_hex)) => {
                decode_account_free_balance_fen(storage_hex.as_str()).unwrap_or(None)
            }
            Some(_) => None,
        };
        result.insert(account_id.clone(), balance);
    }
    Ok(result)
}

/// 启动时从区块链 RPC 获取创世哈希并缓存。
/// 调用一次后，之后的 resolve_chain_genesis_hash() 直接返回缓存值。
pub(crate) async fn init_genesis_hash_from_chain() -> Result<(), String> {
    if CHAIN_GENESIS_HASH.get().is_some() {
        return Ok(());
    }
    if is_production_mode() {
        if TRUSTED_PRODUCTION_CHAINS.is_empty() {
            return Err(
                "production trusted chain whitelist is empty: add chain genesis hashes to TRUSTED_PRODUCTION_CHAINS"
                    .to_string(),
            );
        }
        let hash_bytes = fetch_chain_genesis_hash_from_rpc().await?;
        let Some(chain) = trusted_production_chain_by_hash(&hash_bytes)? else {
            return Err(format!(
                "connected chain genesis hash 0x{} is not in TRUSTED_PRODUCTION_CHAINS",
                hex::encode(hash_bytes)
            ));
        };
        let _ = CHAIN_GENESIS_HASH.set(hash_bytes);
        tracing::info!(
            trusted_chain = chain.name,
            genesis_hash = %format!("0x{}", hex::encode(hash_bytes)),
            "validated production chain genesis hash from RPC"
        );
        return Ok(());
    }

    // 开发环境允许本地显式覆盖，否则启动时自动从链上获取。
    if let Ok(raw) = std::env::var("ONCHAIN_GENESIS_HASH") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            let parsed = parse_hex_hash32(trimmed)
                .map_err(|_| "ONCHAIN_GENESIS_HASH must be 32-byte hex".to_string())?;
            let _ = CHAIN_GENESIS_HASH.set(parsed);
            tracing::info!(
                genesis_hash = %format!("0x{}", hex::encode(parsed)),
                "loaded development genesis hash from environment"
            );
            return Ok(());
        }
    }
    let hash_bytes = fetch_chain_genesis_hash_from_rpc().await?;
    let _ = CHAIN_GENESIS_HASH.set(hash_bytes);
    tracing::info!(
        genesis_hash = %format!("0x{}", hex::encode(hash_bytes)),
        "fetched development chain genesis hash from RPC"
    );
    Ok(())
}

fn parse_hex_hash32(raw: &str) -> Result<[u8; 32], String> {
    let trimmed = raw.trim();
    let no_prefix = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if no_prefix.len() != 64 || !no_prefix.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("invalid hash hex".to_string());
    }
    let bytes = hex::decode(no_prefix).map_err(|_| "invalid hash hex".to_string())?;
    let arr: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| "invalid hash length".to_string())?;
    Ok(arr)
}

// 链上管理员集合读取(去中心化鉴权)
//
const FRG_CODE: [u8; 4] = *b"FRG\0";
pub(crate) const PERSONAL_MULTISIG_LOGIN_UNSUPPORTED: &str =
    "personal multisig is not supported by OnChina";

/// 授权查询的所有数据均固定在同一个最终块。
pub(crate) struct FinalizedChainView {
    pub(crate) client: OnlineClient<PolkadotConfig>,
    pub(crate) block_hash: subxt::utils::H256,
    pub(crate) block_number: u32,
}

/// 固定区块的存储与实际执行元数据必须同行，禁止用候选模块名定位运行链。
pub(crate) struct ChainStorage {
    storage: subxt::storage::Storage<PolkadotConfig, OnlineClient<PolkadotConfig>>,
    metadata: subxt::Metadata,
}
impl ChainStorage {
    pub(crate) fn new(client: &OnlineClient<PolkadotConfig>, hash: subxt::utils::H256) -> Self {
        Self { storage: client.storage().at(hash), metadata: client.metadata() }
    }
    pub(crate) fn pallet_name(&self, index: u8) -> Result<String, String> {
        runtime_pallet_name(&self.metadata, index)
    }
}
impl std::ops::Deref for ChainStorage {
    type Target = subxt::storage::Storage<PolkadotConfig, OnlineClient<PolkadotConfig>>;
    fn deref(&self) -> &Self::Target { &self.storage }
}

/// 模块编号是链调用合同；名称只从本次执行元数据获取，缺失时明确失败。
fn runtime_pallet_name(metadata: &subxt::Metadata, index: u8) -> Result<String, String> {
    metadata.pallet_by_index(index).map(|pallet| pallet.name().to_owned())
        .ok_or_else(|| format!("runtime metadata missing pallet index {index}"))
}

/// 状态由父块上的Runtime执行产生，升级块不能提前使用新Runtime的物理命名空间。
pub(crate) async fn pin_state_metadata(
    client: &OnlineClient<PolkadotConfig>,
    rpc: &LegacyRpcMethods<PolkadotConfig>,
    hash: subxt::utils::H256,
) -> Result<(), String> {
    let header = rpc.chain_get_header(Some(hash)).await.map_err(|e| e.to_string())?
        .ok_or("state header missing")?;
    let writer = if header.number == 0 { hash } else { header.parent_hash };
    let bytes = rpc.state_get_metadata(Some(writer)).await.map_err(|e| e.to_string())?.into_raw();
    let mut input = bytes.as_slice();
    let metadata = subxt::Metadata::decode(&mut input).map_err(|e| e.to_string())?;
    if !input.is_empty() { return Err("runtime metadata has trailing bytes".into()); }
    client.set_metadata(metadata);
    Ok(())
}

impl FinalizedChainView {
    pub(crate) async fn connect() -> Result<Self, String> {
        Self::connect_at(None).await
    }

    pub(crate) async fn connect_at(block_hash: Option<[u8; 32]>) -> Result<Self, String> {
        let ws_url = super::chain_url::chain_ws_url()?;
        let rpc_client = super::chain_url::rpc_client(ws_url.as_str())
            .await
            .map_err(|e| format!("connect chain rpc for finalized admin view failed: {e}"))?;
        let rpc = LegacyRpcMethods::<PolkadotConfig>::new(rpc_client.clone());
        let client = OnlineClient::<PolkadotConfig>::from_rpc_client(rpc_client)
            .await
            .map_err(|e| format!("connect chain client for finalized admin view failed: {e}"))?;
        let block_hash = match block_hash {
            Some(hash) => subxt::utils::H256(hash),
            None => rpc
                .chain_get_finalized_head()
                .await
                .map_err(|e| format!("fetch finalized head for admin view failed: {e}"))?,
        };
        let block_number = rpc
            .chain_get_header(Some(block_hash))
            .await
            .map_err(|e| format!("fetch finalized header for admin view failed: {e}"))?
            .ok_or_else(|| "finalized header for admin view is missing".to_string())?
            .number;
        pin_state_metadata(&client, &rpc, block_hash).await?;
        Ok(Self {
            client,
            block_hash,
            block_number,
        })
    }

    pub fn storage(&self) -> ChainStorage {
        ChainStorage::new(&self.client, self.block_hash)
    }

    /// 岗位任期按同一 finalized 区块的链时间判定，禁止使用 OnChina 主机本地时钟。
    pub(crate) async fn current_day(&self) -> Result<u32, String> {
        let value = self
            .storage()
            .fetch(&dynamic::storage("Timestamp", "Now", Vec::new()))
            .await
            .map_err(|e| format!("fetch finalized Timestamp.Now for admin view failed: {e}"))?;
        let millis = match value {
            Some(value) => {
                let mut encoded = value.encoded();
                let millis = u64::decode(&mut encoded)
                    .map_err(|e| format!("decode finalized Timestamp.Now failed: {e}"))?;
                if !encoded.is_empty() {
                    return Err("finalized Timestamp.Now has trailing bytes".to_string());
                }
                millis
            }
            None if self.block_number == 0 => 0,
            None => return Err("finalized Timestamp.Now missing outside genesis".to_string()),
        };
        u32::try_from(millis / 86_400_000)
            .map_err(|_| "finalized chain day is outside u32 range".to_string())
    }
}

/// 实体的真实存储命名空间，不是独立钱包或人员名单。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityPallet {
    Public,
    Private,
}
impl EntityPallet {
    pub(crate) fn pallet_name(self, metadata: &subxt::Metadata) -> Result<String, String> {
        runtime_pallet_name(metadata, match self { Self::Public => 30, Self::Private => 31 })
    }
    pub(crate) fn module_tag(self) -> &'static [u8] {
        match self {
            Self::Public => b"pub-mgmt",
            Self::Private => b"pri-mgmt",
        }
    }
}
pub struct NodeInstitutionIdentity {
    pub entity_pallets: Vec<EntityPallet>,
    pub cid_number: String,
}
#[derive(Debug, Clone)]
pub(crate) struct ActiveMembership {
    pub(crate) institution_code: [u8; 4],
    pub(crate) cid_number: String,
    pub(crate) person_cid: String,
    pub(crate) role_code: String,
    pub(crate) role_name: String,
    pub(crate) town_code: Vec<u8>,
    pub(crate) frg_province_code: Option<[u8; 2]>,
}
impl ActiveMembership {
    pub fn candidate_id(&self) -> String {
        format!("{}:{}", self.cid_number, self.role_code)
    }
    pub(crate) fn frg_province_code_hex(&self) -> Option<String> {
        self.frg_province_code
            .map(|c| format!("0x{}", hex::encode(c)))
    }
}
fn console_entity_pallets(code: &[u8; 4]) -> Result<Vec<EntityPallet>, String> {
    use primitives::cid::code::*;
    if let Some(reason) = console_login_block_reason(code) {
        return Err(reason.into());
    }
    if is_fixed_governance_code(code) || is_public_legal_code(code) {
        Ok(vec![EntityPallet::Public])
    } else if is_private_legal_code(code) {
        Ok(vec![EntityPallet::Private])
    } else if is_unincorporated_code(code) {
        Ok(vec![EntityPallet::Public, EntityPallet::Private])
    } else {
        Err("unsupported institution code".into())
    }
}

fn console_login_block_reason(code: &[u8; 4]) -> Option<&'static str> {
    use primitives::cid::code::is_personal_code;

    if is_personal_code(code) {
        return Some(PERSONAL_MULTISIG_LOGIN_UNSUPPORTED);
    }
    None
}

pub fn identity_from_binding_parts(
    institution_code: &str,
    institution_cid_number: Option<&str>,
) -> Result<NodeInstitutionIdentity, String> {
    let code = primitives::cid::code::institution_code_from_str(institution_code)
        .ok_or_else(|| "binding institution_code is invalid".to_string())?;
    let entity_pallets = console_entity_pallets(&code)?;
    let cid_number = institution_cid_number
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "binding institution_cid_number is required".to_string())?;
    if primitives::cid::code::institution_code_from_cid_number(&cid_number) != Some(code) {
        return Err("binding institution_cid_number does not match institution_code".to_string());
    }
    Ok(NodeInstitutionIdentity {
        entity_pallets,
        cid_number,
    })
}

/// 机构码字节转 3/4 字符文本(供会话/DTO 存储)。
pub(crate) fn institution_code_label(code: &[u8; 4]) -> String {
    primitives::cid::code::institution_code_text(code)
        .unwrap_or("")
        .to_string()
}

/// 机构码行政层级标签(NATIONAL/PROVINCE/CITY/TOWN);私权法人/非法人无层级返回 None。
pub(crate) fn admin_level_label(code: &[u8; 4]) -> Option<String> {
    use primitives::cid::code::AdminLevel;
    primitives::cid::code::admin_level(code).map(|level| {
        match level {
            AdminLevel::National => "NATIONAL",
            AdminLevel::Province => "PROVINCE",
            AdminLevel::City => "CITY",
            AdminLevel::Town => "TOWN",
        }
        .to_string()
    })
}

/// 由机构码文本派生行政层级标签(供 DTO 构造)。无法解析或无层级返回 None。
pub(crate) fn admin_level_label_for(institution_code: &str) -> Option<String> {
    let bytes = primitives::cid::code::institution_code_from_str(institution_code)?;
    admin_level_label(&bytes)
}

/// Tier1 创世注册局机构码(本期 = 联邦注册局)。控制台注册局码单源,谓词与 SQL bind 共用,
/// 取代散落各处的 `"FRG"` 字面(谓词单点除外)。
pub const TIER1_REGISTRY_CODE: &str = "FRG";

/// Tier2 下级注册局机构码(本期 = 市注册局),由 Tier1 供给。控制台注册局码单源,
/// 取代散落各处的 `"CREG"` 字面。
pub(crate) const TIER2_REGISTRY_CODE: &str = "CREG";

/// 控制台注册局分层单点谓词:Tier1 = 创世注册局(本期 = 联邦注册局 FRG)。
///
/// 取代散落各处的 `institution_code == "FRG"` 字面。FRG 的 `admin_level` 虽为
/// `National`(链端铁律不可改),但其管理员按省分区(每节点单省),故控制台据此谓词
/// 单点矫正为省级分层 / 治理边界,而非全国。
pub fn is_tier1_registry(institution_code: &str) -> bool {
    institution_code == TIER1_REGISTRY_CODE
}

/// 控制台注册局分层单点谓词:Tier2 = 下级注册局(本期 = 市注册局 CREG),由 Tier1 供给。
///
/// 取代散落各处的 `institution_code == "CREG"` 字面。
pub fn is_subordinate_registry(institution_code: &str) -> bool {
    institution_code == TIER2_REGISTRY_CODE
}

/// 省名 → 链上省码([u8;2]),单源 `primitives::cid::code::PROVINCE_CODE_INFOS`。
///
/// 此为链上 `ProvinceCode`(FRG 省级组 storage 键),与 china.sqlite 行政区编码
/// (`crate::codes::china::province_code_by_name`)是两套不同口径,勿混用。
pub fn chain_province_code_by_name(province_name: &str) -> Option<[u8; 2]> {
    let trimmed = province_name.trim();
    primitives::cid::code::PROVINCE_CODE_INFOS
        .iter()
        .find(|info| info.province_name == trimmed)
        .map(|info| info.province_code)
}

pub(crate) fn chain_province_name_by_code(province_code: [u8; 2]) -> Option<String> {
    primitives::cid::code::PROVINCE_CODE_INFOS
        .iter()
        .find(|info| info.province_code == province_code)
        .map(|info| info.province_name.to_string())
}

/// 解出 `Blake2_128Concat<CidNumber>` storage key 中的 CID。
pub struct OnChainInstitution {
    pub cid_full_name: Vec<u8>,
    pub cid_short_name: Vec<u8>,
    pub town_code: Vec<u8>,
    pub legal_representative: Option<OnChainLegalRepresentative>,
    pub institution_code: [u8; 4],
}

/// 链上法定代表人公开信息；人的姓名字段全仓只使用姓、名。
pub struct OnChainLegalRepresentative {
    pub family_name: Vec<u8>,
    pub given_name: Vec<u8>,
    pub cid_number: Vec<u8>,
    pub account_id: Option<[u8; 32]>,
}

/// 链上公权机构账户投影。真源为 `PublicEntity::InstitutionAccounts`。
pub struct OnChainInstitutionAccount {
    pub cid_number: Vec<u8>,
    pub account_name: Vec<u8>,
    pub account_id: [u8; 32],
}

/// 与 publicentity `InstitutionInfo` 字段序一致的最小解码结构。
#[derive(codec::Decode)]
struct RawInstitutionInfo {
    cid_full_name: Vec<u8>,
    cid_short_name: Vec<u8>,
    town_code: Vec<u8>,
    legal_representative: Option<RawLegalRepresentative>,
    institution_code: [u8; 4],
    _created_at: u32,
}

type RawLegalRepresentative = entitybase::LegalRepresentative<Vec<u8>, Vec<u8>>;
async fn project_legal_representative(
    storage: &ChainStorage,
    value: Option<RawLegalRepresentative>,
) -> Result<Option<OnChainLegalRepresentative>, String> {
    let Some(value) = value else { return Ok(None) };
    let person =
        std::str::from_utf8(&value.person_cid).map_err(|_| "invalid representative person CID")?;
    let account_id = crate::chain_identity::read_active_cid_account_id_at(storage, person).await?;
    Ok(Some(OnChainLegalRepresentative {
        family_name: value.family_name,
        given_name: value.given_name,
        cid_number: value.person_cid,
        account_id,
    }))
}

/// 与 publicentity `InstitutionAccountInfo<AccountId, Balance, BlockNumber>` 字段序一致。
#[derive(codec::Decode)]
struct RawInstitutionAccountInfo {
    address: [u8; 32],
    _initial_balance: u128,
    _created_at: u32,
}

/// 按唯一 CID 读取链上机构；依次查询公权、私权两个实体命名空间，禁止本地猜测归属。
pub async fn institution_lookup(cid_number: &str) -> Result<Option<OnChainInstitution>, String> {
    institution_lookup_at(cid_number, None).await
}

pub async fn institution_lookup_at(
    cid_number: &str,
    block_hash: Option<[u8; 32]>,
) -> Result<Option<OnChainInstitution>, String> {
    let finalized = FinalizedChainView::connect_at(block_hash).await?;
    let storage = finalized.storage();
    let mut institution = None;
    for owner in [EntityPallet::Public, EntityPallet::Private] {
        let pallet = owner.pallet_name(&finalized.client.metadata())?;
        let query = dynamic::storage(
            pallet.clone(),
            "Institutions",
            vec![dynamic::Value::from_bytes(cid_number.as_bytes())],
        );
        let Some(value) = storage
            .fetch(&query)
            .await
            .map_err(|e| format!("fetch {pallet} institution failed: {e}"))?
        else {
            continue;
        };
        let mut raw = value.encoded();
        let info = RawInstitutionInfo::decode_all(&mut raw)
            .map_err(|e| format!("decode {pallet} institution info failed: {e}"))?;
        if institution.is_some() {
            return Err("institution exists in both namespaces".into());
        }
        institution = Some(OnChainInstitution {
            cid_full_name: info.cid_full_name,
            cid_short_name: info.cid_short_name,
            town_code: info.town_code,
            legal_representative: project_legal_representative(&storage, info.legal_representative)
                .await?,
            institution_code: info.institution_code,
        });
    }
    Ok(institution)
}

/// 全量遍历链上 `PublicEntity::Institutions`(部署验收对账用),
/// 每条回调 `(cid_number 字节, 机构信息)`,返回遍历总数。
pub async fn for_each_chain_institution(
    f: impl FnMut(Vec<u8>, OnChainInstitution),
) -> Result<usize, String> {
    let finalized = FinalizedChainView::connect().await?;
    for_each_chain_institution_at(&finalized, f).await
}

pub(crate) async fn for_each_chain_institution_at(
    finalized: &FinalizedChainView,
    mut f: impl FnMut(Vec<u8>, OnChainInstitution),
) -> Result<usize, String> {
    let storage = finalized.storage();
    let query = dynamic::storage(finalized.storage().pallet_name(30)?, "Institutions", Vec::<dynamic::Value>::new());
    let mut iter = storage
        .iter(query)
        .await
        .map_err(|e| format!("iterate institutions failed: {e}"))?;
    let mut count = 0usize;
    while let Some(item) = iter.next().await {
        let kv = item.map_err(|e| format!("read institution entry failed: {e}"))?;
        // 键 = 32 前缀 + 16 blake2_128 + SCALE(BoundedVec<u8>);取尾段解出号字节。
        let suffix = kv
            .key_bytes
            .get(48..)
            .ok_or("institution storage key too short")?;
        let mut cursor = suffix;
        let cid: Vec<u8> = codec::Decode::decode(&mut cursor)
            .map_err(|e| format!("decode institution key failed: {e}"))?;
        let mut raw = kv.value.encoded();
        let info = RawInstitutionInfo::decode_all(&mut raw)
            .map_err(|e| format!("decode institution info failed: {e}"))?;
        f(
            cid,
            OnChainInstitution {
                cid_full_name: info.cid_full_name,
                cid_short_name: info.cid_short_name,
                town_code: info.town_code,
                legal_representative: project_legal_representative(
                    &storage,
                    info.legal_representative,
                )
                .await?,
                institution_code: info.institution_code,
            },
        );
        count += 1;
    }
    Ok(count)
}

/// 全量遍历链上 `PublicEntity::InstitutionAccounts`。
///
/// 只读取链上 storage,不按本地行政区或模板派生账户;本地 PostgreSQL 仅作为投影缓存。
pub(crate) async fn for_each_chain_institution_account_at(
    finalized: &FinalizedChainView,
    mut f: impl FnMut(OnChainInstitutionAccount),
) -> Result<usize, String> {
    let storage = finalized.storage();
    let query = dynamic::storage(
        storage.pallet_name(30)?,
        "InstitutionAccounts",
        Vec::<dynamic::Value>::new(),
    );
    let mut iter = storage
        .iter(query)
        .await
        .map_err(|e| format!("iterate institution accounts failed: {e}"))?;
    let mut count = 0usize;
    while let Some(item) = iter.next().await {
        let kv = item.map_err(|e| format!("read institution account entry failed: {e}"))?;
        // 键 = 32 前缀 + 16 blake2_128 + SCALE(cid) + 16 blake2_128 + SCALE(account_name)。
        let mut suffix = kv
            .key_bytes
            .get(48..)
            .ok_or("institution account storage key too short")?;
        let cid_number: Vec<u8> = codec::Decode::decode(&mut suffix)
            .map_err(|e| format!("decode institution account cid key failed: {e}"))?;
        if suffix.len() < 16 {
            return Err("institution account key missing account_name hash suffix".to_string());
        }
        suffix = &suffix[16..];
        let account_name: Vec<u8> = codec::Decode::decode(&mut suffix)
            .map_err(|e| format!("decode institution account name key failed: {e}"))?;
        let mut raw = kv.value.encoded();
        let info = RawInstitutionAccountInfo::decode(&mut raw)
            .map_err(|e| format!("decode institution account info failed: {e}"))?;
        f(OnChainInstitutionAccount {
            cid_number,
            account_name,
            account_id: info.address,
        });
        count += 1;
    }
    Ok(count)
}

/// 按机构 CID 前缀读取该机构在链上的全部账户(协议 + 自定义)。
///
/// 真源 = `PublicEntity/PrivateEntity::InstitutionAccounts` DoubleMap,首键 = cid_number。
/// 按机构码选 pallet(私法人 → PrivateEntity / 其余 → PublicEntity),对首键做前缀迭代;
/// 键解码沿用 `for_each_chain_institution_account`:48 头(32 存储前缀 + 16 blake2_128(cid))
/// → decode cid → 跳过 16 blake2_128(account_name) → decode account_name。
pub(crate) async fn institution_accounts_lookup(
    institution_code: &[u8; 4],
    cid_number: &str,
) -> Result<Vec<OnChainInstitutionAccount>, String> {
    let finalized = FinalizedChainView::connect().await?;
    institution_accounts_lookup_at(institution_code, cid_number, &finalized).await
}

pub(crate) async fn institution_accounts_lookup_at(
    institution_code: &[u8; 4],
    cid_number: &str,
    finalized: &FinalizedChainView,
) -> Result<Vec<OnChainInstitutionAccount>, String> {
    let snapshot = crate::institutions::registration_snapshot_at(cid_number, finalized).await?;
    if snapshot.institution.institution_code != *institution_code {
        return Err("institution code mismatch".into());
    }
    let pallet = snapshot.owner.pallet_name(&finalized.client.metadata())?;
    let storage = finalized.storage();
    // 前缀迭代:首键锁定本机构 cid,只返回该机构名下账户。
    let query = dynamic::storage(
        pallet,
        "InstitutionAccounts",
        vec![dynamic::Value::from_bytes(cid_number.as_bytes())],
    );
    let mut iter = storage
        .iter(query)
        .await
        .map_err(|e| format!("iterate institution accounts failed: {e}"))?;
    let mut out = Vec::new();
    while let Some(item) = iter.next().await {
        let kv = item.map_err(|e| format!("read institution account entry failed: {e}"))?;
        // 键 = 32 存储前缀 + 16 blake2_128(cid) + SCALE(cid) + 16 blake2_128(name) + SCALE(name)。
        let mut suffix = kv
            .key_bytes
            .get(48..)
            .ok_or("institution account storage key too short")?;
        let cid: Vec<u8> = codec::Decode::decode(&mut suffix)
            .map_err(|e| format!("decode institution account cid key failed: {e}"))?;
        if suffix.len() < 16 {
            return Err("institution account key missing account_name hash suffix".to_string());
        }
        suffix = &suffix[16..];
        let account_name: Vec<u8> = codec::Decode::decode(&mut suffix)
            .map_err(|e| format!("decode institution account name key failed: {e}"))?;
        let mut raw = kv.value.encoded();
        let info = RawInstitutionAccountInfo::decode(&mut raw)
            .map_err(|e| format!("decode institution account info failed: {e}"))?;
        out.push(OnChainInstitutionAccount {
            cid_number: cid,
            account_name,
            account_id: info.address,
        });
    }
    Ok(out)
}

/// 链上公民竞选身份专属公开档案(投票身份为 None)。
pub struct OnChainCandidate {
    pub family_name: Vec<u8>,
    pub given_name: Vec<u8>,
    pub citizen_sex: u8,
    pub birth_date: u32,
    pub birth_province_code: String,
    pub birth_city_code: String,
    pub birth_town_code: String,
}

/// 链上单个公民的完整投影数据(供 registry::projection 映射为 ChainCitizen)。
pub struct OnChainCitizenDetail {
    pub cid_number: String,
    /// 居住省/市/镇码(= 归属地);优先取 VotingIdentity,回落 CidRegistry(镇为空)。
    pub residence_province_code: String,
    pub residence_city_code: String,
    pub residence_town_code: String,
    /// 绑定链账户的 raw 32 字节(未绑定为 None)。
    pub account_id: Option<[u8; 32]>,
    /// CID 当前绑定单调版本；链上已登记 CID 必须大于 0。
    pub binding_revision: u64,
    pub binding_block_number: u32,
    pub binding_block_hash: [u8; 32],
    /// citizen_status == Normal / CidRecord.status == Active。
    pub status_normal: bool,
    pub passport_valid_from: Option<u32>,
    pub passport_valid_until: Option<u32>,
    pub candidate: Option<OnChainCandidate>,
}

fn on_chain_bytes_to_string(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

/// 读链上单个公民完整身份；底层统一走同一 finalized 区块的六读闭环快照。
/// `None` = 该 CID 未占号；residence/status 优先取 VotingIdentity，回落 CidRegistry。
pub async fn read_chain_citizen_detail(
    cid_number: &str,
) -> Result<Option<OnChainCitizenDetail>, String> {
    read_chain_citizen_detail_at(cid_number, None).await
}

/// 在指定 finalized 区块读取单个公民；indexer 用事件所在块，普通查询用最新 finalized。
pub async fn read_chain_citizen_detail_at(
    cid_number: &str,
    block_hash: Option<[u8; 32]>,
) -> Result<Option<OnChainCitizenDetail>, String> {
    use crate::chain_identity::{read_citizen_identity_at, FinalizedCidStatus};

    let snapshot = read_citizen_identity_at(cid_number, block_hash).await?;
    if snapshot.cid_status == FinalizedCidStatus::Missing {
        return Ok(None);
    }
    let binding_revision = snapshot
        .binding_revision
        .ok_or_else(|| format!("CID {cid_number} binding revision missing"))?;
    let candidate = snapshot.candidate.map(|candidate| OnChainCandidate {
        family_name: candidate.family_name,
        given_name: candidate.given_name,
        citizen_sex: candidate.citizen_sex,
        birth_date: candidate.birth_date,
        birth_province_code: on_chain_bytes_to_string(candidate.birth_province_code),
        birth_city_code: on_chain_bytes_to_string(candidate.birth_city_code),
        birth_town_code: on_chain_bytes_to_string(candidate.birth_town_code),
    });

    // residence + status:优先 VotingIdentity,回落 CidRegistry(status: 0=Normal/Active)。
    let (province, city, town, status_normal, valid_from, valid_until) = match &snapshot.voting {
        Some(v) => (
            v.residence_province_code.clone(),
            v.residence_city_code.clone(),
            v.residence_town_code.clone(),
            snapshot.cid_status == FinalizedCidStatus::Active && v.citizen_status == 0,
            Some(v.passport_valid_from),
            Some(v.passport_valid_until),
        ),
        None => (
            snapshot.residence_province_code.clone(),
            snapshot.residence_city_code.clone(),
            Vec::new(),
            snapshot.cid_status == FinalizedCidStatus::Active,
            None,
            None,
        ),
    };

    Ok(Some(OnChainCitizenDetail {
        cid_number: cid_number.to_string(),
        residence_province_code: on_chain_bytes_to_string(province),
        residence_city_code: on_chain_bytes_to_string(city),
        residence_town_code: on_chain_bytes_to_string(town),
        // Revoked 记录可能仍保留链上映射供审计，但本地不得把它展示成有效当前账户。
        account_id: (snapshot.cid_status == FinalizedCidStatus::Active)
            .then_some(snapshot.account_id)
            .flatten(),
        binding_revision,
        binding_block_number: snapshot.finalized_block_number,
        binding_block_hash: snapshot.finalized_block_hash,
        status_normal,
        passport_valid_from: valid_from,
        passport_valid_until: valid_until,
        candidate,
    }))
}

/// 前缀扫描链上 `Identity::CidRegistry`,回调 residence 落在指定 (省,市) 的公民 CID。
/// 供联邦 drill-in 按市枚举本市公民(链上无按市索引,故过滤全表 residence)。
pub async fn for_each_chain_citizen_cid_in_scope(
    province_code: &str,
    city_code: &str,
    f: impl FnMut(String),
) -> Result<usize, String> {
    for_each_chain_citizen_cid_in_scope_at(province_code, city_code, None, f).await
}

pub async fn for_each_chain_citizen_cid_in_scope_at(
    province_code: &str,
    city_code: &str,
    block_hash: Option<[u8; 32]>,
    mut f: impl FnMut(String),
) -> Result<usize, String> {
    #[derive(Decode)]
    struct RawCidRecord {
        _registrar_cid_number: Vec<u8>,
        _commitment: [u8; 32],
        province: Vec<u8>,
        city: Vec<u8>,
        _status: u8,
        _registered_at: u32,
        _revoked_at: Option<u32>,
    }
    let finalized = FinalizedChainView::connect_at(block_hash).await?;
    let storage = finalized.storage();
    let query = dynamic::storage(storage.pallet_name(10)?, "CidRegistry", Vec::<dynamic::Value>::new());
    let mut iter = storage
        .iter(query)
        .await
        .map_err(|e| format!("iterate CidRegistry failed: {e}"))?;
    let mut count = 0usize;
    while let Some(item) = iter.next().await {
        let kv = item.map_err(|e| format!("read CidRegistry entry failed: {e}"))?;
        // 键 = 32 前缀 + 16 blake2_128 + SCALE(cid);取尾段解出号字节。
        let mut cursor = kv
            .key_bytes
            .get(48..)
            .ok_or("entity storage key too short")?;
        let cid: Vec<u8> = codec::Decode::decode(&mut cursor)
            .map_err(|e| format!("decode CidRegistry key failed: {e}"))?;
        let mut raw = kv.value.encoded();
        let record = RawCidRecord::decode(&mut raw)
            .map_err(|e| format!("decode CidRegistry record failed: {e}"))?;
        let cid_str = String::from_utf8(cid).map_err(|_| "citizen CID is not UTF-8")?;
        let voting = crate::chain_identity::read_voting_identity_at(&storage, &cid_str).await?;
        let (province, city) = match voting {
            Some(voting) => (voting.residence_province_code, voting.residence_city_code),
            None => (record.province, record.city),
        };
        if on_chain_bytes_to_string(province) == province_code
            && on_chain_bytes_to_string(city) == city_code
        {
            f(cid_str);
            count += 1;
        }
    }
    Ok(count)
}

/// 前缀扫描链上 `PrivateEntity::Institutions`,回调每个私权机构 CID 字符串。
/// 归属市由 CID 市码决定,过滤在调用方(或投影层按作用域再判)。
pub async fn for_each_chain_private_institution_cid(
    f: impl FnMut(String),
) -> Result<usize, String> {
    for_each_chain_private_institution_cid_at(None, f).await
}

pub async fn for_each_chain_private_institution_cid_at(
    block_hash: Option<[u8; 32]>,
    mut f: impl FnMut(String),
) -> Result<usize, String> {
    let finalized = FinalizedChainView::connect_at(block_hash).await?;
    let storage = finalized.storage();
    let query = dynamic::storage(
        storage.pallet_name(31)?,
        "Institutions",
        Vec::<dynamic::Value>::new(),
    );
    let mut iter = storage
        .iter(query)
        .await
        .map_err(|e| format!("iterate private institutions failed: {e}"))?;
    let mut count = 0usize;
    while let Some(item) = iter.next().await {
        let kv = item.map_err(|e| format!("read private institution entry failed: {e}"))?;
        let mut cursor = kv
            .key_bytes
            .get(48..)
            .ok_or("entity storage key too short")?;
        let cid: Vec<u8> = codec::Decode::decode(&mut cursor)
            .map_err(|e| format!("decode private institution key failed: {e}"))?;
        let cid_str = String::from_utf8(cid).map_err(|_| "institution CID is not UTF-8")?;
        f(cid_str);
        count += 1;
    }
    Ok(count)
}

/// 钱包先解析identity，再从实体岗位任职枚举候选；一人可有多个机构和岗位。
pub(crate) async fn find_active_memberships(
    account: &str,
) -> Result<Vec<ActiveMembership>, String> {
    let account = parse_account_id_bytes(account).ok_or("invalid signer account")?;
    let view = FinalizedChainView::connect().await?;
    let Some(person) =
        crate::chain_identity::read_active_cid_number_by_account_id_at(&view.storage(), account)
            .await?
    else {
        return Ok(Vec::new());
    };
    let person = String::from_utf8(person).map_err(|_| "person CID is not UTF-8")?;
    // 先扫描真实任职记录筛选人员，再读取命中机构；不逐一查询全量机构目录。
    let mut candidates = BTreeMap::new();
    for owner in [EntityPallet::Public, EntityPallet::Private] {
        if !crate::institutions::person_allowed(person.as_bytes(), owner) {
            continue;
        }
        let mut iter = view
            .storage()
            .iter(dynamic::storage(
                owner.pallet_name(&view.client.metadata())?,
                "InstitutionRoleAssignments",
                Vec::<dynamic::Value>::new(),
            ))
            .await
            .map_err(|e| e.to_string())?;
        while let Some(row) = iter.next().await {
            let row = row.map_err(|e| e.to_string())?;
            let rows =
                Vec::<entitybase::InstitutionAssignment<Vec<u8>, Vec<u8>, Vec<u8>>>::decode_all(
                    &mut row.value.encoded(),
                )
                .map_err(|e| e.to_string())?;
            if !rows.iter().any(|a| a.person_cid == person.as_bytes()) {
                continue;
            }
            let mut key = row.key_bytes.get(48..).ok_or("assignment key malformed")?;
            let cid = Vec::<u8>::decode(&mut key).map_err(|e| e.to_string())?;
            let mut role = key.get(16..).ok_or("assignment role key malformed")?;
            let role = Vec::<u8>::decode_all(&mut role).map_err(|e| e.to_string())?;
            if rows
                .iter()
                .any(|a| a.cid_number != cid || a.role_code != role)
            {
                return Err("assignment subject mismatch".into());
            }
            let cid = String::from_utf8(cid).map_err(|_| "institution CID malformed")?;
            if candidates
                .insert(cid, owner)
                .is_some_and(|existing| existing != owner)
            {
                return Err("institution exists in both namespaces".into());
            }
        }
    }
    let mut result = Vec::new();
    let signer = format!("0x{}", hex::encode(account));
    for (cid, owner) in candidates {
        let code = primitives::cid::code::institution_code_from_cid_number(&cid)
            .ok_or("invalid institution code")?;
        if console_login_block_reason(&code).is_some() {
            continue;
        }
        let snapshot = crate::institutions::registration_snapshot_at(&cid, &view).await?;
        if snapshot.owner != owner {
            return Err("assignment namespace mismatch".into());
        }
        let identity = NodeInstitutionIdentity {
            entity_pallets: vec![owner],
            cid_number: cid.clone(),
        };
        for a in crate::institutions::fetch_active_assignments_at(&identity, &view, Some(&signer))
            .await?
        {
            let province = if code == FRG_CODE {
                primitives::cid::code::PROVINCE_CODE_INFOS
                    .iter()
                    .find(|p| {
                        primitives::governance_skeleton::province_commissioner_role_code(
                            p.province_code,
                        ) == a.role_code.as_bytes()
                    })
                    .map(|p| p.province_code)
            } else {
                None
            };
            result.push(ActiveMembership {
                institution_code: code,
                cid_number: cid.clone(),
                person_cid: person.clone(),
                role_code: a.role_code,
                role_name: a.role_name,
                town_code: snapshot.institution.town_code.clone(),
                frg_province_code: province,
            });
        }
    }
    result.sort_by_key(|m| m.candidate_id());
    result.dedup_by_key(|m| m.candidate_id());
    Ok(result)
}

pub async fn finalized_block_hash() -> Result<[u8; 32], String> {
    Ok(FinalizedChainView::connect().await?.block_hash.0)
}

/// 当前岗位授权辖区只从机构CID、稳定岗位码和同块机构镇码派生。
pub(crate) fn role_scope(
    code: &[u8; 4],
    cid: &str,
    role: &str,
    town: &[u8],
) -> Result<(Option<String>, Option<String>, Option<String>), String> {
    if *code == FRG_CODE {
        let province = primitives::cid::code::PROVINCE_CODE_INFOS
            .iter()
            .find(|p| {
                primitives::governance_skeleton::province_commissioner_role_code(p.province_code)
                    == role.as_bytes()
            })
            .and_then(|p| chain_province_name_by_code(p.province_code));
        return Ok((province, None, None));
    }
    let (province, city) = primitives::cid::number::cid_scope_codes(cid.as_bytes())?;
    let (p, c, t) = crate::codes::china::area_display_names(
        std::str::from_utf8(&province).map_err(|_| "province invalid")?,
        Some(std::str::from_utf8(&city).map_err(|_| "city invalid")?),
        Some(std::str::from_utf8(town).map_err(|_| "town invalid")?),
    );
    Ok((
        (!p.is_empty()).then_some(p),
        (!c.is_empty()).then_some(c),
        (!t.is_empty()).then_some(t),
    ))
}

#[cfg(test)]
#[path = "../tests/chain_runtime.rs"]
mod tests;
