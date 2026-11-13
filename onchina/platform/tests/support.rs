//! 公共真实Router测试构造，调用原生产分发器。
use super::*;
#[cfg(test)]
use axum::http::HeaderMap;
pub fn router_for(module: Arc<dyn modules::model::ModuleBusiness>) -> Router {
    build_routes(AppState {
        db: Db::empty_for_route_test(),
        rate_limiter: Arc::new(LocalRateLimiter::new()),
        modules: Arc::new(modules::ModuleCatalog::new(Some(module))),
    })
}

/// 真实隔离库内签发一份登录快照；故意不建管理员行，读取不得重新查资格或依赖链RPC。
#[cfg(test)]
pub(crate) fn session_fixture(
    db: &Db,
    setup_required: bool,
) -> (AppState, HeaderMap, authentication::login::AdminSession) {
    use authentication::login::{AdminSession, NodeInstitutionBinding};
    let now = chrono::Utc::now();
    let actor = format!("0x{}", "11".repeat(32));
    let institution = "ZS001-FRG07-249474503-2026";
    let binding = NodeInstitutionBinding {
        binding_id: "fixture-binding".into(),
        candidate_id: "fixture-candidate".into(),
        institution_code: "FRG".into(),
        institution_cid_number: institution.into(),
        bound_account_id: actor.clone(),
        bound_at: now,
        status: "ACTIVE".into(),
    };
    let session: AdminSession = serde_json::from_value(serde_json::json!({
        "accountId":actor,"institutionCode":"FRG","candidateId":"fixture-candidate",
        "institutionCidNumber":institution,"adminLevel":"NATIONAL","familyName":"测","givenName":"试",
        "scopeProvinceName":"贵州省","scopeCityName":null,"scopeTownName":null,"cidShortName":"联邦注册局",
        "capabilities":modules::capability::EMPTY,
        "workspace":{"workspace_kind":"registry","workspace_title":"fixture","workspace_sections":[],"workspace_modules":[]},
        "setupRequired":setup_required,"expiresAt":(now+chrono::Duration::hours(1)).timestamp_millis(),"lastActiveAt":now.timestamp_millis()
    })).unwrap();
    let session = AdminSession {
        token: "fixture-session".into(),
        ..session
    };
    db.with_client(|conn| {
        authentication::repo::upsert_active_node_binding_conn(conn, &binding)?;
        authentication::repo::insert_admin_session_conn(conn, &session)
    })
    .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer fixture-session".parse().unwrap());
    (
        AppState {
            db: db.clone(),
            rate_limiter: Arc::new(LocalRateLimiter::new()),
            modules: Arc::new(modules::ModuleCatalog::new(None)),
        },
        headers,
        session,
    )
}

/// 平台自身回归选择公共宿主，不编入正式软件。
#[cfg(test)]
pub(super) fn local_module() -> Option<Arc<dyn modules::model::ModuleBusiness>> {
    None
}
