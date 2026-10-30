use serde_json::Value;

/// 查询最新 finalized 区块哈希(0x + 64 位十六进制)。
///
/// 所有业务读取的钉块来源,禁止改用 chain_getHeader/best 哈希。
pub fn fetch_finalized_head() -> Result<String, String> {
    let result = crate::signing::rpc_post("chain_getFinalizedHead", Value::Array(vec![]))?;
    match result {
        Value::String(hash) => Ok(hash),
        _ => Err("chain_getFinalizedHead 返回格式无效".to_string()),
    }
}

/// 钉 finalized 块读取单条 storage。`None` = 该 storage key 不存在。
///
/// (ADR-017):业务读取禁止 best——不带 at 参数的 `state_getStorage`
/// 读的是 best 头,分叉窗口内会看到尚未固化(可能被裁掉)的状态。
pub fn fetch_finalized_storage(key: &str) -> Result<Option<String>, String> {
    let finalized_hash = fetch_finalized_head()?;
    fetch_storage_at(key, &finalized_hash)
}

/// 在调用方指定的 finalized 哈希读取 storage，供同一聚合查询复用单一快照。
pub fn fetch_storage_at(key: &str, finalized_hash: &str) -> Result<Option<String>, String> {
    let result = crate::signing::rpc_post(
        "state_getStorage",
        Value::Array(vec![
            Value::String(key.to_string()),
            Value::String(finalized_hash.to_string()),
        ]),
    )?;
    match result {
        Value::Null => Ok(None),
        Value::String(hex_data) => Ok(Some(hex_data)),
        _ => Err("state_getStorage 返回格式无效".to_string()),
    }
}

/// 钉 finalized 块列举 storage key(单页,最多 `count` 条,`start_key` 翻页)。
///
/// (ADR-017):索引扫描同样禁止 best——不带 at 参数的
/// `state_getKeysPaged` 在 best 漂移时会列出半新半旧的 key 集合。
pub fn fetch_finalized_keys_paged(
    prefix: &str,
    count: u32,
    start_key: Option<&str>,
) -> Result<Vec<String>, String> {
    let finalized_hash = fetch_finalized_head()?;
    fetch_keys_paged_at(prefix, count, start_key, &finalized_hash)
}

/// 在调用方指定的 finalized 哈希枚举 storage key，避免翻页期间快照漂移。
pub fn fetch_keys_paged_at(
    prefix: &str,
    count: u32,
    start_key: Option<&str>,
    finalized_hash: &str,
) -> Result<Vec<String>, String> {
    let start = match start_key {
        Some(s) => Value::String(s.to_string()),
        None => Value::Null,
    };
    let result = crate::signing::rpc_post(
        "state_getKeysPaged",
        Value::Array(vec![
            Value::String(prefix.to_string()),
            Value::Number(count.into()),
            start,
            Value::String(finalized_hash.to_string()),
        ]),
    )?;
    let arr = result
        .as_array()
        .ok_or_else(|| "state_getKeysPaged 返回非数组".to_string())?;
    Ok(arr
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect())
}

/// 构造查询账户余额的存储 key：`System::Account(account_id)`。
/// `account_id` 必须是小写 `0x` + 64 位十六进制。
pub fn system_account_key(account_id: &str) -> Result<String, String> {
    let account_id = nodedesktop::validation::normalize_account_id(account_id)?;
    let account_bytes = hex::decode(account_id.trim_start_matches("0x"))
        .map_err(|e| format!("解析账户 ID 失败: {e}"))?;
    if account_bytes.len() != 32 {
        return Err(format!(
            "账户 ID 长度必须为 32 字节，实际: {}",
            account_bytes.len()
        ));
    }
    Ok(nodedesktop::storage::to_hex(
        &nodedesktop::storage::blake2_map(b"System", b"Account", &account_bytes),
    ))
}

/// 构造无 map key 的存储 value key：twox_128(pallet) + twox_128(storage)。
/// 用于查询 NextProposalId 等 StorageValue。
pub fn value_key(pallet: &str, storage: &str) -> String {
    nodedesktop::storage::to_hex(&nodedesktop::storage::prefix(
        pallet.as_bytes(),
        storage.as_bytes(),
    ))
}

/// 构造 StorageMap key：twox_128(pallet) + twox_128(storage) + blake2_128_concat(key_data)。
/// blake2_128_concat = blake2_128(data) + data。
pub fn map_key(pallet: &str, storage: &str, key_data: &[u8]) -> String {
    nodedesktop::storage::to_hex(&nodedesktop::storage::blake2_map(
        pallet.as_bytes(),
        storage.as_bytes(),
        key_data,
    ))
}

/// 构造 DoubleMap key：twox_128(pallet) + twox_128(storage)
///   + blake2_128_concat(key1) + blake2_128_concat(key2)。
pub fn double_map_key(pallet: &str, storage: &str, key1: &[u8], key2: &[u8]) -> String {
    nodedesktop::storage::to_hex(&nodedesktop::storage::blake2_double_map(
        pallet.as_bytes(),
        storage.as_bytes(),
        key1,
        key2,
    ))
}

/// 构造 `StorageDoubleMap<_, Twox64Concat, K1, Twox64Concat, K2, _>` 的
/// **前缀**(只到第一层 K1,不含第二层 K2),用于 `state_getKeysPaged` 列举:
///   twox_128(pallet) + twox_128(storage) + twox_64(K1) + K1
///
/// 对应 votebase 的 `ProposalsByCode / ProposalsByCid / ByOwner / ByYear`。
/// 4 张反向索引的列举前缀。
pub fn twox64_concat_prefix(pallet: &str, storage: &str, key1: &[u8]) -> String {
    nodedesktop::storage::to_hex(&nodedesktop::storage::twox64_map(
        pallet.as_bytes(),
        storage.as_bytes(),
        key1,
    ))
}
#[cfg(test)]
#[path = "../tests/contract.rs"]
mod tests;
