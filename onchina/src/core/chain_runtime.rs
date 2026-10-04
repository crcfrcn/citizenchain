use codec::Decode;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, hash::Hasher, sync::OnceLock};
use subxt::backend::legacy::LegacyRpcMethods;
use subxt::backend::rpc::RpcClient;
use subxt::{dynamic, OnlineClient, PolkadotConfig};
use twox_hash::XxHash64;

use crate::auth::login::parse_account_id_bytes;

// 机构操作(登记/创建/治理/自定义账户关闭)统一为「任职管理员使用签名钱包直接冷签一笔普通 extrinsic」,
// 由链端在 origin 处按机构 CID、岗位码和管理员账户 ID 三者鉴权，OnChina 后端不签发链上凭证,
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

pub(crate) fn normalize_account_id(account_id: &str) -> Option<String> {
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
struct ChainGetBlockHashResponse {
    result: Option<String>,
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
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
    let client = OnlineClient::<PolkadotConfig>::from_insecure_url(ws_url)
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

/// finalized `SquarePost` 平台价格快照。价格单位固定为分，CID 和价格均不落本地副本。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct PlatformMembershipSnapshot {
    pub(crate) block_hash: String,
    pub(crate) platform_cid_number: Option<String>,
    pub(crate) freedom_price_fen: Option<u128>,
    pub(crate) democracy_price_fen: Option<u128>,
    pub(crate) spark_price_fen: Option<u128>,
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
pub(crate) async fn fetch_platform_membership_snapshot(
) -> Result<PlatformMembershipSnapshot, String> {
    let http_url = super::chain_url::chain_http_url()?;
    let client = super::chain_url::rpc_http_client()?;
    let block_hash = fetch_finalized_head_via_http(&client, http_url.as_str()).await?;
    // 平台机构 CID 为创世固定常量，不再从链上存储读取；仅批量读取三档 finalized 价格。
    let keys = [
        twox64_concat_storage_map_key(b"SquarePost", b"PlatformPrice", &[0]),
        twox64_concat_storage_map_key(b"SquarePost", b"PlatformPrice", &[1]),
        twox64_concat_storage_map_key(b"SquarePost", b"PlatformPrice", &[2]),
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
pub(crate) async fn fetch_account_balances_onchain(
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

fn parse_hex_2(raw: &str) -> Result<[u8; 2], String> {
    let trimmed = raw.trim();
    let no_prefix = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if no_prefix.len() != 4 || !no_prefix.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("invalid 2-byte hex".to_string());
    }
    let bytes = hex::decode(no_prefix).map_err(|_| "invalid 2-byte hex".to_string())?;
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| "invalid 2-byte length".to_string())
}

// 链上管理员集合读取(去中心化鉴权)
//
// 真源:机构 Active 管理员集合落链端两个机构 pallet 的 `AdminAccounts` storage——
// `PublicAdmins`(公权法人,含固定治理档 NRC/PRC/PRB/NJD/FRG)、
// `PrivateAdmins`(私权法人:股权/股份/有限合伙/公益/协会/私立学校等)。
// 节点按自身机构码路由到对应 pallet,登录验签后比对该集合放行,
// 本地 admins 表仅作展示元数据缓存，不保存管理员省权限。个人多签 PMUL 不在控制台范围。
/// 联邦注册局机构码,镜像 `admin_primitives::FRG`(`*b"FRG\0"`)。
/// onchina 不依赖 admin-primitives(避免引入 frame-support 重依赖),
/// 此处单字面镜像;FRG 为稳定常量,与链端保持一致即可。
const FRG_CODE: [u8; 4] = *b"FRG\0";
/// 国家司法院机构码。NJD 虽属固定治理档,但按产品边界进入 OnChina 控制台。
const NJD_CODE: [u8; 4] = *b"NJD\0";
pub(crate) const DESKTOP_GOVERNANCE_LOGIN_UNSUPPORTED: &str =
    "desktop governance institution is not supported by OnChina";
pub(crate) const PERSONAL_MULTISIG_LOGIN_UNSUPPORTED: &str =
    "personal multisig is not supported by OnChina";

/// 公私权管理员解码后统一为只读视图；原始 SCALE 布局仍直接复用 runtime 共享类型。
struct OnChainAdminRecord {
    account_id: [u8; 32],
    cid_number: Vec<u8>,
    family_name: Vec<u8>,
    given_name: Vec<u8>,
}

/// 管理员名册与当前签名账户的同区块解析结果。
///
/// `role_assignment_account_id` 保留链上岗位任职使用的名册锚点；`account_id` 是管理员
/// CID 在同一个 finalized 区块上的当前绑定账户，也是 OnChina 唯一接受的签名账户。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedOnChainAdminRecord {
    role_assignment_account_id: [u8; 32],
    account_id: [u8; 32],
    cid_number: Vec<u8>,
    family_name: Vec<u8>,
    given_name: Vec<u8>,
}

struct OnChainAdminAccount {
    institution_code: [u8; 4],
    admins: Vec<OnChainAdminRecord>,
}

/// 提供给 OnChina 鉴权、目录和页面的链上管理员人员记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OnChainAdmin {
    pub(crate) account_id: String,
    pub(crate) role_assignment_account_id: [u8; 32],
    pub(crate) cid_number: String,
    pub(crate) family_name: String,
    pub(crate) given_name: String,
}

fn decode_onchain_admin_account(
    raw: &[u8],
    pallet: AdminPallet,
) -> Result<OnChainAdminAccount, String> {
    let mut input = raw;
    let (institution_code, admins): ([u8; 4], Vec<OnChainAdminRecord>) = match pallet {
        AdminPallet::PublicAdmins => {
            type Raw = admin_primitives::InstitutionAdmins<Vec<admin_primitives::Admin<[u8; 32]>>>;
            let decoded = Raw::decode(&mut input)
                .map_err(|e| format!("decode PublicInstitutionAdmins failed: {e}"))?;
            let admins = decoded
                .admins
                .into_iter()
                .map(|admin| OnChainAdminRecord {
                    account_id: admin.account_id,
                    cid_number: admin.cid_number.into_inner(),
                    family_name: admin.family_name.into_inner(),
                    given_name: admin.given_name.into_inner(),
                })
                .collect();
            (decoded.institution_code, admins)
        }
        AdminPallet::PrivateAdmins => {
            type Raw = admin_primitives::InstitutionAdmins<Vec<admin_primitives::Admin<[u8; 32]>>>;
            let decoded = Raw::decode(&mut input)
                .map_err(|e| format!("decode PrivateInstitutionAdmins failed: {e}"))?;
            let admins = decoded
                .admins
                .into_iter()
                .map(|admin| OnChainAdminRecord {
                    account_id: admin.account_id,
                    cid_number: admin.cid_number.into_inner(),
                    family_name: admin.family_name.into_inner(),
                    given_name: admin.given_name.into_inner(),
                })
                .collect();
            (decoded.institution_code, admins)
        }
    };
    if !input.is_empty() {
        return Err("InstitutionAdmins has trailing bytes".to_string());
    }
    let mut seen = std::collections::BTreeSet::new();
    for admin in &admins {
        if !seen.insert(admin.account_id) {
            return Err("InstitutionAdmins contains duplicate account_id".to_string());
        }
        std::str::from_utf8(admin.family_name.as_slice())
            .map_err(|_| "InstitutionAdmins family_name is not UTF-8".to_string())?;
        std::str::from_utf8(admin.given_name.as_slice())
            .map_err(|_| "InstitutionAdmins given_name is not UTF-8".to_string())?;
        std::str::from_utf8(admin.cid_number.as_slice())
            .map_err(|_| "InstitutionAdmins cid_number is not UTF-8".to_string())?;
    }
    Ok(OnChainAdminAccount {
        institution_code,
        admins,
    })
}

/// 一次 OnChina 授权读取固定到一个 finalized 区块，禁止管理员名册、CID 绑定与岗位
/// 分别读取不同高度。
pub(crate) struct FinalizedChainView {
    pub(crate) client: OnlineClient<PolkadotConfig>,
    pub(crate) block_hash: subxt::utils::H256,
    block_number: u32,
}

impl FinalizedChainView {
    pub(crate) async fn connect() -> Result<Self, String> {
        let ws_url = super::chain_url::chain_ws_url()?;
        let rpc_client = RpcClient::from_insecure_url(ws_url.as_str())
            .await
            .map_err(|e| format!("connect chain rpc for finalized admin view failed: {e}"))?;
        let rpc = LegacyRpcMethods::<PolkadotConfig>::new(rpc_client.clone());
        let client = OnlineClient::<PolkadotConfig>::from_rpc_client(rpc_client)
            .await
            .map_err(|e| format!("connect chain client for finalized admin view failed: {e}"))?;
        let block_hash = rpc
            .chain_get_finalized_head()
            .await
            .map_err(|e| format!("fetch finalized head for admin view failed: {e}"))?;
        let block_number = rpc
            .chain_get_header(Some(block_hash))
            .await
            .map_err(|e| format!("fetch finalized header for admin view failed: {e}"))?
            .ok_or_else(|| "finalized header for admin view is missing".to_string())?
            .number;
        Ok(Self {
            client,
            block_hash,
            block_number,
        })
    }

    pub(crate) fn storage(
        &self,
    ) -> subxt::storage::Storage<PolkadotConfig, OnlineClient<PolkadotConfig>> {
        self.client.storage().at(self.block_hash)
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

/// 机构 Active 管理员集合所属链上 pallet。
///
/// 机构码决定容器:`PublicAdmins` 收公权法人和固定治理档,`PrivateAdmins` 收私权法人。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdminPallet {
    PublicAdmins,
    PrivateAdmins,
}

impl AdminPallet {
    /// construct_runtime 中的 pallet 名(subxt dynamic storage 寻址用)。
    pub(crate) fn pallet_name(self) -> &'static str {
        match self {
            AdminPallet::PublicAdmins => "PublicAdmins",
            AdminPallet::PrivateAdmins => "PrivateAdmins",
        }
    }
}

/// 本节点已绑定的链上机构身份(由首次 active admin 登录确认后落库)。
pub(crate) struct NodeInstitutionIdentity {
    /// 本机构 Active 管理员集合的候选 pallet;非法人为 [Public, Private] 按序探测。
    pub(crate) admin_pallets: Vec<AdminPallet>,
    /// 本机构 CID 号。提案归属/订阅统一按 CID,机构码只用于分类。
    pub(crate) cid_number: String,
    /// 联邦注册局专用:本节点所辖省的链上省码([u8;2]);其它机构为 `None`。
    ///
    /// 联邦注册局节点辖省。管理员集合仍是唯一 FRG `AdminAccounts`，省界由 entity 岗位表达。
    pub(crate) frg_province_code: Option<[u8; 2]>,
}

#[derive(Debug, Clone)]
pub(crate) struct ActiveAdminMembership {
    pub(crate) institution_code: [u8; 4],
    pub(crate) cid_number: String,
    pub(crate) frg_province_code: Option<[u8; 2]>,
}

impl ActiveAdminMembership {
    pub(crate) fn candidate_id(&self) -> String {
        let code = institution_code_label(&self.institution_code);
        if let Some(province_code) = self.frg_province_code {
            return format!("FRG:{}:{}", code, hex::encode(province_code));
        }
        format!("ADM:{}:{}", code, self.cid_number)
    }

    pub(crate) fn frg_province_code_hex(&self) -> Option<String> {
        self.frg_province_code
            .map(|code| format!("0x{}", hex::encode(code)))
    }
}

/// 机构码 → 控制台准入的候选 admin pallet。
///
/// 镜像链端 `admin-primitives` 路由语义(用 `primitives::cid::code` 分类,不引入 admin-primitives
/// 重依赖):FRG→公权省级组;NJD/其它公权法人→公权;私权法人→私权;
/// 非法人按所属法人落公权或私权——账户键全局唯一,登录时按 [Public, Private] 顺序探测命中。
/// 国家储委会/省储委会/省储行走节点桌面端,个人主体/个人多签都不在控制台范围,返回错误拒绝。
fn console_admin_pallets(code: &[u8; 4]) -> Result<Vec<AdminPallet>, String> {
    use primitives::cid::code::{
        is_fixed_governance_code, is_private_legal_code, is_public_legal_code,
        is_unincorporated_code,
    };
    if *code == FRG_CODE {
        return Ok(vec![AdminPallet::PublicAdmins]);
    }
    if *code == NJD_CODE {
        return Ok(vec![AdminPallet::PublicAdmins]);
    }
    if let Some(reason) = console_login_block_reason(code) {
        return Err(reason.to_string());
    }
    if is_fixed_governance_code(code) {
        return Err("fixed-governance institution is not managed by this console".to_string());
    }
    if is_public_legal_code(code) {
        return Ok(vec![AdminPallet::PublicAdmins]);
    }
    if is_private_legal_code(code) {
        return Ok(vec![AdminPallet::PrivateAdmins]);
    }
    if is_unincorporated_code(code) {
        return Ok(vec![AdminPallet::PublicAdmins, AdminPallet::PrivateAdmins]);
    }
    Err("node institution code is not a console-managed institution".to_string())
}

fn console_login_block_reason(code: &[u8; 4]) -> Option<&'static str> {
    use primitives::cid::code::{is_personal_code, NRC, PRB, PRC};
    if matches!(*code, NRC | PRC | PRB) {
        return Some(DESKTOP_GOVERNANCE_LOGIN_UNSUPPORTED);
    }
    if is_personal_code(code) {
        return Some(PERSONAL_MULTISIG_LOGIN_UNSUPPORTED);
    }
    None
}

/// 只有目标签名账户确实属于该链上管理员名册时，才返回控制台边界拒绝原因。
///
/// 管理员反查会遍历全部名册；若在确认成员资格前记录拒绝原因，任意陌生账户都会因链上
/// 存在节点桌面治理机构而被误报为其管理员。
fn console_login_block_reason_for_membership(
    code: &[u8; 4],
    membership_matched: bool,
) -> Option<&'static str> {
    membership_matched
        .then(|| console_login_block_reason(code))
        .flatten()
}

pub(crate) fn identity_from_binding_parts(
    institution_code: &str,
    institution_cid_number: Option<&str>,
    frg_province_code: Option<&str>,
) -> Result<NodeInstitutionIdentity, String> {
    let code = primitives::cid::code::institution_code_from_str(institution_code)
        .ok_or_else(|| "binding institution_code is invalid".to_string())?;
    let admin_pallets = console_admin_pallets(&code)?;
    let frg_code = frg_province_code
        .map(parse_hex_2)
        .transpose()
        .map_err(|_| "binding frg_province_code must be 2-byte hex".to_string())?;
    let cid_number = institution_cid_number
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "binding institution_cid_number is required".to_string())?;
    if primitives::cid::code::institution_code_from_cid_number(&cid_number) != Some(code) {
        return Err("binding institution_cid_number does not match institution_code".to_string());
    }
    Ok(NodeInstitutionIdentity {
        admin_pallets,
        cid_number,
        frg_province_code: frg_code,
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
pub(crate) const TIER1_REGISTRY_CODE: &str = "FRG";

/// Tier2 下级注册局机构码(本期 = 市注册局),由 Tier1 供给。控制台注册局码单源,
/// 取代散落各处的 `"CREG"` 字面。
pub(crate) const TIER2_REGISTRY_CODE: &str = "CREG";

/// 控制台注册局分层单点谓词:Tier1 = 创世注册局(本期 = 联邦注册局 FRG)。
///
/// 取代散落各处的 `institution_code == "FRG"` 字面。FRG 的 `admin_level` 虽为
/// `National`(链端铁律不可改),但其管理员按省分区(每节点单省),故控制台据此谓词
/// 单点矫正为省级分层 / 治理边界,而非全国。
pub(crate) fn is_tier1_registry(institution_code: &str) -> bool {
    institution_code == TIER1_REGISTRY_CODE
}

/// 控制台注册局分层单点谓词:Tier2 = 下级注册局(本期 = 市注册局 CREG),由 Tier1 供给。
///
/// 取代散落各处的 `institution_code == "CREG"` 字面。
pub(crate) fn is_subordinate_registry(institution_code: &str) -> bool {
    institution_code == TIER2_REGISTRY_CODE
}

/// 省名 → 链上省码([u8;2]),单源 `primitives::cid::code::PROVINCE_CODE_INFOS`。
///
/// 此为链上 `ProvinceCode`(FRG 省级组 storage 键),与 china.sqlite 行政区编码
/// (`crate::cid::china::province_code_by_name`)是两套不同口径,勿混用。
pub(crate) fn chain_province_code_by_name(province_name: &str) -> Option<[u8; 2]> {
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
fn admin_accounts_cid_from_key(key_bytes: &[u8]) -> Result<Vec<u8>, String> {
    const PREFIX_AND_HASH_LEN: usize = 32 + 16;
    let encoded = key_bytes
        .get(PREFIX_AND_HASH_LEN..)
        .ok_or_else(|| "AdminAccounts storage key is too short".to_string())?;
    let mut input = encoded;
    let cid_number = Vec::<u8>::decode(&mut input)
        .map_err(|e| format!("decode AdminAccounts cid_number failed: {e}"))?;
    if !input.is_empty() {
        return Err("AdminAccounts storage key has trailing bytes".to_string());
    }
    if cid_number.is_empty()
        || cid_number.len() > primitives::core_const::CID_NUMBER_MAX_BYTES as usize
    {
        return Err("AdminAccounts cid_number length is invalid".to_string());
    }
    Ok(cid_number)
}

/// 用冷钱包签名账户反查其所属的链上 active admin 机构集合。
///
/// 这是链上中国通用平台的登录真源。平台启动时不再预设机构;
/// 已验签账户在链上哪些机构的 Active 管理员集合内,就得到哪些可绑定候选。
/// 链上公权机构登记查询结果(创世目录抽样/全量对账用,字段最小化)。
pub(crate) struct OnChainInstitution {
    pub(crate) cid_full_name: Vec<u8>,
    pub(crate) cid_short_name: Vec<u8>,
    pub(crate) town_code: Vec<u8>,
    pub(crate) legal_representative: Option<OnChainLegalRepresentative>,
    pub(crate) institution_code: [u8; 4],
}

/// 链上法定代表人公开信息；人的姓名字段全仓只使用姓、名。
pub(crate) struct OnChainLegalRepresentative {
    pub(crate) family_name: Vec<u8>,
    pub(crate) given_name: Vec<u8>,
    pub(crate) cid_number: Vec<u8>,
    pub(crate) account_id: [u8; 32],
}

/// 链上公权机构账户投影。真源为 `PublicManage::InstitutionAccounts`。
pub(crate) struct OnChainInstitutionAccount {
    pub(crate) cid_number: Vec<u8>,
    pub(crate) account_name: Vec<u8>,
    pub(crate) account_id: [u8; 32],
}

/// 与 public-manage `InstitutionInfo` 字段序一致的最小解码结构。
#[derive(codec::Decode)]
struct RawInstitutionInfo {
    cid_full_name: Vec<u8>,
    cid_short_name: Vec<u8>,
    town_code: Vec<u8>,
    legal_representative: Option<RawLegalRepresentative>,
    institution_code: [u8; 4],
    _created_at: u32,
}

/// 与 entity-primitives `LegalRepresentative` 字段序一致的最小解码结构。
#[derive(codec::Decode)]
struct RawLegalRepresentative {
    family_name: Vec<u8>,
    given_name: Vec<u8>,
    cid_number: Vec<u8>,
    account_id: [u8; 32],
}

fn project_legal_representative(
    value: Option<RawLegalRepresentative>,
) -> Option<OnChainLegalRepresentative> {
    value.map(|value| OnChainLegalRepresentative {
        family_name: value.family_name,
        given_name: value.given_name,
        cid_number: value.cid_number,
        account_id: value.account_id,
    })
}

async fn private_legal_representative_at(
    storage: &subxt::storage::Storage<PolkadotConfig, OnlineClient<PolkadotConfig>>,
    institution_cid_number: &[u8],
) -> Result<Option<RawLegalRepresentative>, String> {
    let address = dynamic::storage(
        "PrivateManage",
        "Institutions",
        vec![dynamic::Value::from_bytes(institution_cid_number)],
    );
    let Some(value) = storage
        .fetch(&address)
        .await
        .map_err(|e| format!("fetch PrivateManage institution failed: {e}"))?
    else {
        return Err("private admin roster has no matching institution".to_string());
    };
    let mut encoded = value.encoded();
    let info = RawInstitutionInfo::decode(&mut encoded)
        .map_err(|e| format!("decode PrivateManage institution info failed: {e}"))?;
    if !encoded.is_empty() {
        return Err("PrivateManage institution info has trailing bytes".to_string());
    }
    Ok(info.legal_representative)
}

/// 反查目标公民 CID 当前担任法定代表人的私权机构及其岗位关联账户。
///
/// `PrivateManage` 没有按法定代表人 CID 建二级索引，因此登录反查需要一次 finalized
/// 全表遍历；结果只保留目标 CID，避免为每个私权管理员名册逐条发起 storage 请求。
async fn private_legal_representative_accounts_for_cid(
    storage: &subxt::storage::Storage<PolkadotConfig, OnlineClient<PolkadotConfig>>,
    target_cid_number: Option<&[u8]>,
) -> Result<BTreeMap<Vec<u8>, [u8; 32]>, String> {
    let Some(target_cid_number) = target_cid_number else {
        return Ok(BTreeMap::new());
    };
    let mut matches = BTreeMap::new();
    let mut iter = storage
        .iter(dynamic::storage(
            "PrivateManage",
            "Institutions",
            Vec::<dynamic::Value>::new(),
        ))
        .await
        .map_err(|e| format!("iterate PrivateManage Institutions failed: {e}"))?;
    while let Some(item) = iter.next().await {
        let item = item.map_err(|e| format!("read PrivateManage Institutions failed: {e}"))?;
        let mut encoded = item.value.encoded();
        let info = RawInstitutionInfo::decode(&mut encoded)
            .map_err(|e| format!("decode PrivateManage institution info failed: {e}"))?;
        if !encoded.is_empty() {
            return Err("PrivateManage institution info has trailing bytes".to_string());
        }
        let Some(legal_representative) = info.legal_representative else {
            continue;
        };
        if legal_representative.cid_number.as_slice() != target_cid_number {
            continue;
        }
        const PREFIX_AND_HASH_LEN: usize = 32 + 16;
        let mut key = item
            .key_bytes
            .get(PREFIX_AND_HASH_LEN..)
            .ok_or_else(|| "PrivateManage Institutions storage key is too short".to_string())?;
        let institution_cid_number = Vec::<u8>::decode(&mut key)
            .map_err(|e| format!("decode PrivateManage institution CID failed: {e}"))?;
        if !key.is_empty() {
            return Err("PrivateManage Institutions storage key has trailing bytes".to_string());
        }
        matches.insert(institution_cid_number, legal_representative.account_id);
    }
    Ok(matches)
}

fn matching_roster_admin_by_cid_or_public_account<'a>(
    decoded: &'a OnChainAdminAccount,
    pallet: AdminPallet,
    target_account_id: &[u8; 32],
    target_cid_number: Option<&[u8]>,
) -> Option<&'a OnChainAdminRecord> {
    if let Some(target_cid_number) = target_cid_number {
        if let Some(admin) = decoded
            .admins
            .iter()
            .find(|admin| admin.cid_number.as_slice() == target_cid_number)
        {
            return Some(admin);
        }
    }
    (pallet == AdminPallet::PublicAdmins)
        .then(|| {
            decoded
                .admins
                .iter()
                .find(|admin| admin.cid_number.is_empty() && &admin.account_id == target_account_id)
        })
        .flatten()
}

async fn matching_admin_for_signer<'a>(
    storage: &subxt::storage::Storage<PolkadotConfig, OnlineClient<PolkadotConfig>>,
    decoded: &'a OnChainAdminAccount,
    pallet: AdminPallet,
    institution_cid_number: &[u8],
    target_account_id: &[u8; 32],
    target_cid_number: Option<&[u8]>,
    private_legal_representatives: &BTreeMap<Vec<u8>, [u8; 32]>,
) -> Result<Option<&'a OnChainAdminRecord>, String> {
    if let Some(admin) = matching_roster_admin_by_cid_or_public_account(
        decoded,
        pallet,
        target_account_id,
        target_cid_number,
    ) {
        return Ok(Some(admin));
    }
    if target_cid_number.is_some() && pallet == AdminPallet::PrivateAdmins {
        if let Some(role_assignment_account_id) =
            private_legal_representatives.get(institution_cid_number)
        {
            let admin = decoded.admins.iter().find(|admin| {
                &admin.account_id == role_assignment_account_id && admin.cid_number.is_empty()
            });
            if admin.is_none() {
                return Err("private legal representative is missing from admin roster".to_string());
            }
            return Ok(admin);
        }
    }
    if pallet == AdminPallet::PublicAdmins {
        return Ok(None);
    }
    let Some(account_only_admin) = decoded
        .admins
        .iter()
        .find(|admin| admin.cid_number.is_empty() && &admin.account_id == target_account_id)
    else {
        return Ok(None);
    };
    let legal_representative =
        private_legal_representative_at(storage, institution_cid_number).await?;
    if legal_representative
        .as_ref()
        .is_some_and(|value| value.account_id == account_only_admin.account_id)
    {
        // 目标仍是名册旧账户时，LR 必须继续通过其 CID 当前绑定匹配，禁止回退旧账户。
        return Ok(None);
    }
    Ok(Some(account_only_admin))
}

/// 把管理员名册的岗位关联账户解析为同一 finalized 区块上的实际签名账户。
///
/// 任何带 CID 的机构管理员、以及从机构记录取得 CID 的私权法定代表人，只认 CID 当前
/// 绑定账户；无 CID 的冻结公权管理员与私权非 LR 按名册 `account_id` 直接授权，严格
/// 镜像 runtime `resolve_admin_account`。个人多签不经过本入口。CID 已撤销或当前未绑定
/// 时，该管理员不进入有效集合，不回退旧账户。
async fn resolve_onchain_admin_records(
    storage: &subxt::storage::Storage<PolkadotConfig, OnlineClient<PolkadotConfig>>,
    decoded: OnChainAdminAccount,
    pallet: AdminPallet,
    institution_cid_number: &[u8],
) -> Result<Vec<ResolvedOnChainAdminRecord>, String> {
    use crate::core::chain_citizen_identity::read_active_cid_account_id_at;

    let legal_representative = if pallet == AdminPallet::PrivateAdmins {
        private_legal_representative_at(storage, institution_cid_number).await?
    } else {
        None
    };
    let mut resolved = Vec::with_capacity(decoded.admins.len());
    let mut effective_accounts = std::collections::BTreeSet::new();
    for admin in decoded.admins {
        let matching_legal_representative = legal_representative
            .as_ref()
            .filter(|value| value.account_id == admin.account_id);
        let (cid_number, family_name, given_name) = match matching_legal_representative {
            Some(value) => {
                if !admin.cid_number.is_empty() && admin.cid_number != value.cid_number {
                    return Err(
                        "private legal representative CID does not match admin roster".to_string(),
                    );
                }
                (
                    value.cid_number.clone(),
                    value.family_name.clone(),
                    value.given_name.clone(),
                )
            }
            None => (
                admin.cid_number.clone(),
                admin.family_name.clone(),
                admin.given_name.clone(),
            ),
        };
        let account_id = if cid_number.is_empty() {
            // 冻结公权管理员、私权非 LR 没有公民 CID，不虚构身份字段，直接使用名册账户。
            admin.account_id
        } else {
            if family_name.is_empty() || given_name.is_empty() {
                return Err("CID-bound admin family_name/given_name is empty".to_string());
            }
            let cid_number_text = std::str::from_utf8(cid_number.as_slice())
                .map_err(|_| "admin cid_number is not UTF-8".to_string())?;
            let Some(current_account_id) =
                read_active_cid_account_id_at(storage, cid_number_text).await?
            else {
                // 不存在有效当前绑定时，该 CID 管理员没有可接受的签名账户。
                continue;
            };
            current_account_id
        };
        if !effective_accounts.insert(account_id) {
            return Err("resolved admin set contains duplicate account_id".to_string());
        }
        resolved.push(ResolvedOnChainAdminRecord {
            role_assignment_account_id: admin.account_id,
            account_id,
            cid_number,
            family_name,
            given_name,
        });
    }
    Ok(resolved)
}

/// 与 public-manage `InstitutionAccountInfo<AccountId, Balance, BlockNumber>` 字段序一致。
#[derive(codec::Decode)]
struct RawInstitutionAccountInfo {
    address: [u8; 32],
    _initial_balance: u128,
    _created_at: u32,
}

/// 按唯一 CID 读取链上机构；依次查询公权、私权两个实体命名空间，禁止本地猜测归属。
pub(crate) async fn institution_lookup(
    cid_number: &str,
) -> Result<Option<OnChainInstitution>, String> {
    let ws_url = super::chain_url::chain_ws_url()?;
    let client = OnlineClient::<PolkadotConfig>::from_insecure_url(ws_url.as_str())
        .await
        .map_err(|e| format!("connect chain ws for institutions failed: {e}"))?;
    let storage = client
        .storage()
        .at_latest()
        .await
        .map_err(|e| format!("get latest chain storage failed: {e}"))?;
    for pallet in ["PublicManage", "PrivateManage"] {
        let query = dynamic::storage(
            pallet,
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
        let info = RawInstitutionInfo::decode(&mut raw)
            .map_err(|e| format!("decode {pallet} institution info failed: {e}"))?;
        return Ok(Some(OnChainInstitution {
            cid_full_name: info.cid_full_name,
            cid_short_name: info.cid_short_name,
            town_code: info.town_code,
            legal_representative: project_legal_representative(info.legal_representative),
            institution_code: info.institution_code,
        }));
    }
    Ok(None)
}

/// 全量遍历链上 `PublicManage::Institutions`(部署验收对账用),
/// 每条回调 `(cid_number 字节, 机构信息)`,返回遍历总数。
pub(crate) async fn for_each_chain_institution(
    mut f: impl FnMut(Vec<u8>, OnChainInstitution),
) -> Result<usize, String> {
    let ws_url = super::chain_url::chain_ws_url()?;
    let client = OnlineClient::<PolkadotConfig>::from_insecure_url(ws_url.as_str())
        .await
        .map_err(|e| format!("connect chain ws for institutions failed: {e}"))?;
    let storage = client
        .storage()
        .at_latest()
        .await
        .map_err(|e| format!("get latest chain storage failed: {e}"))?;
    let query = dynamic::storage("PublicManage", "Institutions", Vec::<dynamic::Value>::new());
    let mut iter = storage
        .iter(query)
        .await
        .map_err(|e| format!("iterate institutions failed: {e}"))?;
    let mut count = 0usize;
    while let Some(item) = iter.next().await {
        let kv = item.map_err(|e| format!("read institution entry failed: {e}"))?;
        // 键 = 32 前缀 + 16 blake2_128 + SCALE(BoundedVec<u8>);取尾段解出号字节。
        let suffix = &kv.key_bytes[48..];
        let mut cursor = suffix;
        let cid: Vec<u8> = codec::Decode::decode(&mut cursor)
            .map_err(|e| format!("decode institution key failed: {e}"))?;
        let mut raw = kv.value.encoded();
        let info = RawInstitutionInfo::decode(&mut raw)
            .map_err(|e| format!("decode institution info failed: {e}"))?;
        f(
            cid,
            OnChainInstitution {
                cid_full_name: info.cid_full_name,
                cid_short_name: info.cid_short_name,
                town_code: info.town_code,
                legal_representative: project_legal_representative(info.legal_representative),
                institution_code: info.institution_code,
            },
        );
        count += 1;
    }
    Ok(count)
}

/// 全量遍历链上 `PublicManage::InstitutionAccounts`。
///
/// 只读取链上 storage,不按本地行政区或模板派生账户;本地 PostgreSQL 仅作为投影缓存。
pub(crate) async fn for_each_chain_institution_account(
    mut f: impl FnMut(OnChainInstitutionAccount),
) -> Result<usize, String> {
    let ws_url = super::chain_url::chain_ws_url()?;
    let client = OnlineClient::<PolkadotConfig>::from_insecure_url(ws_url.as_str())
        .await
        .map_err(|e| format!("connect chain ws for institution accounts failed: {e}"))?;
    let storage = client
        .storage()
        .at_latest()
        .await
        .map_err(|e| format!("get latest chain storage failed: {e}"))?;
    let query = dynamic::storage(
        "PublicManage",
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
        let mut suffix = &kv.key_bytes[48..];
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
/// 真源 = `PublicManage/PrivateManage::InstitutionAccounts` DoubleMap,首键 = cid_number。
/// 按机构码选 pallet(私法人 → PrivateManage / 其余 → PublicManage),对首键做前缀迭代;
/// 键解码沿用 `for_each_chain_institution_account`:48 头(32 存储前缀 + 16 blake2_128(cid))
/// → decode cid → 跳过 16 blake2_128(account_name) → decode account_name。
pub(crate) async fn institution_accounts_lookup(
    institution_code: &[u8; 4],
    cid_number: &str,
) -> Result<Vec<OnChainInstitutionAccount>, String> {
    let pallet = if primitives::cid::code::is_private_legal_code(institution_code) {
        "PrivateManage"
    } else {
        "PublicManage"
    };
    let ws_url = super::chain_url::chain_ws_url()?;
    let client = OnlineClient::<PolkadotConfig>::from_insecure_url(ws_url.as_str())
        .await
        .map_err(|e| format!("connect chain ws for institution accounts failed: {e}"))?;
    let storage = client
        .storage()
        .at_latest()
        .await
        .map_err(|e| format!("get latest chain storage failed: {e}"))?;
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
        let mut suffix = &kv.key_bytes[48..];
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
pub(crate) struct OnChainCandidate {
    pub(crate) family_name: Vec<u8>,
    pub(crate) given_name: Vec<u8>,
    pub(crate) citizen_sex: u8,
    pub(crate) birth_date: u32,
    pub(crate) birth_province_code: String,
    pub(crate) birth_city_code: String,
    pub(crate) birth_town_code: String,
}

/// 链上单个公民的完整投影数据(供 domains::projection 映射为 ChainCitizen)。
pub(crate) struct OnChainCitizenDetail {
    pub(crate) cid_number: String,
    /// 居住省/市/镇码(= 归属地);优先取 VotingIdentity,回落 CidRegistry(镇为空)。
    pub(crate) residence_province_code: String,
    pub(crate) residence_city_code: String,
    pub(crate) residence_town_code: String,
    /// 绑定链账户的 raw 32 字节(未绑定为 None)。
    pub(crate) account_id: Option<[u8; 32]>,
    /// CID 当前绑定单调版本；链上已登记 CID 必须大于 0。
    pub(crate) binding_revision: u64,
    pub(crate) binding_finalized_block_number: u32,
    pub(crate) binding_finalized_block_hash: [u8; 32],
    /// citizen_status == Normal / CidRecord.status == Active。
    pub(crate) status_normal: bool,
    pub(crate) passport_valid_from: Option<u32>,
    pub(crate) passport_valid_until: Option<u32>,
    pub(crate) candidate: Option<OnChainCandidate>,
}

fn on_chain_bytes_to_string(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

/// 读链上单个公民完整身份；底层统一走同一 finalized 区块的六读闭环快照。
/// `None` = 该 CID 未占号；residence/status 优先取 VotingIdentity，回落 CidRegistry。
pub(crate) async fn read_chain_citizen_detail(
    cid_number: &str,
) -> Result<Option<OnChainCitizenDetail>, String> {
    read_chain_citizen_detail_at(cid_number, None).await
}

/// 在指定 finalized 区块读取单个公民；indexer 用事件所在块，普通查询用最新 finalized。
pub(crate) async fn read_chain_citizen_detail_at(
    cid_number: &str,
    block_hash: Option<[u8; 32]>,
) -> Result<Option<OnChainCitizenDetail>, String> {
    use crate::core::chain_citizen_identity::{read_citizen_identity_at, FinalizedCidStatus};

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
        binding_finalized_block_number: snapshot.finalized_block_number,
        binding_finalized_block_hash: snapshot.finalized_block_hash,
        status_normal,
        passport_valid_from: valid_from,
        passport_valid_until: valid_until,
        candidate,
    }))
}

/// 前缀扫描链上 `CitizenIdentity::CidRegistry`,回调 residence 落在指定 (省,市) 的公民 CID。
/// 供联邦 drill-in 按市枚举本市公民(链上无按市索引,故过滤全表 residence)。
pub(crate) async fn for_each_chain_citizen_cid_in_scope(
    province_code: &str,
    city_code: &str,
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
    let ws_url = super::chain_url::chain_ws_url()?;
    let client = OnlineClient::<PolkadotConfig>::from_insecure_url(ws_url.as_str())
        .await
        .map_err(|e| format!("connect chain ws for citizen scan failed: {e}"))?;
    let storage = client
        .storage()
        .at_latest()
        .await
        .map_err(|e| format!("get latest chain storage failed: {e}"))?;
    let query = dynamic::storage(
        "CitizenIdentity",
        "CidRegistry",
        Vec::<dynamic::Value>::new(),
    );
    let mut iter = storage
        .iter(query)
        .await
        .map_err(|e| format!("iterate CidRegistry failed: {e}"))?;
    let mut count = 0usize;
    while let Some(item) = iter.next().await {
        let kv = item.map_err(|e| format!("read CidRegistry entry failed: {e}"))?;
        // 键 = 32 前缀 + 16 blake2_128 + SCALE(cid);取尾段解出号字节。
        let mut cursor = &kv.key_bytes[48..];
        let cid: Vec<u8> = codec::Decode::decode(&mut cursor)
            .map_err(|e| format!("decode CidRegistry key failed: {e}"))?;
        let mut raw = kv.value.encoded();
        let record = RawCidRecord::decode(&mut raw)
            .map_err(|e| format!("decode CidRegistry record failed: {e}"))?;
        if on_chain_bytes_to_string(record.province) == province_code
            && on_chain_bytes_to_string(record.city) == city_code
        {
            if let Ok(cid_str) = String::from_utf8(cid) {
                f(cid_str);
                count += 1;
            }
        }
    }
    Ok(count)
}

/// 前缀扫描链上 `PrivateManage::Institutions`,回调每个私权机构 CID 字符串。
/// 归属市由 CID 市码决定,过滤在调用方(或投影层按作用域再判)。
pub(crate) async fn for_each_chain_private_institution_cid(
    mut f: impl FnMut(String),
) -> Result<usize, String> {
    let ws_url = super::chain_url::chain_ws_url()?;
    let client = OnlineClient::<PolkadotConfig>::from_insecure_url(ws_url.as_str())
        .await
        .map_err(|e| format!("connect chain ws for private institutions failed: {e}"))?;
    let storage = client
        .storage()
        .at_latest()
        .await
        .map_err(|e| format!("get latest chain storage failed: {e}"))?;
    let query = dynamic::storage(
        "PrivateManage",
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
        let mut cursor = &kv.key_bytes[48..];
        let cid: Vec<u8> = codec::Decode::decode(&mut cursor)
            .map_err(|e| format!("decode private institution key failed: {e}"))?;
        if let Ok(cid_str) = String::from_utf8(cid) {
            f(cid_str);
            count += 1;
        }
    }
    Ok(count)
}

/// 按当前签名 `account_id` 查找该账户所属的全部 Active 管理员机构集合。
///
/// 带 CID 的名册成员先解析到当前绑定账户，再和已验签账户匹配；换绑后新账户立即接管，
/// 名册中的旧岗位关联账户不再具有签名权。冻结公权管理员与私权非 LR 没有 CID 时，
/// 与 runtime 一致按名册 `account_id` 匹配。
pub(crate) async fn find_active_admin_memberships(
    verified_account_id: &str,
) -> Result<Vec<ActiveAdminMembership>, String> {
    let target = parse_account_id_bytes(verified_account_id).ok_or_else(|| {
        "verified_account_id must be lowercase 0x plus 64 hexadecimal characters".to_string()
    })?;
    let finalized = FinalizedChainView::connect().await?;
    let storage = finalized.storage();
    let target_cid_number =
        crate::core::chain_citizen_identity::read_active_cid_number_by_account_id_at(
            &storage, target,
        )
        .await?;
    let private_legal_representatives =
        private_legal_representative_accounts_for_cid(&storage, target_cid_number.as_deref())
            .await?;

    let mut memberships = Vec::new();
    let mut blocked_login_reason: Option<&'static str> = None;
    for pallet in [AdminPallet::PublicAdmins, AdminPallet::PrivateAdmins] {
        let query = dynamic::storage(
            pallet.pallet_name(),
            "AdminAccounts",
            Vec::<dynamic::Value>::new(),
        );
        let mut iter = storage
            .iter(query)
            .await
            .map_err(|e| format!("iterate {} AdminAccounts failed: {e}", pallet.pallet_name()))?;
        while let Some(item) = iter.next().await {
            let kv = item
                .map_err(|e| format!("read {} AdminAccounts failed: {e}", pallet.pallet_name()))?;
            let raw = kv.value.encoded();
            let decoded = decode_onchain_admin_account(raw, pallet).map_err(|e| {
                format!("decode {} AdminAccounts failed: {e}", pallet.pallet_name())
            })?;
            let institution_code = decoded.institution_code;
            let cid_number = admin_accounts_cid_from_key(&kv.key_bytes)?;
            let cid_number_text = String::from_utf8(cid_number.clone())
                .map_err(|_| "AdminAccounts cid_number is not UTF-8".to_string())?;
            if primitives::cid::code::institution_code_from_cid_number(&cid_number_text)
                != Some(institution_code)
            {
                return Err("AdminAccounts cid_number does not match institution_code".to_string());
            }
            let matched_admin = matching_admin_for_signer(
                &storage,
                &decoded,
                pallet,
                &cid_number,
                &target,
                target_cid_number.as_deref(),
                &private_legal_representatives,
            )
            .await?;
            if let Some(reason) = console_login_block_reason_for_membership(
                &institution_code,
                matched_admin.is_some(),
            ) {
                blocked_login_reason.get_or_insert(reason);
                continue;
            }
            let Some(matched_admin) = matched_admin else {
                continue;
            };
            let allowed = console_admin_pallets(&institution_code)?;
            if !allowed.contains(&pallet) {
                continue;
            }
            if institution_code == FRG_CODE {
                let province_codes =
                    crate::institution::admins::chain_roles::fetch_frg_province_codes_for_admin(
                        &finalized,
                        &cid_number,
                        matched_admin.account_id,
                    )
                    .await?;
                for province_code in province_codes {
                    memberships.push(ActiveAdminMembership {
                        institution_code: FRG_CODE,
                        cid_number: cid_number_text.clone(),
                        frg_province_code: Some(province_code),
                    });
                }
                continue;
            }
            memberships.push(ActiveAdminMembership {
                institution_code,
                cid_number: cid_number_text,
                frg_province_code: None,
            });
        }
    }

    memberships.sort_by_key(|m| m.candidate_id());
    memberships.dedup_by_key(|m| m.candidate_id());
    if memberships.is_empty() {
        if let Some(reason) = blocked_login_reason {
            return Err(reason.to_string());
        }
    }
    Ok(memberships)
}

/// 读取本节点机构的链上管理员人员集合；`account_id` 始终是当前可签名账户。
///
/// 按候选 pallet 顺序探测 `<Pallet>::AdminAccounts[cid_number]`，命中首个集合即返回。
///
/// 返回:`Ok(Some(set))`=命中 Active 集合;`Ok(None)`=不存在或非 Active;`Err`=链不可达或解码失败。
pub(crate) async fn fetch_active_admins_onchain(
    identity: &NodeInstitutionIdentity,
) -> Result<Option<Vec<OnChainAdmin>>, String> {
    let finalized = FinalizedChainView::connect().await?;
    fetch_active_admins_onchain_at(identity, &finalized).await
}

/// 在调用方固定的 finalized 区块读取管理员集合，供岗位合并复用同一快照。
pub(crate) async fn fetch_active_admins_onchain_at(
    identity: &NodeInstitutionIdentity,
    finalized: &FinalizedChainView,
) -> Result<Option<Vec<OnChainAdmin>>, String> {
    let storage = finalized.storage();

    let addresses = identity
        .admin_pallets
        .iter()
        .map(|pallet| {
            (
                *pallet,
                dynamic::storage(
                    pallet.pallet_name(),
                    "AdminAccounts",
                    vec![dynamic::Value::from_bytes(identity.cid_number.as_bytes())],
                ),
            )
        })
        .collect::<Vec<_>>();

    for (pallet, address) in &addresses {
        let Some(thunk) = storage
            .fetch(address)
            .await
            .map_err(|e| format!("fetch on-chain admin account failed: {e}"))?
        else {
            continue;
        };
        let raw = thunk.encoded();
        let decoded = decode_onchain_admin_account(raw, *pallet)
            .map_err(|e| format!("decode on-chain admin account failed: {e}"))?;
        let mut admin_records = resolve_onchain_admin_records(
            &storage,
            decoded,
            *pallet,
            identity.cid_number.as_bytes(),
        )
        .await?;
        if let Some(province_code) = identity.frg_province_code {
            let province_admins =
                crate::institution::admins::chain_roles::fetch_frg_admins_for_province(
                    finalized,
                    identity.cid_number.as_bytes(),
                    province_code,
                )
                .await?;
            admin_records
                .retain(|admin| province_admins.contains(&admin.role_assignment_account_id));
        }
        let admins = admin_records
            .into_iter()
            .map(|admin| {
                Ok(OnChainAdmin {
                    account_id: format!("0x{}", hex::encode(admin.account_id)),
                    role_assignment_account_id: admin.role_assignment_account_id,
                    cid_number: String::from_utf8(admin.cid_number)
                        .map_err(|_| "on-chain cid_number is not UTF-8".to_string())?,
                    family_name: String::from_utf8(admin.family_name)
                        .map_err(|_| "on-chain family_name is not UTF-8".to_string())?,
                    given_name: String::from_utf8(admin.given_name)
                        .map_err(|_| "on-chain given_name is not UTF-8".to_string())?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        return Ok(Some(admins));
    }
    Ok(None)
}

#[cfg(test)]
// Runtime 元数据与哈希夹具必须精确匹配，断言式解包用于暴露契约回归。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::{
        decode_scale_u128, is_production_mode, parse_hex_hash32, trusted_production_chain_by_hash,
    };

    #[test]
    fn platform_price_storage_values_decode_strictly() {
        // 平台 CID 已是创世常量，不再从链上解码；仅严格校验三档价格 u128 解码。
        let price = 123_456_u128;
        assert_eq!(
            decode_scale_u128(&format!("0x{}", hex::encode(price.to_le_bytes()))).unwrap(),
            price
        );
        assert!(decode_scale_u128("0x01").is_err());
    }

    /// 锁定公权管理员四字段 SCALE 布局；机构 CID 仍只存在于 storage key。
    #[test]
    fn onchain_institution_admin_account_decodes_unified_records_only() {
        use codec::Encode;

        let bytes = admin_primitives::InstitutionAdmins {
            institution_code: *b"CREG",
            admins: vec![admin_primitives::Admin {
                account_id: [0x42u8; 32],
                cid_number: "CN220-CTZN2-198805200-2026"
                    .as_bytes()
                    .to_vec()
                    .try_into()
                    .expect("citizen cid fits"),
                family_name: Default::default(),
                given_name: Default::default(),
            }],
        }
        .encode();
        let decoded = super::decode_onchain_admin_account(&bytes, super::AdminPallet::PublicAdmins)
            .expect("public institution admin account must decode unified layout");
        assert_eq!(decoded.institution_code, *b"CREG");
        assert_eq!(decoded.admins.len(), 1);
        assert_eq!(decoded.admins[0].account_id, [0x42; 32]);
        // 管理员 CID 由链上名册提供，钱包码无需重复声明；实际签名账户由该 CID
        // 在同一个 finalized 区块上的当前绑定决定。
        assert_eq!(
            decoded.admins[0].cid_number.as_slice(),
            "CN220-CTZN2-198805200-2026".as_bytes()
        );
        let resolved = [super::ResolvedOnChainAdminRecord {
            role_assignment_account_id: decoded.admins[0].account_id,
            account_id: [0x43; 32],
            cid_number: decoded.admins[0].cid_number.clone(),
            family_name: decoded.admins[0].family_name.clone(),
            given_name: decoded.admins[0].given_name.clone(),
        }];
        // 换绑后岗位关联仍保留名册账户，但只有 CID 当前绑定的新账户能签名。
        assert!(resolved.iter().all(|admin| admin.account_id != [0x42; 32]));
        assert!(resolved.iter().any(|admin| admin.account_id == [0x43; 32]));
        assert_eq!(resolved[0].role_assignment_account_id, [0x42; 32]);

        let old_layout = (*b"CREG", vec![[0x42u8; 32]]).encode();
        assert!(
            super::decode_onchain_admin_account(&old_layout, super::AdminPallet::PublicAdmins,)
                .is_err()
        );
    }

    #[test]
    fn private_institution_admin_account_decodes_unified_layout() {
        use codec::Encode;

        let bytes = admin_primitives::InstitutionAdmins {
            institution_code: *b"SFGY",
            admins: vec![admin_primitives::Admin {
                account_id: [0x24u8; 32],
                cid_number: "CN220-CTZN2-198805200-2026"
                    .as_bytes()
                    .to_vec()
                    .try_into()
                    .expect("citizen cid fits"),
                family_name: "程"
                    .as_bytes()
                    .to_vec()
                    .try_into()
                    .expect("family name fits"),
                given_name: "伟"
                    .as_bytes()
                    .to_vec()
                    .try_into()
                    .expect("given name fits"),
            }],
        }
        .encode();
        let decoded =
            super::decode_onchain_admin_account(&bytes, super::AdminPallet::PrivateAdmins).expect(
                "private institution admin account must decode unified account/cid/name layout",
            );
        assert_eq!(decoded.institution_code, *b"SFGY");
        assert_eq!(decoded.admins[0].account_id, [0x24; 32]);
        assert_eq!(
            decoded.admins[0].cid_number,
            "CN220-CTZN2-198805200-2026".as_bytes()
        );
        assert_eq!(decoded.admins[0].family_name, "程".as_bytes());
        assert_eq!(decoded.admins[0].given_name, "伟".as_bytes());

        let account_only = admin_primitives::InstitutionAdmins {
            institution_code: *b"SFGY",
            admins: vec![admin_primitives::Admin {
                account_id: [0x25u8; 32],
                cid_number: Default::default(),
                family_name: Default::default(),
                given_name: Default::default(),
            }],
        }
        .encode();
        let decoded_account_only =
            super::decode_onchain_admin_account(&account_only, super::AdminPallet::PrivateAdmins)
                .expect("private non-LR admin may remain account-only");
        assert!(decoded_account_only.admins[0].cid_number.is_empty());
        assert!(decoded_account_only.admins[0].family_name.is_empty());
        assert!(decoded_account_only.admins[0].given_name.is_empty());
    }

    #[test]
    fn frozen_public_admin_without_cid_uses_roster_account_only() {
        let mut decoded = super::OnChainAdminAccount {
            institution_code: *b"FRG\0",
            admins: vec![super::OnChainAdminRecord {
                account_id: [0x31; 32],
                cid_number: Vec::new(),
                family_name: Vec::new(),
                given_name: Vec::new(),
            }],
        };
        assert!(super::matching_roster_admin_by_cid_or_public_account(
            &decoded,
            super::AdminPallet::PublicAdmins,
            &[0x31; 32],
            Some(b"CN220-CTZN2-198805200-2026"),
        )
        .is_some());

        // 一旦名册已有管理员 CID，旧名册账户不得再作为无 CID 例外回退。
        decoded.admins[0].cid_number = b"CN220-CTZN2-198805200-2026".to_vec();
        assert!(super::matching_roster_admin_by_cid_or_public_account(
            &decoded,
            super::AdminPallet::PublicAdmins,
            &[0x31; 32],
            Some(b"CN221-CTZN2-198805200-2026"),
        )
        .is_none());
    }

    #[test]
    fn console_pallets_allow_njd_and_block_desktop_governance() {
        assert_eq!(
            super::console_admin_pallets(b"NJD\0").unwrap(),
            vec![super::AdminPallet::PublicAdmins]
        );

        for code in [b"NRC\0", b"PRC\0", b"PRB\0"] {
            assert_eq!(
                super::console_admin_pallets(code).unwrap_err(),
                super::DESKTOP_GOVERNANCE_LOGIN_UNSUPPORTED
            );
            // 遍历到受阻机构并不代表目标账户属于该机构；只有实际命中名册才返回边界错误。
            assert_eq!(
                super::console_login_block_reason_for_membership(code, false),
                None
            );
            assert_eq!(
                super::console_login_block_reason_for_membership(code, true),
                Some(super::DESKTOP_GOVERNANCE_LOGIN_UNSUPPORTED)
            );
        }
    }

    #[test]
    fn console_pallets_keep_unincorporated_dual_probe_and_personal_rejected() {
        assert_eq!(
            super::console_admin_pallets(b"UNIN").unwrap(),
            vec![
                super::AdminPallet::PublicAdmins,
                super::AdminPallet::PrivateAdmins
            ]
        );
        assert_eq!(
            super::console_admin_pallets(b"PMUL").unwrap_err(),
            super::PERSONAL_MULTISIG_LOGIN_UNSUPPORTED
        );
    }

    #[test]
    fn parse_hex_hash32_accepts_prefixed_hash() {
        let parsed = parse_hex_hash32(&format!("0x{}", "11".repeat(32))).unwrap();
        assert_eq!(parsed, [0x11; 32]);
    }

    #[test]
    fn trusted_production_chain_lookup_returns_none_for_unknown_hash() {
        let result = trusted_production_chain_by_hash(&[0x22; 32]).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn production_mode_detects_prod_env() {
        let previous = std::env::var("ONCHINA_ENV").ok();
        std::env::set_var("ONCHINA_ENV", "prod");
        assert!(is_production_mode());
        if let Some(value) = previous {
            std::env::set_var("ONCHINA_ENV", value);
        } else {
            std::env::remove_var("ONCHINA_ENV");
        }
    }
}
