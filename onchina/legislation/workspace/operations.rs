//! 既有业务入口向量，保持原权限条件及关闭状态。
use crate::modules::capability::CapabilitySet;
use crate::workspace::manifest::action;
use crate::workspace::model::{WorkspaceAction, WorkspaceActionItem};
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
                    capabilities.can_cast_representative_vote,
                ),
            ]
}
