//! 生成司法工作台分区及其能力条件。

use onchina::modules::capability::CapabilitySet;
use onchina::workspace::manifest::{action, section};
use onchina::workspace::model::{WorkspaceSection, WorkspaceSectionKind};
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

#[cfg(test)]
#[path = "../tests/workspace.rs"]
mod tests;
