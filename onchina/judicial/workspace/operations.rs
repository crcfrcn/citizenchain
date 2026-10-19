//! 既有司法操作入口，保持原有禁用条件。
use crate::modules::capability::CapabilitySet;
use crate::workspace::manifest::action;
use crate::workspace::model::{WorkspaceAction, WorkspaceActionItem};
pub(super) fn judicial_operations(capabilities: CapabilitySet) -> Vec<WorkspaceActionItem> {
vec![
                action(
                    "sign_legislation".to_string(),
                    "护宪终审",
                    capabilities.can_sign_legislation,
                ),
                action("manage_own_admins".to_string(), "变更本机构管理员", false),
            ]
}
