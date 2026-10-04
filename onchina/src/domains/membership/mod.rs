//! 平台会员价格治理域。
//!
//! 本域只读取 finalized `SquarePost` 真源、校验公民链基金会准确 CID，并构造现有
//! `propose_set_platform_price` 冷签交易。投票资格、计票与执行仍完全归统一投票引擎。

pub(crate) mod chain_call;
pub(crate) mod handler;

use crate::workspace::WorkspaceModule;

/// 统一链冷签会话用途；提交仍走 `core::chain_submit` 唯一路径。
pub(crate) const PURPOSE_PLATFORM_PRICE_PROPOSAL: &str = "PLATFORM_PRICE_PROPOSAL";

/// 按准确机构 CID 解析实例级工作台模块。链不可达或链上未绑定时返回空集合。
pub(crate) async fn workspace_modules_for(institution_cid_number: &str) -> Vec<WorkspaceModule> {
    match crate::core::chain_runtime::fetch_platform_membership_snapshot().await {
        Ok(snapshot) if snapshot.platform_cid_number.as_deref() == Some(institution_cid_number) => {
            vec![WorkspaceModule::PlatformMembershipPrice]
        }
        Ok(_) => Vec::new(),
        Err(err) => {
            tracing::warn!(error = %err, "resolve platform membership workspace module failed");
            Vec::new()
        }
    }
}
