//! 机构工作台登录态 DTO。
//!
//! 字段统一使用 `workspace_` 前缀,避免与机构、权限、业务列表字段混淆。

use serde::Serialize;

/// 当前机构使用的工作台类型。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceKind {
    Registry,
    Private,
    Judicial,
    Legislation,
    Public,
    Unincorporated,
}

/// 当前准确机构实例可挂载的专属业务模块。
///
/// 类型能力只能决定工作台大类；公民链基金会等唯一机构必须按 CID 实例授权。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceModule {
    PlatformMembershipPrice,
}

/// 工作台顶层分区。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceSectionKind {
    Operations,
    Display,
    Records,
}

/// 工作台可见动作或页面入口。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceAction {
    RegisterCitizen,
    RegisterInstitution,
    RegisterPrivate,
    RegisterEducation,
    ManageRegistryAdmins,
    ManageOwnAdmins,
    ViewOwnAdmins,
    ViewLegislation,
    ProposeLegislation,
    CastRepresentativeVote,
    SignLegislation,
    ViewInstitutionProfile,
    ViewOperationRecords,
}

/// 单个工作台入口。`enabled=false` 表示链上/后端能力尚未开放。
#[derive(Debug, Clone, Serialize)]
pub(crate) struct WorkspaceActionItem {
    pub(crate) action: WorkspaceAction,
    pub(crate) title: String,
    pub(crate) enabled: bool,
}

/// 工作台分区及其入口清单。
#[derive(Debug, Clone, Serialize)]
pub(crate) struct WorkspaceSection {
    pub(crate) workspace_section: WorkspaceSectionKind,
    pub(crate) workspace_section_title: String,
    pub(crate) workspace_actions: Vec<WorkspaceActionItem>,
}

/// 当前登录机构的工作台清单。
#[derive(Debug, Clone, Serialize)]
pub(crate) struct InstitutionWorkspace {
    pub(crate) workspace_kind: WorkspaceKind,
    pub(crate) workspace_title: String,
    pub(crate) workspace_sections: Vec<WorkspaceSection>,
    pub(crate) workspace_modules: Vec<WorkspaceModule>,
}
