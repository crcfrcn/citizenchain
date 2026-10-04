//! 登录态工作台清单生成。
//!
//! 这里把后端能力位组织成「操作 / 显示 / 记录」三类页面;前端按该清单选择工作台壳。

use crate::platform::capability::CapabilitySet;

use super::kind::workspace_kind_for;
use super::model::{
    InstitutionWorkspace, WorkspaceAction, WorkspaceActionItem, WorkspaceKind, WorkspaceModule,
    WorkspaceSection, WorkspaceSectionKind,
};

fn action(action: WorkspaceAction, title: &str, enabled: bool) -> WorkspaceActionItem {
    WorkspaceActionItem {
        action,
        title: title.to_string(),
        enabled,
    }
}

fn section(
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

fn registry_sections(capabilities: CapabilitySet) -> Vec<WorkspaceSection> {
    vec![
        section(
            WorkspaceSectionKind::Operations,
            "操作",
            vec![
                action(
                    WorkspaceAction::RegisterCitizen,
                    "公民登记",
                    capabilities.can_view_citizens && capabilities.can_business_write,
                ),
                action(
                    WorkspaceAction::RegisterInstitution,
                    "公权机构登记",
                    capabilities.can_manage_institutions && capabilities.can_register_institutions,
                ),
                action(
                    WorkspaceAction::RegisterPrivate,
                    "私权机构登记",
                    capabilities.can_view_private && capabilities.can_business_write,
                ),
                action(
                    WorkspaceAction::RegisterEducation,
                    "教育机构登记",
                    capabilities.can_view_education && capabilities.can_business_write,
                ),
                action(
                    WorkspaceAction::ManageRegistryAdmins,
                    "注册局管理员",
                    capabilities.can_view_city_registry || capabilities.can_view_federal_registry,
                ),
            ],
        ),
        section(
            WorkspaceSectionKind::Display,
            "显示",
            vec![action(
                WorkspaceAction::ViewOwnAdmins,
                "本机构管理员",
                capabilities.can_view_own_admins
                    || capabilities.can_view_city_registry
                    || capabilities.can_view_federal_registry,
            )],
        ),
        section(
            WorkspaceSectionKind::Records,
            "记录",
            vec![action(
                WorkspaceAction::ViewOperationRecords,
                "操作记录",
                capabilities.can_manage_institutions,
            )],
        ),
    ]
}

fn judicial_sections(capabilities: CapabilitySet) -> Vec<WorkspaceSection> {
    vec![
        section(
            WorkspaceSectionKind::Operations,
            "操作",
            vec![
                action(
                    WorkspaceAction::SignLegislation,
                    "护宪终审",
                    capabilities.can_sign_legislation,
                ),
                action(WorkspaceAction::ManageOwnAdmins, "变更本机构管理员", false),
            ],
        ),
        section(
            WorkspaceSectionKind::Display,
            "显示",
            vec![
                action(WorkspaceAction::ViewInstitutionProfile, "本机构信息", true),
                action(
                    WorkspaceAction::ViewOwnAdmins,
                    "本机构管理员",
                    capabilities.can_view_own_admins,
                ),
            ],
        ),
        section(
            WorkspaceSectionKind::Records,
            "记录",
            vec![action(
                WorkspaceAction::ViewOperationRecords,
                "操作记录",
                false,
            )],
        ),
    ]
}

fn legislation_sections(capabilities: CapabilitySet) -> Vec<WorkspaceSection> {
    vec![
        section(
            WorkspaceSectionKind::Operations,
            "操作",
            vec![
                action(
                    WorkspaceAction::ProposeLegislation,
                    "发起法律案",
                    capabilities.can_propose_legislation,
                ),
                action(
                    WorkspaceAction::CastRepresentativeVote,
                    "代表机构表决",
                    capabilities.can_cast_representative_vote,
                ),
            ],
        ),
        section(
            WorkspaceSectionKind::Display,
            "显示",
            vec![
                action(
                    WorkspaceAction::ViewLegislation,
                    "立法与表决",
                    capabilities.can_view_legislation,
                ),
                action(
                    WorkspaceAction::ViewOwnAdmins,
                    "本机构管理员",
                    capabilities.can_view_own_admins,
                ),
            ],
        ),
        section(
            WorkspaceSectionKind::Records,
            "记录",
            vec![action(
                WorkspaceAction::ViewOperationRecords,
                "操作记录",
                false,
            )],
        ),
    ]
}

fn own_institution_sections(capabilities: CapabilitySet) -> Vec<WorkspaceSection> {
    vec![
        section(
            WorkspaceSectionKind::Operations,
            "操作",
            vec![action(
                WorkspaceAction::ManageOwnAdmins,
                "变更本机构管理员",
                false,
            )],
        ),
        section(
            WorkspaceSectionKind::Display,
            "显示",
            vec![
                action(WorkspaceAction::ViewInstitutionProfile, "本机构信息", true),
                action(
                    WorkspaceAction::ViewOwnAdmins,
                    "本机构管理员",
                    capabilities.can_view_own_admins,
                ),
            ],
        ),
        section(
            WorkspaceSectionKind::Records,
            "记录",
            vec![action(
                WorkspaceAction::ViewOperationRecords,
                "操作记录",
                false,
            )],
        ),
    ]
}

/// 构建当前登录机构的工作台清单。
pub(crate) fn build_institution_workspace(
    institution_code: &str,
    cid_short_name: Option<&str>,
    capabilities: CapabilitySet,
    workspace_modules: Vec<WorkspaceModule>,
) -> InstitutionWorkspace {
    let workspace_kind = workspace_kind_for(institution_code);
    let workspace_sections = match workspace_kind {
        WorkspaceKind::Registry => registry_sections(capabilities),
        WorkspaceKind::Private => own_institution_sections(capabilities),
        WorkspaceKind::Judicial => judicial_sections(capabilities),
        WorkspaceKind::Legislation => legislation_sections(capabilities),
        WorkspaceKind::Public | WorkspaceKind::Unincorporated => {
            own_institution_sections(capabilities)
        }
    };
    InstitutionWorkspace {
        workspace_kind,
        workspace_title: workspace_title(workspace_kind, cid_short_name, institution_code),
        workspace_sections,
        workspace_modules,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_legal_institution_gets_own_private_workspace() {
        let workspace = build_institution_workspace(
            "SFGQ",
            Some("示例私权机构"),
            crate::platform::capability::capabilities_for("SFGQ"),
            vec![WorkspaceModule::PlatformMembershipPrice],
        );
        assert_eq!(workspace.workspace_kind, WorkspaceKind::Private);
        assert_eq!(
            workspace.workspace_modules,
            vec![WorkspaceModule::PlatformMembershipPrice]
        );
    }

    #[test]
    fn registry_and_private_workspaces_never_share_kind() {
        let registry = build_institution_workspace(
            "FRG",
            None,
            crate::platform::capability::capabilities_for("FRG"),
            Vec::new(),
        );
        let private = build_institution_workspace(
            "SFGQ",
            None,
            crate::platform::capability::capabilities_for("SFGQ"),
            Vec::new(),
        );
        assert_eq!(registry.workspace_kind, WorkspaceKind::Registry);
        assert_eq!(private.workspace_kind, WorkspaceKind::Private);
    }
}
