use crate::address::decode_ss58_prefix;
use crate::constants::SS58_PREFIX;

const SS58_PRE: &[u8] = b"SS58PRE";

#[derive(Debug, thiserror::Error)]
pub(crate) enum ValidationError {
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
        .map_err(|_| crate::validation::ValidationError::Ss58DecodeFailed)?;

    let (prefix, prefix_len) =
        decode_ss58_prefix(&data).map_err(crate::validation::ValidationError::Ss58PrefixDecode)?;
    if prefix != SS58_PREFIX {
        return Err(crate::validation::ValidationError::Ss58InvalidPrefix);
    }

    if data.len() < prefix_len + 32 + 2 {
        return Err(crate::validation::ValidationError::Ss58InvalidLength);
    }
    let payload_len = data.len() - prefix_len - 2;
    if payload_len != 32 {
        return Err(crate::validation::ValidationError::Ss58InvalidAccountLength);
    }

    let (without_checksum, checksum) = data.split_at(data.len() - 2);
    let hash = blake2b_simd::Params::new()
        .hash_length(64)
        .to_state()
        .update(SS58_PRE)
        .update(without_checksum)
        .finalize();
    if checksum != &hash.as_bytes()[..2] {
        return Err(crate::validation::ValidationError::Ss58InvalidChecksum);
    }

    Ok(())
}

/// 将跨进程账户文本规范化为唯一格式：小写 `0x` + 64 位十六进制。
pub fn normalize_account_id(input: &str) -> Result<String, String> {
    let value = input.trim();
    if value.is_empty() {
        return Err(crate::validation::ValidationError::AccountIdEmpty.into());
    }

    let Some(raw) = value.strip_prefix("0x") else {
        return Err(crate::validation::ValidationError::AccountIdInvalidHex.into());
    };
    if raw.len() != 64
        || !raw
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(crate::validation::ValidationError::AccountIdInvalidHex.into());
    }
    Ok(value.to_string())
}

/// 将 32 字节公钥文本规范化为小写 `0x` + 64 位十六进制。
pub fn normalize_public_key(input: &str) -> Result<String, String> {
    let value = input.trim();
    if value.is_empty() {
        return Err(crate::validation::ValidationError::PublicKeyEmpty.into());
    }
    let Some(raw) = value.strip_prefix("0x") else {
        return Err(crate::validation::ValidationError::PublicKeyInvalidHex.into());
    };
    if raw.len() != 64
        || !raw
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(crate::validation::ValidationError::PublicKeyInvalidHex.into());
    }
    Ok(value.to_string())
}

/// 校验仅用于展示或输入的 SS58 地址，并保留原始 Base58 文本。
pub fn normalize_ss58_address(input: &str) -> Result<String, String> {
    let value = input.trim();
    if value.is_empty() {
        return Err(crate::validation::ValidationError::Ss58DecodeFailed.into());
    }
    validate_ss58_address(value)?;
    Ok(value.to_string())
}

pub fn normalize_node_key(input: &str) -> Result<String, String> {
    let value = input.trim();
    if value.is_empty() {
        return Err(crate::validation::ValidationError::NodeKeyEmpty.into());
    }

    let raw = value.strip_prefix("0x").unwrap_or(value);
    if raw.len() != 64 || !raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(crate::validation::ValidationError::NodeKeyInvalidHex.into());
    }

    Ok(raw.to_ascii_lowercase())
}

pub fn normalize_grandpa_key(input: &str) -> Result<String, String> {
    let value = input.trim();
    if value.is_empty() {
        return Err(crate::validation::ValidationError::GrandpaKeyEmpty.into());
    }

    let raw = value.strip_prefix("0x").unwrap_or(value);
    if raw.len() != 64 || !raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(crate::validation::ValidationError::GrandpaKeyInvalidHex.into());
    }

    Ok(raw.to_ascii_lowercase())
}
#[cfg(test)]
#[path = "../tests/security.rs"]
mod tests;
