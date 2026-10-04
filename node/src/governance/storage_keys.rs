// 治理相关 pallet RPC 查询用的存储 key String 门面。
//
// key 派生逻辑单源在 `crate::shared::storage_keys`(裸字节);本模块只做 `&str` 入参 +
// `"0x"+hex` 出参的 RPC 适配,并 re-export 原始 hasher 供少数手搓键的调用点复用。
// 格式：twox_128(pallet) + twox_128(storage) + blake2_128_concat(key)。

use crate::shared::storage_keys as skeys;

// 原始 hasher 单源出口(委托 shared → sp_crypto_hashing),保持
// `storage_keys::{twox_128,blake2_128}` 既有调用点(endpoint/institution_read/admins)不变。
pub use crate::shared::storage_keys::{blake2_128, twox_128};

/// 构造查询账户余额的存储 key：`System::Account(account_id)`。
/// `account_id` 必须是小写 `0x` + 64 位十六进制。
pub fn system_account_key(account_id: &str) -> Result<String, String> {
    let account_id = crate::shared::validation::normalize_account_id(account_id)?;
    let account_bytes = hex::decode(account_id.trim_start_matches("0x"))
        .map_err(|e| format!("解析账户 ID 失败: {e}"))?;
    if account_bytes.len() != 32 {
        return Err(format!(
            "账户 ID 长度必须为 32 字节，实际: {}",
            account_bytes.len()
        ));
    }
    Ok(skeys::to_hex(&skeys::blake2_map(
        b"System",
        b"Account",
        &account_bytes,
    )))
}

/// 构造无 map key 的存储 value key：twox_128(pallet) + twox_128(storage)。
/// 用于查询 NextProposalId 等 StorageValue。
pub fn value_key(pallet: &str, storage: &str) -> String {
    skeys::to_hex(&skeys::prefix(pallet.as_bytes(), storage.as_bytes()))
}

/// 构造 StorageMap key：twox_128(pallet) + twox_128(storage) + blake2_128_concat(key_data)。
/// blake2_128_concat = blake2_128(data) + data。
pub fn map_key(pallet: &str, storage: &str, key_data: &[u8]) -> String {
    skeys::to_hex(&skeys::blake2_map(
        pallet.as_bytes(),
        storage.as_bytes(),
        key_data,
    ))
}

/// 构造 DoubleMap key：twox_128(pallet) + twox_128(storage)
///   + blake2_128_concat(key1) + blake2_128_concat(key2)。
pub fn double_map_key(pallet: &str, storage: &str, key1: &[u8], key2: &[u8]) -> String {
    skeys::to_hex(&skeys::blake2_double_map(
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
/// 对应 votingengine 的 `ProposalsByCode / ProposalsByCid / ByOwner / ByYear`。
/// 4 张反向索引的列举前缀。
pub fn twox64_concat_prefix(pallet: &str, storage: &str, key1: &[u8]) -> String {
    skeys::to_hex(&skeys::twox64_map(
        pallet.as_bytes(),
        storage.as_bytes(),
        key1,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_account_key_has_correct_length() {
        let account_id = "0xa5423e483bba281da84b99620a670718d5a7eceb5ae720f7d492e8b5c2570d84";
        let key = system_account_key(account_id).unwrap();
        // 0x 前缀 + (16+16+16+32)*2 hex 字符 = 2 + 160 = 162
        assert_eq!(key.len(), 162);
    }
}
