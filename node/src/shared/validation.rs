// 共享输入校验与标准化逻辑。
use crate::settings::address_utils::decode_ss58_prefix;
use crate::shared::constants::SS58_PREFIX;
const SS58_PRE: &[u8] = b"SS58PRE";

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("SS58 地址解码失败")]
    Ss58DecodeFailed,
    #[error("SS58 地址前缀无效，必须为 2027")]
    Ss58InvalidPrefix,
    #[error("SS58 地址长度无效")]
    Ss58InvalidLength,
    #[error("SS58 地址账户长度无效，必须是 32 字节账户地址")]
    Ss58InvalidAccountLength,
    #[error("SS58 地址校验和无效")]
    Ss58InvalidChecksum,
    #[error("{0}")]
    Ss58PrefixDecode(String),
    #[error("账户 ID 不能为空")]
    AccountIdEmpty,
    #[error("账户 ID 格式无效，应为小写 0x + 64 位十六进制")]
    AccountIdInvalidHex,
    #[error("公钥不能为空")]
    PublicKeyEmpty,
    #[error("公钥格式无效，应为小写 0x + 64 位十六进制")]
    PublicKeyInvalidHex,
    #[error("node-key 不能为空")]
    NodeKeyEmpty,
    #[error("node-key 必须是 64 位十六进制字符串")]
    NodeKeyInvalidHex,
    #[error("GRANDPA 私钥不能为空")]
    GrandpaKeyEmpty,
    #[error("GRANDPA 私钥必须是 64 位十六进制字符串")]
    GrandpaKeyInvalidHex,
}

impl From<ValidationError> for String {
    fn from(e: ValidationError) -> Self {
        e.to_string()
    }
}

fn validate_ss58_address(input: &str) -> Result<(), ValidationError> {
    let data = bs58::decode(input)
        .into_vec()
        .map_err(|_| ValidationError::Ss58DecodeFailed)?;

    let (prefix, prefix_len) =
        decode_ss58_prefix(&data).map_err(ValidationError::Ss58PrefixDecode)?;
    if prefix != SS58_PREFIX {
        return Err(ValidationError::Ss58InvalidPrefix);
    }

    if data.len() < prefix_len + 32 + 2 {
        return Err(ValidationError::Ss58InvalidLength);
    }
    let payload_len = data.len() - prefix_len - 2;
    if payload_len != 32 {
        return Err(ValidationError::Ss58InvalidAccountLength);
    }

    let (without_checksum, checksum) = data.split_at(data.len() - 2);
    let hash = blake2b_simd::Params::new()
        .hash_length(64)
        .to_state()
        .update(SS58_PRE)
        .update(without_checksum)
        .finalize();
    if checksum != &hash.as_bytes()[..2] {
        return Err(ValidationError::Ss58InvalidChecksum);
    }

    Ok(())
}

/// 将跨进程账户文本规范化为唯一格式：小写 `0x` + 64 位十六进制。
pub fn normalize_account_id(input: &str) -> Result<String, String> {
    let value = input.trim();
    if value.is_empty() {
        return Err(ValidationError::AccountIdEmpty.into());
    }

    let Some(raw) = value.strip_prefix("0x") else {
        return Err(ValidationError::AccountIdInvalidHex.into());
    };
    if raw.len() != 64
        || !raw
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ValidationError::AccountIdInvalidHex.into());
    }
    Ok(value.to_string())
}

/// 将 32 字节公钥文本规范化为小写 `0x` + 64 位十六进制。
pub fn normalize_public_key(input: &str) -> Result<String, String> {
    let value = input.trim();
    if value.is_empty() {
        return Err(ValidationError::PublicKeyEmpty.into());
    }
    let Some(raw) = value.strip_prefix("0x") else {
        return Err(ValidationError::PublicKeyInvalidHex.into());
    };
    if raw.len() != 64
        || !raw
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ValidationError::PublicKeyInvalidHex.into());
    }
    Ok(value.to_string())
}

/// 校验仅用于展示或输入的 SS58 地址，并保留原始 Base58 文本。
pub fn normalize_ss58_address(input: &str) -> Result<String, String> {
    let value = input.trim();
    if value.is_empty() {
        return Err(ValidationError::Ss58DecodeFailed.into());
    }
    validate_ss58_address(value)?;
    Ok(value.to_string())
}

pub fn normalize_node_key(input: &str) -> Result<String, String> {
    let value = input.trim();
    if value.is_empty() {
        return Err(ValidationError::NodeKeyEmpty.into());
    }

    let raw = value.strip_prefix("0x").unwrap_or(value);
    if raw.len() != 64 || !raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ValidationError::NodeKeyInvalidHex.into());
    }

    Ok(raw.to_ascii_lowercase())
}

pub fn normalize_grandpa_key(input: &str) -> Result<String, String> {
    let value = input.trim();
    if value.is_empty() {
        return Err(ValidationError::GrandpaKeyEmpty.into());
    }

    let raw = value.strip_prefix("0x").unwrap_or(value);
    if raw.len() != 64 || !raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ValidationError::GrandpaKeyInvalidHex.into());
    }

    Ok(raw.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- normalize_account_id ---

    #[test]
    fn account_id_empty_rejected() {
        assert!(normalize_account_id("").is_err());
        assert!(normalize_account_id("   ").is_err());
    }

    #[test]
    fn account_id_valid_hex() {
        let hex = format!("0x{}", "a1b2c3d4".repeat(8));
        let result = normalize_account_id(&hex).unwrap();
        assert!(result.starts_with("0x"));
        assert_eq!(result.len(), 66);
        assert_eq!(result, result.to_ascii_lowercase());
    }

    #[test]
    fn account_id_hex_wrong_length() {
        assert!(normalize_account_id("0xabcd").is_err());
    }

    #[test]
    fn account_id_hex_non_hex_chars() {
        let bad = format!("0x{}zz", "a1".repeat(31));
        assert!(normalize_account_id(&bad).is_err());
    }

    #[test]
    fn account_id_hex_uppercase_rejected() {
        let hex = format!("0x{}", "A1B2C3D4".repeat(8));
        assert!(normalize_account_id(&hex).is_err());
    }

    #[test]
    fn account_id_requires_0x_prefix() {
        assert!(normalize_account_id(&"a1".repeat(32)).is_err());
    }

    #[test]
    fn public_key_uses_prefixed_lowercase_hex() {
        let input = format!("0x{}", "a1b2".repeat(16));
        assert_eq!(normalize_public_key(&input).unwrap(), input);
        assert!(normalize_public_key(&format!("0x{}", "A1B2".repeat(16))).is_err());
        assert!(normalize_public_key(&"a1".repeat(32)).is_err());
    }

    // --- normalize_node_key ---

    #[test]
    fn node_key_empty_rejected() {
        assert!(normalize_node_key("").is_err());
    }

    #[test]
    fn node_key_valid_no_prefix() {
        let key = "a1b2c3d4".repeat(8);
        let result = normalize_node_key(&key).unwrap();
        assert_eq!(result.len(), 64);
        assert!(!result.starts_with("0x"));
    }

    #[test]
    fn node_key_valid_with_0x_prefix() {
        let key = format!("0x{}", "a1b2c3d4".repeat(8));
        let result = normalize_node_key(&key).unwrap();
        assert_eq!(result.len(), 64);
        assert!(!result.starts_with("0x"));
    }

    #[test]
    fn node_key_wrong_length() {
        assert!(normalize_node_key("abcdef").is_err());
    }

    // --- normalize_grandpa_key ---

    #[test]
    fn grandpa_key_empty_rejected() {
        assert!(normalize_grandpa_key("").is_err());
    }

    #[test]
    fn grandpa_key_valid() {
        let key = "a1b2c3d4".repeat(8);
        let result = normalize_grandpa_key(&key).unwrap();
        assert_eq!(result.len(), 64);
    }

    #[test]
    fn grandpa_key_strips_0x() {
        let key = format!("0x{}", "A1B2C3D4".repeat(8));
        let result = normalize_grandpa_key(&key).unwrap();
        assert_eq!(result, "a1b2c3d4".repeat(8));
    }

    #[test]
    fn grandpa_key_wrong_length() {
        assert!(normalize_grandpa_key("abcd").is_err());
    }
}
