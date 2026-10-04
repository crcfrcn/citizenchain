//! 链上存储 key 派生唯一真源。
//!
//! Substrate 存储 key 布局:`twox_128(pallet) ++ twox_128(storage)`(StorageValue / map 前缀),
//! StorageMap 追加 hasher 段(`Blake2_128Concat` = `blake2_128(k) ++ k`,`Twox64Concat` =
//! `twox_64(k) ++ k`),DoubleMap 依次追加两段。全部返回裸 `Vec<u8>`;需要 RPC 十六进制键时
//! 用 [`to_hex`] 包一层(`"0x" + hex`)。
//!
//! 本模块是节点侧存储 key 构造的**唯一实现**,以下调用方全部委托这里:
//! - `core::node_guard::*`(区块守卫,消费裸字节);
//! - `governance::storage_keys`(RPC String 门面,委托 + [`to_hex`]);
//! - `home::rpc` / `mining::dashboard` / `settings::reward_account`（各自 RPC 读键）。
//!
//! hasher 统一取 `sp_crypto_hashing`(Substrate 规范实现),与链端逐字节一致;原来 governance 侧
//! 手搓的 `twox_hash` / `blake2b_simd` 实现由此单源取代。

// 原始 hasher 单一出口:既供本模块构造键,也供需要直接取哈希的调用点(key 校验、pallet 前缀等)复用。
pub use sp_crypto_hashing::{blake2_128, twox_128, twox_64};

/// StorageValue key / StorageMap 前缀:`twox_128(pallet) ++ twox_128(storage)`(32 字节)。
pub fn prefix(pallet: &[u8], storage: &[u8]) -> Vec<u8> {
    let mut key = Vec::with_capacity(32);
    key.extend_from_slice(&twox_128(pallet));
    key.extend_from_slice(&twox_128(storage));
    key
}

/// `Blake2_128Concat` hasher 段:`blake2_128(encoded) ++ encoded`(不含 pallet/storage 前缀)。
/// `encoded` 必须是已 SCALE 编码的键字节(与链端 hasher 输入一致)。
pub fn blake2_128_concat(encoded: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + encoded.len());
    out.extend_from_slice(&blake2_128(encoded));
    out.extend_from_slice(encoded);
    out
}

/// `StorageMap<_, Blake2_128Concat, K, _>` 完整键:`prefix ++ blake2_128_concat(encoded_key)`。
pub fn blake2_map(pallet: &[u8], storage: &[u8], encoded_key: &[u8]) -> Vec<u8> {
    let mut key = prefix(pallet, storage);
    key.extend_from_slice(&blake2_128(encoded_key));
    key.extend_from_slice(encoded_key);
    key
}

/// `StorageDoubleMap<_, Blake2_128Concat, K1, Blake2_128Concat, K2, _>` 完整键。
pub fn blake2_double_map(pallet: &[u8], storage: &[u8], k1: &[u8], k2: &[u8]) -> Vec<u8> {
    let mut key = prefix(pallet, storage);
    key.extend_from_slice(&blake2_128(k1));
    key.extend_from_slice(k1);
    key.extend_from_slice(&blake2_128(k2));
    key.extend_from_slice(k2);
    key
}

/// `StorageMap<_, Twox64Concat, K, _>` 完整键 / `Twox64Concat` DoubleMap 第一层列举前缀:
/// `prefix ++ twox_64(encoded_key) ++ encoded_key`。
pub fn twox64_map(pallet: &[u8], storage: &[u8], encoded_key: &[u8]) -> Vec<u8> {
    let mut key = prefix(pallet, storage);
    key.extend_from_slice(&twox_64(encoded_key));
    key.extend_from_slice(encoded_key);
    key
}

/// 裸键字节转 RPC 十六进制键:`"0x" + hex`。
pub fn to_hex(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_matches_known_system_account() {
        // 规范值:twox_128("System") ++ twox_128("Account");与 Substrate 生态公认常量逐字节一致,
        // 证明本模块 twox_128 就是规范实现(据此保证委托来的旧调用点键不变)。
        assert_eq!(
            to_hex(&prefix(b"System", b"Account")),
            "0x26aa394eea5630e07c48ae0c9558cef7b99d880ec681799c0cf30e8886371da9"
        );
    }

    #[test]
    fn blake2_map_layout_is_prefix_plus_concat() {
        let k = b"\x01\x02\x03";
        let full = blake2_map(b"Foo", b"Bar", k);
        let mut expect = prefix(b"Foo", b"Bar");
        expect.extend_from_slice(&blake2_128_concat(k));
        assert_eq!(full, expect);
    }

    #[test]
    fn twox64_map_layout_is_prefix_plus_concat() {
        let k = 7u32.to_le_bytes();
        let full = twox64_map(b"Foo", b"Bar", &k);
        let mut expect = prefix(b"Foo", b"Bar");
        expect.extend_from_slice(&twox_64(&k));
        expect.extend_from_slice(&k);
        assert_eq!(full, expect);
    }
}
