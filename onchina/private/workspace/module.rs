//! 本包真实业务声明、HTTP路由和原业务回调。
use crate::*;
use onchina::modules::model::{ModuleBusiness, ModuleDescription, ModuleRoute};
pub(crate) const VERSION: &str = "1.0.0";
#[derive(Default)]
pub(crate) struct Module;
fn declaration(
    code: &str,
) -> Option<(
    modules::capability::CapabilitySet,
    onchina::workspace::model::WorkspaceKind,
    Vec<onchina::workspace::model::WorkspaceSection>,
)> {
    let bytes = primitives::cid::code::institution_code_from_str(code)?;
    if !primitives::cid::code::is_private_legal_code(&bytes) {
        return None;
    }
    let caps = modules::capability::base_capabilities(code);
    Some((
        caps,
        onchina::workspace::model::WorkspaceKind::Private,
        onchina::workspace::manifest::own_institution_sections(caps),
    ))
}
fn purposes() -> Vec<String> {
    vec![membership::PURPOSE_PLATFORM_PRICE_PROPOSAL.into()]
}
fn actions() -> Vec<String> {
    Vec::new()
}

#[async_trait::async_trait]
impl ModuleBusiness for Module {
    fn describe(&self) -> ModuleDescription {
        let mut description = modules::describe_module(
            "private",
            VERSION,
            "frontend/Workspace.tsx",
            &["PrivateInstitutionWorkspace"],
            self.routes(),
            purposes(),
            actions(),
            declaration,
        );
        description.instance_workspace = true;
        description.frontend_workspace = Some("PrivateInstitutionWorkspace".into());
        description
    }
    fn routes(&self) -> Vec<ModuleRoute> {
        vec![
            modules::model::ModuleRoute::new(
                "admin",
                "/api/membership/platform-prices",
                &["GET"],
                get(crate::membership::handler::platform_prices),
            ),
            modules::model::ModuleRoute::new(
                "admin",
                "/api/membership/platform-prices/propose",
                &["POST"],
                post(crate::membership::handler::propose_platform_price),
            ),
        ]
    }
    async fn workspace_modules(&self, cid: &str) -> Vec<String> {
        workspace_modules_for(cid).await
    }
    // 回调只交付复查结果；保留原权限检查及错误响应，不向通用提交层传递会员快照。
    async fn recheck_submit(
        &self,
        state: &AppState,
        ctx: &authentication::login::AdminAuthContext,
        _session: &transactions::session::ChainSignSession,
    ) -> Result<(), axum::response::Response> {
        membership::handler::recheck_platform_admin(state, ctx)
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
#[path = "../tests/module.rs"]
mod tests;

// 私权机构实例业务清单；基金会会员治理保留准确 CID 检查。
use onchina::workspace::WorkspaceModule;
/// 按准确机构 CID 解析实例级工作台模块。链不可达或链上未绑定时返回空集合。
pub(crate) async fn workspace_modules_for(institution_cid_number: &str) -> Vec<WorkspaceModule> {
    match onchina::chain_runtime::fetch_platform_membership_snapshot().await {
        Ok(snapshot) if snapshot.platform_cid_number.as_deref() == Some(institution_cid_number) => {
            vec!["platform_membership_price".to_string()]
        }
        Ok(_) => Vec::new(),
        Err(err) => {
            tracing::warn!(error = %err, "resolve platform membership workspace module failed");
            Vec::new()
        }
    }
}

#[cfg(test)]
#[path = "../tests/workspace.rs"]
mod workspace_tests;
