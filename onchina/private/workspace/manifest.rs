//! 私权机构实例业务清单；基金会会员治理保留准确 CID 检查。
use crate::workspace::WorkspaceModule;
/// 按准确机构 CID 解析实例级工作台模块。链不可达或链上未绑定时返回空集合。
pub(crate) async fn workspace_modules_for(institution_cid_number: &str) -> Vec<WorkspaceModule> {
    match crate::core::chain_runtime::fetch_platform_membership_snapshot().await {
        Ok(snapshot) if snapshot.platform_cid_number.as_deref() == Some(institution_cid_number) => {
            vec!["platform_membership_price".to_string()]
        }
        Ok(_) => Vec::new(),
        Err(err) => {
            tracing::warn!(error = %err, "resolve platform membership workspace module failed");
            Vec::new()
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
