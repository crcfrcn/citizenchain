//! 签名、公钥与本地验签。
use crate::SigningMaterial;
use sha2::{Digest, Sha256};
use sp_core::{sr25519, Pair};
use sp_runtime::{traits::IdentifyAccount, AccountId32, MultiSigner};

/// sha256 hex，用于会话校验和日志定位。
pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// 解析跨端唯一格式的 32 字节 sr25519 公钥：小写 `0x` + 64 位十六进制。
pub fn parse_sr25519_public_key(public_key: &str) -> Result<sr25519::Public, String> {
    if public_key.len() != 66
        || !public_key.starts_with("0x")
        || !public_key[2..]
            .chars()
            .all(|ch| ch.is_ascii_digit() || ('a'..='f').contains(&ch))
    {
        return Err("公钥格式无效，应为小写 0x + 64 位十六进制".to_string());
    }
    let raw = hex::decode(&public_key[2..]).map_err(|e| format!("公钥解码失败: {e}"))?;
    let bytes = <[u8; 32]>::try_from(raw.as_slice()).map_err(|_| "公钥必须 32 字节")?;
    Ok(sr25519::Public::from_raw(bytes))
}

/// 解析 64 字节 sr25519 签名 hex。
pub fn parse_sr25519_signature_hex(signature_hex: &str) -> Result<sr25519::Signature, String> {
    let raw = hex::decode(signature_hex.trim_start_matches("0x"))
        .map_err(|e| format!("签名解码失败: {e}"))?;
    let bytes = <[u8; 64]>::try_from(raw.as_slice()).map_err(|_| "签名必须 64 字节")?;
    Ok(sr25519::Signature::from_raw(bytes))
}

/// 使用共享签名字节本地验签。
pub fn verify_signature(
    material: &SigningMaterial,
    signature: &sr25519::Signature,
    public: &sr25519::Public,
) -> bool {
    sr25519::Pair::verify(signature, &material.signing_bytes, public)
}

/// sr25519 公钥对应的 AccountId。
pub fn account_id_from_public(public: sr25519::Public) -> AccountId32 {
    MultiSigner::from(public).into_account()
}

/// 用 sr25519 pair 对共享签名字节签名。
pub fn sign_material_with_pair(
    material: &SigningMaterial,
    sender: &sr25519::Pair,
) -> sr25519::Signature {
    sender.sign(&material.signing_bytes)
}
