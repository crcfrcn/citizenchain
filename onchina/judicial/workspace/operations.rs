//! 生成司法操作入口并保留未开放动作的禁用条件。

use onchina::modules::capability::CapabilitySet;
use onchina::workspace::manifest::action;
use onchina::workspace::model::WorkspaceActionItem;
pub(super) fn judicial_operations(capabilities: CapabilitySet) -> Vec<WorkspaceActionItem> {
    vec![
        action(
            "sign_legislation".to_string(),
            "护宪终审",
            capabilities.can_sign_legislation,
        ),
    ]
}
