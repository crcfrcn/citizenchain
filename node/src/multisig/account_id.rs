/// 严格解析机构交易明确传入的账户，不接受 CID、前缀身份串或主账户回退。
pub fn institution_account_from_id(account_id: &str) -> Result<[u8; 32], String> {
    let account_id = crate::shared::validation::normalize_account_id(account_id)?;
    let account = hex::decode(account_id.trim_start_matches("0x"))
        .map_err(|e| format!("institution_account_id 解码失败: {e}"))?;
    if account.len() != 32 {
        return Err("institution_account_id 必须是 32 字节 AccountId".to_string());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&account);
    Ok(out)
}
