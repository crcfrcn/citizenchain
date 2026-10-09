/// 管理员账户文本只允许使用全仓统一的账户 ID 格式。
pub fn normalize_account_id(account_id: &str) -> Result<String, String> {
    crate::shared::validation::normalize_account_id(account_id)
}
