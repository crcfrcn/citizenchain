use sp_crypto_hashing::blake2_128;
use sp_crypto_hashing::twox_128;
use sp_crypto_hashing::twox_64;

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
#[path = "../tests/integration.rs"]
mod tests;
