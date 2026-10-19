//! 注册局独立模块入口。公民、机构登记、地址与投影业务沿用原实现。
include!("../platform/interface.rs");
pub(crate) use registry::admins::model::*;
pub(crate) use registry::citizens::model::*;
pub(crate) use registry::documents::model::*;
#[path = "."]
mod registry {
#[path = "address/mod.rs"]
pub(crate) mod address;
#[path = "citizens/mod.rs"]
pub(crate) mod citizens;
#[path = "institutions/mod.rs"]
pub(crate) mod institutions;
#[path = "projection/mod.rs"]
pub(crate) mod projection;
#[path = "documents/mod.rs"]
pub(crate) mod documents;

#[path = "workspace/manifest.rs"]
pub(crate) mod workspace;

#[path = "admins/mod.rs"]
pub(crate) mod admins;

    use crate::*;
    use crate::modules::model::{ModuleBusiness, ModuleDescription, ModuleRoute};
    pub(crate) const VERSION: &str = "1.0.0";
    #[derive(Default)]
    pub(crate) struct Module { federal_cache: admins::catalog::FederalAdminsCache }
use crate::modules::capability::{CapabilitySet, EMPTY};
// Tier1 创世注册局(FRG):是 CREG 的省级上游,能力必须是 Tier2 业务能力的超集。
// 链上 runtime 已限制 FRG 只能按本省省级组登记本省 CID;这里仅声明控制台可见能力。
const TIER1_REGISTRY: CapabilitySet = CapabilitySet {
    can_view_citizens: true,
    can_view_institutions: true,
    can_view_private: true,
    can_view_education: true,
    can_view_federal_registry_admins: true,
    can_view_city_registry_admins: true,
    can_crud_city_registry_admins: true,
    can_manage_institutions: true,
    can_register_institutions: true,
    can_business_write: true,
    can_view_city_registry: true,
    can_view_federal_registry: true,
    ..EMPTY
};

// Tier2 下级注册局(CREG):录入公民/私权/教育/公权机构 + 看本市市注册局;
// 同时只读本省联邦注册局管理员列表,不得发起联邦注册局管理员操作。
const SUBORDINATE_REGISTRY: CapabilitySet = CapabilitySet {
    can_view_citizens: true,
    can_view_institutions: true,
    can_view_private: true,
    can_view_education: true,
    can_view_federal_registry_admins: true,
    can_view_city_registry_admins: true,
    can_manage_institutions: true,
    can_register_institutions: true,
    can_business_write: true,
    can_view_city_registry: true,
    can_view_federal_registry: true,
    ..EMPTY
};


fn declaration(code: &str) -> Option<(CapabilitySet, crate::workspace::model::WorkspaceKind, Vec<crate::workspace::model::WorkspaceSection>)> {
    let caps = if core::chain_runtime::is_tier1_registry(code) { TIER1_REGISTRY }
        else if core::chain_runtime::is_subordinate_registry(code) { SUBORDINATE_REGISTRY } else { return None; };
    Some((caps, crate::workspace::model::WorkspaceKind::Registry, registry::workspace::registry_sections(caps)))
}
fn purposes() -> Vec<String> { vec![citizens::occupy::PURPOSE_CITIZEN_OCCUPY.into(), citizens::occupy::PURPOSE_CITIZEN_ADMIN_REBIND.into(), citizens::occupy::PURPOSE_CITIZEN_REVOKE.into(), citizens::occupy::PURPOSE_CITIZEN_IDENTITY_PUSH.into(), admins::governance::PURPOSE_INSTITUTION_REGISTER_ADMINS.into()] }
fn actions() -> Vec<String> { vec!["CREATE_SUBORDINATE_REGISTRY".into(),"DELETE_SUBORDINATE_REGISTRY".into(),"INSTITUTION_CREATE".into()] }

    #[async_trait::async_trait]
    impl ModuleBusiness for Module {
        fn describe(&self) -> ModuleDescription {
            let mut description = modules::describe_module(
                "registry", VERSION, "frontend/Workspace.tsx", &["RegistryWorkspace", "RegistryAdminsView"],
                self.routes(), purposes(), actions(), declaration,
            );
            description.frontend_workspace = Some("RegistryWorkspace".into());
            description
        }
        fn routes(&self) -> Vec<ModuleRoute> {
            vec![
        modules::model::ModuleRoute::new("admin", "/api/admin/city-registry-admins", &["GET"], get(registry::admins::city_registry_admins::list_city_registry_admins)),
        modules::model::ModuleRoute::new("admin", "/api/admin/federal-registry-admins", &["GET"], get(registry::admins::catalog::list_federal_registry_admins).layer(axum::Extension(self.federal_cache.clone()))),
        modules::model::ModuleRoute::new("admin", "/api/admin/registry/drill-in-project", &["POST"], post(registry::projection::drill_in_project_city)),
        modules::model::ModuleRoute::new("admin", "/api/admin/institution/admins/register/prepare", &["POST"], post(registry::admins::governance::prepare_register_institution_admins)),
        modules::model::ModuleRoute::new("admin", "/api/institutions/check-cid-full-name", &["GET"], get(crate::institutions::subjects::admin::check_cid_full_name)),
        modules::model::ModuleRoute::new("admin", "/api/institutions/search-parents", &["GET"], get(crate::institutions::subjects::admin::search_parent_institutions)),
        modules::model::ModuleRoute::new("admin", "/api/institutions/legal-representative/photo", &["POST"], post(crate::institutions::subjects::admin::upload_legal_representative_photo).layer(
            axum::extract::DefaultBodyLimit::max(5 * 1024 * 1024 + 64 * 1024),
        )).body_limit(5 * 1024 * 1024 + 64 * 1024),
        modules::model::ModuleRoute::new("admin", "/api/institutions/create", &["POST"], post(registry::institutions::registration::create_institution)),
        modules::model::ModuleRoute::new("admin", "/api/private/sole", &["GET", "POST"], get(registry::institutions::private::sole::list).post(registry::institutions::private::sole::create)),
        modules::model::ModuleRoute::new("admin", "/api/private/partnership", &["GET", "POST"], get(registry::institutions::private::partnership::list)
            .post(registry::institutions::private::partnership::create)),
        modules::model::ModuleRoute::new("admin", "/api/private/company", &["GET", "POST"], get(registry::institutions::private::company::list).post(registry::institutions::private::company::create)),
        modules::model::ModuleRoute::new("admin", "/api/private/corporation", &["GET", "POST"], get(registry::institutions::private::corporation::list)
            .post(registry::institutions::private::corporation::create)),
        modules::model::ModuleRoute::new("admin", "/api/private/welfare", &["GET", "POST"], get(registry::institutions::private::welfare::list).post(registry::institutions::private::welfare::create)),
        modules::model::ModuleRoute::new("admin", "/api/private/association", &["GET", "POST"], get(registry::institutions::private::association::list)
            .post(registry::institutions::private::association::create)),
        modules::model::ModuleRoute::new("admin", "/api/institutions/list", &["GET"], get(registry::institutions::registration::list_institutions)),
        modules::model::ModuleRoute::new("admin", "/api/institutions/gov", &["GET"], get(registry::institutions::government::handler::list_official_institutions)),
        modules::model::ModuleRoute::new("admin", "/api/institutions/federal-registry", &["GET"], get(crate::institutions::subjects::admin::get_federal_registry)),
        modules::model::ModuleRoute::new("admin", "/api/admin/audit-logs", &["GET"], get(audit::admin_list_audit_logs)),
        modules::model::ModuleRoute::new("admin", "/api/admin/citizens", &["GET", "POST"], get(registry::citizens::handler::admin_list_citizens)
            .post(registry::citizens::occupy::prepare_citizen_occupy)),
        modules::model::ModuleRoute::new("admin", "/api/admin/citizens/occupy/submit", &["POST"], post(registry::citizens::occupy::submit_citizen_occupy)),
        modules::model::ModuleRoute::new("admin", "/api/admin/citizens/rebind/prepare", &["POST"], post(registry::citizens::occupy::prepare_citizen_rebind)),
        modules::model::ModuleRoute::new("admin", "/api/admin/citizens/rebind/submit", &["POST"], post(registry::citizens::occupy::submit_citizen_rebind)),
        modules::model::ModuleRoute::new("admin", "/api/admin/citizens/{cid_number}/binding", &["GET"], get(registry::citizens::chain_identity::finalized_citizen_binding)),
        modules::model::ModuleRoute::new("admin", "/api/admin/citizens/{cid_number}/edit", &["POST"], post(registry::citizens::admin_entry::admin_update_citizen)),
        modules::model::ModuleRoute::new("admin", "/api/admin/citizens/{cid_number}/onchain/revoke/prepare", &["POST"], post(registry::citizens::occupy::prepare_citizen_revoke)),
        modules::model::ModuleRoute::new("admin", "/api/admin/citizens/legal-representatives", &["GET"], get(registry::citizens::handler::admin_search_legal_representative_citizens)),
        modules::model::ModuleRoute::new("admin", "/api/admin/citizens/{cid_number}/documents", &["GET", "POST"], get(registry::documents::handler::list_citizen_documents)
            .post(registry::documents::handler::upload_citizen_document)
            .layer(crate::documents::upload_body_limit())).body_limit(crate::documents::MAX_REQUEST_BYTES),
        modules::model::ModuleRoute::new("admin", "/api/admin/citizens/{cid_number}/documents/{doc_id}/download", &["GET"], get(registry::documents::handler::download_citizen_document)),
        modules::model::ModuleRoute::new("admin", "/api/admin/citizens/{cid_number}/documents/{doc_id}", &["DELETE"], delete(registry::documents::handler::delete_citizen_document)),
        modules::model::ModuleRoute::new("admin", "/api/admin/citizens/{cid_number}/onchain/prepare", &["POST"], post(registry::citizens::chain_identity::prepare_citizen_onchain_signature)),
        modules::model::ModuleRoute::new("admin", "/api/admin/citizens/{cid_number}/onchain/complete", &["POST"], post(registry::citizens::chain_identity::complete_citizen_onchain_signature)),
        modules::model::ModuleRoute::new("admin", "/api/admin/address/names", &["GET"], get(registry::address::handler::list_address_names)),
        modules::model::ModuleRoute::new("admin", "/api/admin/address/items", &["GET"], get(registry::address::handler::list_addresses)),
        modules::model::ModuleRoute::new("admin", "/api/admin/address/chain-call", &["POST"], post(registry::address::handler::prepare_chain_call)),
        modules::model::ModuleRoute::new("public", "/api/public/identity/search", &["GET"], get(registry::citizens::handler::public_identity_search)),
        modules::model::ModuleRoute::new("app", "/api/app/voters/count", &["GET"], get(registry::citizens::chain_joint_vote::app_voters_count)),
        modules::model::ModuleRoute::new("app", "/api/app/vote/eligibility", &["POST"], post(registry::citizens::chain_vote::app_vote_eligibility)),
            ]
        }
        fn spawn_workers(&self, state: &AppState) { tokio::spawn(registry::projection::entity_projection_worker(state.db.clone())); }
        fn startup(&self, state: &AppState) -> Result<(), String> {
            match registry::projection::genesis::seed_genesis_citizen_blocking(&state.db) {
                Ok(true) => info!("创世公民(法定代表人)已播种进联邦注册局本地库"), Ok(false) => {}, Err(error) => warn!(error = %error, "创世公民播种跳过"),
            }
            match registry::projection::genesis::backfill_genesis_private_blocking(&state.db) {
                Ok(true) => info!("创世私权机构(基金会)已回填进属主市注册局本地库"), Ok(false) => {}, Err(error) => warn!(error = %error, "创世私权机构回填跳过"),
            }
            Ok(())
        }
        fn audit_catalog(&self) -> Result<(), String> { institutions::government::chain_audit::full_audit_blocking() }
        async fn recheck_submit(&self, _state: &AppState, ctx: &authentication::login::AdminAuthContext, _session: &transactions::session::ChainSignSession) -> Result<(), axum::response::Response> { authorization::rules::ensure_registry_admin(ctx) }
        fn validate_submit_context(&self, session: &transactions::session::ChainSignSession) -> Result<(), String> { citizens::occupy::finalized_binding_expectation(session).map(|_| ()) }
        async fn safe_to_resubmit(&self, session: &transactions::session::ChainSignSession) -> bool { citizens::occupy::safe_module_resubmit(session).await }
        async fn verify_finalized_binding(&self, session: &transactions::session::ChainSignSession, finalized: &core::chain_submit::FinalizedChainSubmit) -> Result<Option<core::chain_citizen_identity::FinalizedCitizenIdentity>, axum::response::Response> { citizens::occupy::verify_module_finalized_binding(session, finalized).await }
        async fn commit_projection(&self, db: &Db, session: &transactions::session::ChainSignSession, lease: &transactions::session::SessionLease, finalized: &core::chain_submit::FinalizedChainSubmit, snapshot: Option<&core::chain_citizen_identity::FinalizedCitizenIdentity>) -> Result<Option<serde_json::Value>, axum::response::Response> { citizens::admin_entry::commit_module_projection(db, session, lease, finalized, snapshot).await }
        fn preview_admin(&self, conn: &mut postgres::Client, ctx: &authentication::login::AdminAuthContext, action: &authentication::operation_auth::AdminActionType, payload: &serde_json::Value) -> Result<authentication::actions::ActionPreview, String> { admins::actions::preview_action_conn(conn, ctx, action, payload) }
        fn preview_admin_transaction(&self, conn: &mut postgres::Transaction<'_>, ctx: &authentication::login::AdminAuthContext, action: &authentication::operation_auth::AdminActionType, payload: &serde_json::Value) -> Result<authentication::actions::ActionPreview, String> { admins::actions::preview_action_conn(conn, ctx, action, payload) }
        fn apply_admin_transaction(&self, conn: &mut postgres::Transaction<'_>, ctx: &authentication::login::AdminAuthContext, challenge: &authentication::security_model::AdminActionChallenge) -> Result<serde_json::Value, String> { admins::actions::apply_action_conn(conn, ctx, challenge) }

    }

}
fn local_module() -> Option<Arc<dyn modules::model::ModuleBusiness>> { Some(Arc::new(registry::Module::default())) }
fn main() { run_backend(local_module()); }
