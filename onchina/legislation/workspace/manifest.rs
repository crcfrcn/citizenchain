//! 既有工作台分区，保持文字、顺序与能力条件。
use crate::modules::capability::CapabilitySet;
use crate::workspace::manifest::{action, section};
use crate::workspace::model::{WorkspaceSection, WorkspaceSectionKind};
#[path = "operations.rs"]
mod operations;
pub(crate) fn legislation_sections(capabilities: CapabilitySet) -> Vec<WorkspaceSection> {
    vec![
        section(
            WorkspaceSectionKind::Operations,
            "操作",
            operations::operations(capabilities),
        ),
        section(
            WorkspaceSectionKind::Display,
            "显示",
            vec![
                action(
                    "view_legislation".to_string(),
                    "立法与表决",
                    capabilities.can_view_legislation,
                ),
                action(
                    "view_own_admins".to_string(),
                    "本机构管理员",
                    capabilities.can_view_own_admins,
                ),
            ],
        ),
        section(
            WorkspaceSectionKind::Records,
            "记录",
            vec![action(
                "view_operation_records".to_string(),
                "操作记录",
                false,
            )],
        ),
    ]
}
