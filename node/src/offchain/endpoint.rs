//! 清算行节点声明的链上查询。
//!
//!
//! - `ClearingBankNodes` 属于扫码支付网络准入信息,因此放在
//!   `offchain` 下,供节点注册页和网络概览使用。
//! - 本模块只读取链上声明,不会启动清算 worker;普通全节点可以只读查询,
//!   但不能因此获得清算行权限。

use codec::{Decode, Encode};
use sp_core::ConstU32;
use sp_runtime::{AccountId32, BoundedVec};

use crate::governance::chain_query;
use crate::governance::signing::account_id_to_ss58;
use crate::governance::storage_keys;
use crate::transaction::offchain::types::ClearingBankNodeOnChainInfo;

/// 链上 `ClearingBankNodeInfo<AccountId, BlockNumber>` 在 node 端的 SCALE 镜像。
///
/// runtime 端定义见 [citizenchain/runtime/transaction/offchain/src/lib.rs:65]。
/// 字段顺序 / 边界长度必须与 runtime 严格一致,SCALE 解码才能成功。
#[derive(Decode, Encode)]
struct OnChainNodeInfo {
    peer_id: BoundedVec<u8, ConstU32<64>>,
    rpc_domain: BoundedVec<u8, ConstU32<128>>,
    rpc_port: u16,
    /// runtime 端 `BlockNumber = u32`(citizenchain Runtime 配置)。
    registered_at: u32,
    registered_by: AccountId32,
}

/// SCALE 编码 cid_number 的全仓统一 `BoundedVec<u8, ConstU32<32>>` 形式。
///
/// 字段编码:`Compact<u32>(len)` + `bytes`。
fn encode_cid_key_data(cid_number: &str) -> Result<Vec<u8>, String> {
    let raw = cid_number.as_bytes();
    if raw.is_empty() || raw.len() > primitives::core_const::CID_NUMBER_MAX_BYTES as usize {
        return Err(format!(
            "cid_number 长度必须在链上 CID_NUMBER_MAX_BYTES 范围内,实际:{}",
            raw.len()
        ));
    }
    let bv: BoundedVec<u8, ConstU32<{ primitives::core_const::CID_NUMBER_MAX_BYTES }>> = raw
        .to_vec()
        .try_into()
        .map_err(|_| "cid_number 超出链上 CID_NUMBER_MAX_BYTES".to_string())?;
    Ok(bv.encode())
}

/// 构造 `OffchainTransaction::ClearingBankNodes(cid_number)` 的 storage key(hex 含 0x 前缀)。
pub fn clearing_bank_nodes_key(cid_number: &str) -> Result<String, String> {
    let key_data = encode_cid_key_data(cid_number)?;
    Ok(storage_keys::map_key(
        "OffchainTransaction",
        "ClearingBankNodes",
        &key_data,
    ))
}

/// 链上查询单条清算行节点声明信息。返回 None 表示该 cid_number 尚未注册节点。
pub fn fetch_clearing_bank_node(
    cid_number: &str,
) -> Result<Option<ClearingBankNodeOnChainInfo>, String> {
    let key = clearing_bank_nodes_key(cid_number)?;
    // (ADR-017):节点声明属于业务读取,按 finalized 口径,禁止 best。
    match chain_query::fetch_finalized_storage(&key)? {
        None => Ok(None),
        Some(hex_data) => {
            let clean = hex_data.strip_prefix("0x").unwrap_or(&hex_data);
            let bytes = hex::decode(clean).map_err(|e| format!("storage hex 解码失败:{e}"))?;
            let info = OnChainNodeInfo::decode(&mut &bytes[..])
                .map_err(|e| format!("ClearingBankNodeInfo SCALE 解码失败:{e}"))?;

            let peer_id = String::from_utf8(info.peer_id.into_inner())
                .map_err(|_| "PeerId 编码非 UTF-8".to_string())?;
            let rpc_domain = String::from_utf8(info.rpc_domain.into_inner())
                .map_err(|_| "RPC 域名编码非 UTF-8".to_string())?;

            let registered_by_bytes: [u8; 32] = info.registered_by.into();
            let registered_by_account_id = format!("0x{}", hex::encode(registered_by_bytes));
            let registered_by_ss58_address = account_id_to_ss58(&registered_by_bytes)?;

            Ok(Some(ClearingBankNodeOnChainInfo {
                cid_number: cid_number.to_string(),
                peer_id,
                rpc_domain,
                rpc_port: info.rpc_port,
                registered_at: info.registered_at as u64,
                registered_by_account_id,
                registered_by_ss58_address,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_cid_key_data_round_trip() {
        let raw = "LN001-NRC0G-944805165-2026";
        let encoded = encode_cid_key_data(raw).unwrap();
        // Compact<u32> 长度前缀(单字节模式 raw.len() < 64)+ raw 字节
        assert_eq!(encoded[0], (raw.len() as u8) << 2);
        assert_eq!(&encoded[1..], raw.as_bytes());
    }

    #[test]
    fn empty_cid_rejected() {
        let err = encode_cid_key_data("").unwrap_err();
        assert!(err.contains("长度"));
    }

    #[test]
    fn over_long_cid_rejected() {
        let s = "A".repeat(33);
        let err = encode_cid_key_data(&s).unwrap_err();
        assert!(err.contains("长度"));
    }

    #[test]
    fn clearing_bank_nodes_key_starts_with_pallet_prefix() {
        let key = clearing_bank_nodes_key("AH001-SCB0V-123456789-2026").unwrap();
        let pallet_hex = hex::encode(storage_keys::twox_128(b"OffchainTransaction"));
        let storage_hex = hex::encode(storage_keys::twox_128(b"ClearingBankNodes"));
        let prefix = format!("0x{pallet_hex}{storage_hex}");
        assert!(key.starts_with(&prefix), "实际 key:{key}");
    }
}
