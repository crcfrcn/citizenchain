//! 严格校验机构岗位人员的规范账户文本。

/// 岗位人员账户文本只允许使用全仓统一的账户 ID 格式。

pub(crate) fn normalize_account_id(account_id: &str) -> Result<String, String> {
    nodedesktop::validation::normalize_account_id(account_id)
}
