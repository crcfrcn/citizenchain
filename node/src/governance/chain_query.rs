// 链上状态读取统一收口(ADR-017 全端 finalized 单一口径)。
//
// (ADR-017 死规则):
// - 除交易提交管线豁免区(governance/signing.rs 的 nonce / runtime version /
//   genesis / 签名参数块 / dry-run / submit / 提交后 nonce 核对)外,
//   一切业务与展示层的链上状态读取必须钉 finalized 块哈希,禁止读 best。
// - `state_getStorage` / `state_getKeysPaged` 不带 at 参数即等于 best:
//   分叉风暴 + 跳空块 + GRANDPA 投票规则叠加时 best 视图会漂移,
//   只有 finalized 口径在全端一致,业务读取一律经本模块收口。

use serde_json::Value;

use super::signing;

/// 查询最新 finalized 区块哈希(0x + 64 位十六进制)。
///
/// 所有业务读取的钉块来源,禁止改用 chain_getHeader/best 哈希。
pub(crate) fn fetch_finalized_head() -> Result<String, String> {
    let result = signing::rpc_post("chain_getFinalizedHead", Value::Array(vec![]))?;
    match result {
        Value::String(hash) => Ok(hash),
        _ => Err("chain_getFinalizedHead 返回格式无效".to_string()),
    }
}

/// 钉 finalized 块读取单条 storage。`None` = 该 storage key 不存在。
///
/// (ADR-017):业务读取禁止 best——不带 at 参数的 `state_getStorage`
/// 读的是 best 头,分叉窗口内会看到尚未固化(可能被裁掉)的状态。
pub(crate) fn fetch_finalized_storage(key: &str) -> Result<Option<String>, String> {
    let finalized_hash = fetch_finalized_head()?;
    fetch_storage_at(key, &finalized_hash)
}

/// 在调用方指定的 finalized 哈希读取 storage，供同一聚合查询复用单一快照。
pub(crate) fn fetch_storage_at(key: &str, finalized_hash: &str) -> Result<Option<String>, String> {
    let result = signing::rpc_post(
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
pub(crate) fn fetch_finalized_keys_paged(
    prefix: &str,
    count: u32,
    start_key: Option<&str>,
) -> Result<Vec<String>, String> {
    let finalized_hash = fetch_finalized_head()?;
    fetch_keys_paged_at(prefix, count, start_key, &finalized_hash)
}

/// 在调用方指定的 finalized 哈希枚举 storage key，避免翻页期间快照漂移。
pub(crate) fn fetch_keys_paged_at(
    prefix: &str,
    count: u32,
    start_key: Option<&str>,
    finalized_hash: &str,
) -> Result<Vec<String>, String> {
    let start = match start_key {
        Some(s) => Value::String(s.to_string()),
        None => Value::Null,
    };
    let result = signing::rpc_post(
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
