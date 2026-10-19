//! 私权机构业务入口。公民链基金会会员治理只是其中一项机构专属业务。
//! 由独立模块进程处理，平台通过公开接口装载。
include!("../platform/interface.rs");
#[path = "."]
mod private {
#[path = "membership/mod.rs"]
pub(crate) mod membership;

#[path = "workspace/manifest.rs"]
pub(crate) mod workspace;

    use crate::*;
    use crate::modules::model::{ModuleBusiness, ModuleDescription, ModuleRoute};
    pub(crate) const VERSION: &str = "1.0.0";
    #[derive(Default)]
    pub(crate) struct Module;
fn declaration(code: &str) -> Option<(modules::capability::CapabilitySet, crate::workspace::model::WorkspaceKind, Vec<crate::workspace::model::WorkspaceSection>)> {
    let bytes = primitives::cid::code::institution_code_from_str(code)?;
    if !primitives::cid::code::is_private_legal_code(&bytes) { return None; }
    let caps = modules::capability::base_capabilities(code);
    Some((caps, crate::workspace::model::WorkspaceKind::Private, crate::workspace::manifest::own_institution_sections(caps)))
}
fn purposes() -> Vec<String> { vec![membership::PURPOSE_PLATFORM_PRICE_PROPOSAL.into()] }
fn actions() -> Vec<String> { Vec::new() }

    #[async_trait::async_trait]
    impl ModuleBusiness for Module {
        fn describe(&self) -> ModuleDescription {
            let mut description = modules::describe_module(
                "private", VERSION, "frontend/Workspace.tsx", &["PrivateInstitutionWorkspace"],
                self.routes(), purposes(), actions(), declaration,
            );
            description.instance_workspace = true;
            description.frontend_workspace = Some("PrivateInstitutionWorkspace".into());
            description
        }
        fn routes(&self) -> Vec<ModuleRoute> {
            vec![
        modules::model::ModuleRoute::new("admin", "/api/membership/platform-prices", &["GET"], get(private::membership::handler::platform_prices)),
        modules::model::ModuleRoute::new("admin", "/api/membership/platform-prices/propose", &["POST"], post(private::membership::handler::propose_platform_price)),
            ]
        }
        async fn workspace_modules(&self, cid: &str) -> Vec<String> { workspace::workspace_modules_for(cid).await }
        async fn recheck_submit(&self, state: &AppState, ctx: &authentication::login::AdminAuthContext, _session: &transactions::session::ChainSignSession) -> Result<(), axum::response::Response> { membership::handler::recheck_platform_admin(state, ctx).await }

    }

}
fn local_module() -> Option<Arc<dyn modules::model::ModuleBusiness>> { Some(Arc::new(private::Module::default())) }
fn main() { run_backend(local_module()); }
