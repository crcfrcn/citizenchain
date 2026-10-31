//! 既有工作台分区，保持文字、顺序与能力条件。
use onchina::modules::capability::CapabilitySet;
use onchina::workspace::manifest::{action, section};
use onchina::workspace::model::{WorkspaceSection, WorkspaceSectionKind};
#[path = "operations.rs"]
mod operations;
pub(crate) fn registry_sections(capabilities: CapabilitySet) -> Vec<WorkspaceSection> {
    vec![
        section(
            WorkspaceSectionKind::Operations,
            "操作",
            operations::operations(capabilities),
        ),
        section(
            WorkspaceSectionKind::Display,
            "显示",
            vec![action(
                "view_own_admins".to_string(),
                "本机构管理员",
                capabilities.view_own_admins
                    || capabilities.view_city_registry
                    || capabilities.view_federal_registry,
            )],
        ),
        section(
            WorkspaceSectionKind::Records,
            "记录",
            vec![action(
                "view_operation_records".to_string(),
                "操作记录",
                capabilities.can_manage_institutions,
            )],
        ),
    ]
}
