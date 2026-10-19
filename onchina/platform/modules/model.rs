//! 机构工作台登录态 DTO。
//!
//! 字段统一使用 `workspace_` 前缀,避免与机构、权限、业务列表字段混淆。

use serde::{Deserialize, Serialize};

/// 当前机构使用的工作台类型。
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
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
pub(crate) type WorkspaceModule = String;

/// 工作台顶层分区。
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceSectionKind {
    Operations,
    Display,
    Records,
}

/// 工作台可见动作或页面入口。
pub(crate) type WorkspaceAction = String;

/// 单个工作台入口。`enabled=false` 表示链上/后端能力尚未开放。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct WorkspaceActionItem {
    pub(crate) action: WorkspaceAction,
    pub(crate) title: String,
    pub(crate) enabled: bool,
}

/// 工作台分区及其入口清单。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct WorkspaceSection {
    pub(crate) workspace_section: WorkspaceSectionKind,
    pub(crate) workspace_section_title: String,
    pub(crate) workspace_actions: Vec<WorkspaceActionItem>,
}

/// 当前登录机构的工作台清单。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct InstitutionWorkspace {
    pub(crate) workspace_kind: WorkspaceKind,
    pub(crate) workspace_title: String,
    pub(crate) workspace_sections: Vec<WorkspaceSection>,
    pub(crate) workspace_modules: Vec<WorkspaceModule>,
}


/// 软件装载接口版本，只约束宿主与模块，不涉及链上Runtime版本。
pub(crate) const MODULE_INTERFACE_VERSION: u32 = 1;
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct RouteDescription {
    pub(crate) group: String,
    pub(crate) path: String,
    pub(crate) methods: Vec<String>,
    pub(crate) body_limit: usize,
}
pub(crate) struct ModuleRoute {
    pub(crate) description: RouteDescription,
    pub(crate) handler: axum::routing::MethodRouter<crate::AppState>,
}
impl ModuleRoute {
    pub(crate) fn new(group: &str, path: &str, methods: &[&str], handler: axum::routing::MethodRouter<crate::AppState>) -> Self {
        Self { description: RouteDescription { group: group.into(), path: path.into(), methods: methods.iter().map(|v| (*v).into()).collect(), body_limit: 2 * 1024 * 1024 }, handler }
    }
    pub(crate) fn body_limit(mut self, bytes: usize) -> Self { self.description.body_limit = bytes; self }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct WorkspaceDeclaration {
    pub(crate) kind: WorkspaceKind,
    pub(crate) sections: Vec<WorkspaceSection>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ModuleDescription {
    pub(crate) id: String,
    pub(crate) version: String,
    pub(crate) interface_version: u32,
    pub(crate) frontend_source: String,
    pub(crate) frontend_exports: Vec<String>,
    pub(crate) instance_workspace: bool,
    pub(crate) routes: Vec<RouteDescription>,
    pub(crate) purposes: Vec<String>,
    pub(crate) actions: Vec<String>,
    pub(crate) capabilities: std::collections::BTreeMap<String, super::capability::CapabilitySet>,
    pub(crate) workspaces: std::collections::BTreeMap<String, WorkspaceDeclaration>,
}

/// 每个二进制只提供自己的业务实现；共用流程在公开回调处交接。
#[async_trait::async_trait]
pub(crate) trait ModuleBusiness: Send + Sync {
    fn describe(&self) -> ModuleDescription;
    fn routes(&self) -> Vec<ModuleRoute>;
    fn audit_catalog(&self) -> Result<(), String> { Err("module does not own catalog audit".into()) }
    fn spawn_workers(&self, _state: &crate::AppState) {}
    fn startup(&self, _state: &crate::AppState) -> Result<(), String> { Ok(()) }
    async fn workspace_modules(&self, _cid: &str) -> Vec<String> { Vec::new() }
    async fn recheck_submit(&self, _state: &crate::AppState, _ctx: &crate::authentication::login::AdminAuthContext, _session: &crate::transactions::session::ChainSignSession) -> Result<(), axum::response::Response> { Ok(()) }
    fn validate_submit_context(&self, _session: &crate::transactions::session::ChainSignSession) -> Result<(), String> { Ok(()) }
    async fn safe_to_resubmit(&self, _session: &crate::transactions::session::ChainSignSession) -> bool { true }
    async fn verify_finalized_binding(&self, _session: &crate::transactions::session::ChainSignSession, _finalized: &crate::core::chain_submit::FinalizedChainSubmit) -> Result<Option<crate::core::chain_citizen_identity::FinalizedCitizenIdentity>, axum::response::Response> { Ok(None) }
    async fn commit_projection(&self, db: &crate::Db, session: &crate::transactions::session::ChainSignSession, lease: &crate::transactions::session::SessionLease, finalized: &crate::core::chain_submit::FinalizedChainSubmit, _snapshot: Option<&crate::core::chain_citizen_identity::FinalizedCitizenIdentity>) -> Result<Option<serde_json::Value>, axum::response::Response> {
        crate::transactions::handler::commit_projection(db, session, lease, &finalized.tx_hash, |_| Ok(None)).map_err(|error| {
            tracing::error!(error = %error, "finalized business projection retained for recovery");
            crate::api_error(axum::http::StatusCode::INTERNAL_SERVER_ERROR, 1004, "链上交易已完成，本地投影尚未完成，会话已保留")
        })
    }
    fn preview_admin(&self, conn: &mut postgres::Client, ctx: &crate::authentication::login::AdminAuthContext, action: &crate::authentication::operation_auth::AdminActionType, payload: &serde_json::Value) -> Result<crate::authentication::actions::ActionPreview, String> {
        crate::authentication::actions::preview_action_conn(conn, ctx, action, payload)
    }
    fn preview_admin_transaction(&self, conn: &mut postgres::Transaction<'_>, ctx: &crate::authentication::login::AdminAuthContext, action: &crate::authentication::operation_auth::AdminActionType, payload: &serde_json::Value) -> Result<crate::authentication::actions::ActionPreview, String> {
        crate::authentication::actions::preview_action_conn(conn, ctx, action, payload)
    }
    fn apply_admin_transaction(&self, _conn: &mut postgres::Transaction<'_>, _ctx: &crate::authentication::login::AdminAuthContext, _challenge: &crate::authentication::security_model::AdminActionChallenge) -> Result<serde_json::Value, String> {
        Err("http:bad_request:business action cannot be applied by admin governance endpoint".into())
    }
}
