//! 链账户 ID 与 sr25519 签名公钥工具。
//!
//! 账户输入只接受小写 `0x` 加 64 位十六进制；SS58 仅用于展示，禁止作为授权、
//! 存储或接口身份主键。

pub(crate) fn normalize_account_id(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.len() != 66 || !trimmed.starts_with("0x") {
        return None;
    }
    let hex = &trimmed[2..];
    if !hex
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    Some(trimmed.to_string())
}

pub(crate) fn same_account_id(left: &str, right: &str) -> bool {
    matches!(
        (normalize_account_id(left), normalize_account_id(right)),
        (Some(left), Some(right)) if left == right
    )
}

/// 规范账户 ID 转 SS58 展示地址(prefix=2027)。
pub(crate) fn account_id_to_ss58(account_id: &str) -> Option<String> {
    let account_id = normalize_account_id(account_id)?;
    let pubkey_bytes = hex::decode(&account_id[2..]).ok()?;
    if pubkey_bytes.len() != 32 {
        return None;
    }
    use blake2::{digest::VariableOutput, Blake2bVar};
    let prefix: u16 = 2027;
    let first = ((prefix & 0b0000_0000_1111_1100) as u8) >> 2 | 0b01000000;
    let second = (prefix >> 8) as u8 | ((prefix & 0b0000_0000_0000_0011) as u8) << 6;
    let mut payload = vec![first, second];
    payload.extend_from_slice(&pubkey_bytes);
    let mut hasher = Blake2bVar::new(64).ok()?;
    use blake2::digest::Update;
    hasher.update(b"SS58PRE");
    hasher.update(&payload);
    let mut hash = vec![0u8; 64];
    hasher.finalize_variable(&mut hash).ok()?;
    payload.extend_from_slice(&hash[..2]);
    Some(bs58::encode(payload).into_string())
}

#[cfg(test)]
// 公钥夹具是固定常量，解析失败应直接中止测试。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    const ACCOUNT_ID: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";

    #[test]
    fn account_id_accepts_only_canonical_text() {
        assert_eq!(
            normalize_account_id(ACCOUNT_ID).as_deref(),
            Some(ACCOUNT_ID)
        );
        assert!(normalize_account_id(&ACCOUNT_ID[2..]).is_none());
        assert!(normalize_account_id(&ACCOUNT_ID.to_uppercase()).is_none());
        assert!(normalize_account_id("5GrwvaEF5zXb26Fz9rcQpDWSQ57Cr4NMh6t7vY6zT").is_none());
    }

    #[test]
    fn account_comparison_has_no_case_or_prefix_fallback() {
        assert!(same_account_id(ACCOUNT_ID, ACCOUNT_ID));
        assert!(!same_account_id(ACCOUNT_ID, &ACCOUNT_ID.to_uppercase()));
        assert!(!same_account_id(ACCOUNT_ID, &ACCOUNT_ID[2..]));
    }

    #[test]
    fn ss58_is_derived_without_changing_account_id() {
        let ss58_address = account_id_to_ss58(ACCOUNT_ID).expect("规范账户应能派生展示地址");
        assert!(!ss58_address.starts_with("0x"));
        assert_eq!(
            normalize_account_id(ACCOUNT_ID).as_deref(),
            Some(ACCOUNT_ID)
        );
    }
}
