//! 既有司法工作台分区，文字、顺序及能力条件保持不变。
use crate::modules::capability::CapabilitySet;
use crate::workspace::manifest::{action, section};
use crate::workspace::model::{WorkspaceAction, WorkspaceSection, WorkspaceSectionKind};
#[path = "operations.rs"]
mod operations;

pub(crate) fn judicial_sections(capabilities: CapabilitySet) -> Vec<WorkspaceSection> {
    vec![
        section(
            WorkspaceSectionKind::Operations,
            "操作",
            operations::judicial_operations(capabilities),
        ),
        section(
            WorkspaceSectionKind::Display,
            "显示",
            vec![
                action("view_institution_profile".to_string(), "本机构信息", true),
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

