//! 登录态工作台清单生成。
//!
//! 这里把后端能力位组织成「操作 / 显示 / 记录」三类页面;前端按该清单选择工作台壳。

use crate::modules::capability::CapabilitySet;

use super::kind::workspace_kind_for;
use super::model::{
    InstitutionWorkspace, WorkspaceAction, WorkspaceActionItem, WorkspaceKind, WorkspaceModule,
    WorkspaceSection, WorkspaceSectionKind,
};

pub(crate) fn action(action: WorkspaceAction, title: &str, enabled: bool) -> WorkspaceActionItem {
    WorkspaceActionItem {
        action,
        title: title.to_string(),
        enabled,
    }
}

pub(crate) fn section(
    workspace_section: WorkspaceSectionKind,
    workspace_section_title: &str,
    workspace_actions: Vec<WorkspaceActionItem>,
) -> WorkspaceSection {
    WorkspaceSection {
        workspace_section,
        workspace_section_title: workspace_section_title.to_string(),
        workspace_actions,
    }
}

fn workspace_title(
    workspace_kind: WorkspaceKind,
    cid_short_name: Option<&str>,
    institution_code: &str,
) -> String {
    let fallback = match workspace_kind {
        WorkspaceKind::Registry => "注册局工作台",
        WorkspaceKind::Private => "私权机构工作台",
        WorkspaceKind::Judicial => "司法院工作台",
        WorkspaceKind::Legislation => "立法机构工作台",
        WorkspaceKind::Public => "公权机构工作台",
        WorkspaceKind::Unincorporated => "非法人机构工作台",
    };
    cid_short_name
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| format!("{v}工作台"))
        .unwrap_or_else(|| format!("{institution_code}{fallback}"))
}

pub(crate) fn own_institution_sections(capabilities: CapabilitySet) -> Vec<WorkspaceSection> {
    vec![
        section(
            WorkspaceSectionKind::Operations,
            "操作",
            vec![action(
                "manage_own_admins".to_string(),
                "变更本机构管理员",
                false,
            )],
        ),
        section(
            WorkspaceSectionKind::Display,
            "显示",
            vec![
                action("view_institution_profile".to_string(), "本机构信息", true),
                action(
                    "view_own_admins".to_string(),
                    "本机构管理员",
                    capabilities.view_own_admins,
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

/// 构建当前登录机构的工作台清单。
pub(crate) fn build_institution_workspace(
    catalog: &crate::modules::ModuleCatalog,
    institution_code: &str,
    cid_short_name: Option<&str>,
    capabilities: CapabilitySet,
    workspace_modules: Vec<WorkspaceModule>,
) -> InstitutionWorkspace {
    let workspace_kind = workspace_kind_for(catalog, institution_code);
    let workspace_sections = catalog
        .workspace(institution_code)
        .map(|w| w.sections)
        .unwrap_or_else(|| own_institution_sections(capabilities));
    InstitutionWorkspace {
        workspace_kind,
        workspace_title: workspace_title(workspace_kind, cid_short_name, institution_code),
        workspace_sections,
        workspace_modules,
    }
}
