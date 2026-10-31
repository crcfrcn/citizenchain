//! 既有业务入口向量，保持原权限条件及关闭状态。
use onchina::modules::capability::CapabilitySet;
use onchina::workspace::manifest::action;
use onchina::workspace::model::WorkspaceActionItem;
pub(super) fn operations(capabilities: CapabilitySet) -> Vec<WorkspaceActionItem> {
    vec![
        action(
            "propose_legislation".to_string(),
            "发起法律案",
            capabilities.can_propose_legislation,
        ),
        action(
            "cast_representative_vote".to_string(),
            "代表机构表决",
            capabilities.cast_representative_vote,
        ),
    ]
}
