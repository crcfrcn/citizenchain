use nodedesktop::constants::RPC_RESPONSE_LIMIT_SMALL;
use serde_json::Value;
use std::time::Duration;

const RPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

fn rpc_post(method: &str, params: Value) -> Result<Value, String> {
    nodedesktop::rpc::rpc_post(
        method,
        params,
        RPC_REQUEST_TIMEOUT,
        RPC_RESPONSE_LIMIT_SMALL,
    )
}

/// 查询指定机构的管理员账户及其全部有效岗位任职。
pub(crate) fn fetch_institution_admins(
    cid_number: &str,
) -> Result<Vec<crate::admins::InstitutionAdminInfo>, String> {
    crate::storage::fetch_institution_admins_by_cid_number(cid_number)
}

/// 查询 finalized 块上的账户总余额（free + reserved，单位为分）。
pub fn fetch_balance(account_id: &str) -> Result<Option<u128>, String> {
    // (ADR-017):钉块哈希统一取自 chain_query 收口,业务读取禁止 best。
    let hash = nodeclient::query::fetch_finalized_head()?;
    fetch_balance_at(account_id, Some(&hash))
}

/// 查询指定 finalized 块上的账户总余额；机构、管理员及钱包共用此口径。
pub(crate) fn fetch_balance_at(
    account_id: &str,
    block_hash: Option<&str>,
) -> Result<Option<u128>, String> {
    fetch_account_at(account_id, block_hash)?
        .as_ref()
        .map(nodeclient::codec::account_total)
        .transpose()
}

/// 转账校验独立读取可支配余额，禁止使用展示总余额判断能否支出。
pub fn fetch_spendable_balance(account_id: &str) -> Result<Option<u128>, String> {
    let hash = nodeclient::query::fetch_finalized_head()?;
    Ok(fetch_account_at(account_id, Some(&hash))?
        .as_ref()
        .map(spendable_balance))
}

/// 与 Balances 的 Preserve + Polite 口径一致：保留金、冻结/保留资金不能支出。
fn spendable_balance(account: &nodeclient::codec::AccountInfo) -> u128 {
    let data = &account.data;
    let untouchable = data
        .frozen
        .saturating_sub(data.reserved)
        .max(primitives::core_const::ACCOUNT_EXISTENTIAL_DEPOSIT);
    data.free.saturating_sub(untouchable)
}

fn fetch_account_at(
    account_id: &str,
    block_hash: Option<&str>,
) -> Result<Option<nodeclient::codec::AccountInfo>, String> {
    let storage_key = nodeclient::query::system_account_key(account_id)?;
    let mut params = vec![Value::String(storage_key)];
    if let Some(hash) = block_hash {
        params.push(Value::String(hash.to_string()));
    }
    let result = rpc_post("state_getStorage", Value::Array(params))?;

    match result {
        Value::Null => Ok(None),
        Value::String(hex_data) => {
            let data = nodeclient::codec::decode_hex_storage(&hex_data)?;
            nodeclient::codec::decode_account(&data).map(Some)
        }
        _ => Err("state_getStorage 返回格式无效".to_string()),
    }
}

#[cfg(test)]
#[path = "../tests/balance.rs"]
mod tests;
