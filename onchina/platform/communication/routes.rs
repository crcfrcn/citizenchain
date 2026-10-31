// 平台只登记公共路由；业务路由由独立模块提供。
fn platform_routes() -> Vec<modules::model::ModuleRoute> {
    vec![
        modules::model::ModuleRoute::new(
            "auth",
            "/api/admin/auth/check",
            &["GET"],
            get(authentication::login::admin_auth_check),
        ),
        modules::model::ModuleRoute::new(
            "auth",
            "/api/admin/auth/logout",
            &["POST"],
            post(authentication::login::admin_logout),
        ),
        modules::model::ModuleRoute::new(
            "auth",
            "/api/admin/auth/qr/sign-request",
            &["POST"],
            post(authentication::login::admin_auth_qr_sign_request),
        ),
        modules::model::ModuleRoute::new(
            "auth",
            "/api/admin/auth/qr/complete",
            &["POST"],
            post(authentication::login::admin_auth_qr_complete),
        ),
        modules::model::ModuleRoute::new(
            "auth",
            "/api/admin/auth/qr/result",
            &["GET"],
            get(authentication::login::admin_auth_qr_result),
        ),
        modules::model::ModuleRoute::new(
            "auth",
            "/api/admin/auth/node-binding/confirm",
            &["POST"],
            post(authentication::login::admin_auth_confirm_node_binding),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/admin/actions/prepare",
            &["POST"],
            post(authentication::actions::prepare_admin_action),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/admin/actions/commit",
            &["POST"],
            post(authentication::actions::commit_admin_action),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/admin/auth/passkey/register/begin",
            &["POST"],
            post(authentication::passkey::register_begin),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/admin/auth/passkey/register/finish",
            &["POST"],
            post(authentication::passkey::register_finish),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/admin/auth/passkey/assert/begin",
            &["POST"],
            post(authentication::passkey::assert_begin),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/admin/auth/passkey/assert/finish",
            &["POST"],
            post(authentication::passkey::assert_finish),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/admin/auth/passkey/status",
            &["GET"],
            get(authentication::passkey::passkey_status),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/admin/own-institution-admins",
            &["GET"],
            get(institutions::catalog::list_own_institution_admins),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/admin/own-institution",
            &["GET"],
            get(institutions::catalog::get_own_institution),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/admin/institution/governance/prepare",
            &["POST"],
            post(institutions::admins::prepare_institution_governance),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/institutions/legal-representative/photo/{photo_id}",
            &["GET"],
            get(institutions::subjects::admin::read_legal_representative_photo),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/institutions/{cid_number}/account/create",
            &["POST"],
            post(institutions::accounts::handler::create_account),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/institutions/{cid_number}",
            &["GET", "PATCH"],
            get(institutions::subjects::admin::get_institution)
                // 详情页只更新机构资料;私权类型由创建时身份编码锁定,不可在此改。
                .patch(institutions::subjects::admin::update_institution),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/institutions/{cid_number}/accounts",
            &["GET"],
            get(institutions::accounts::handler::list_accounts),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/institutions/{cid_number}/account/{account_name}",
            &["DELETE"],
            delete(institutions::accounts::handler::delete_account),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/institutions/{cid_number}/docs",
            &["GET", "POST"],
            get(documents::handler::list_documents)
                .post(documents::handler::upload_document)
                .layer(documents::upload_body_limit()),
        )
        .body_limit(documents::MAX_REQUEST_BYTES),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/institutions/{cid_number}/docs/{doc_id}/download",
            &["GET"],
            get(documents::handler::download_document),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/institutions/{cid_number}/docs/{doc_id}",
            &["DELETE"],
            delete(documents::handler::delete_document),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/admin/chain/submit",
            &["POST"],
            post(transactions::handler::submit_chain_sign),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/admin/cid/meta",
            &["GET"],
            get(crate::codes::admin::admin_cid_meta),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/admin/cid/china/cities",
            &["GET"],
            get(crate::codes::china::admin::admin_china_cities),
        ),
        modules::model::ModuleRoute::new(
            "admin",
            "/api/admin/cid/china/towns",
            &["GET"],
            get(crate::codes::china::admin::admin_china_towns),
        ),
        modules::model::ModuleRoute::new("public", "/api/health", &["GET"], get(health)),
        modules::model::ModuleRoute::new(
            "public",
            "/api/public/cid/labels",
            &["GET"],
            get(crate::codes::admin::public_cid_labels),
        ),
        modules::model::ModuleRoute::new(
            "app",
            "/api/app/accounts/{account_id}/transactions",
            &["GET"],
            get(indexer::api::account_transactions),
        ),
        modules::model::ModuleRoute::new(
            "app",
            "/api/app/institutions/search",
            &["GET"],
            get(institutions::subjects::chain_multisig_info::app_search_institutions),
        ),
        modules::model::ModuleRoute::new(
            "app",
            "/api/app/institutions/{cid_number}",
            &["GET"],
            get(institutions::subjects::chain_multisig_info::app_get_institution),
        ),
        modules::model::ModuleRoute::new(
            "app",
            "/api/app/institutions/{cid_number}/accounts",
            &["GET"],
            get(institutions::subjects::chain_multisig_info::app_list_accounts),
        ),
        modules::model::ModuleRoute::new(
            "app",
            "/api/app/public-institutions",
            &["GET"],
            get(citizenapp::public_institution::list_public_institutions),
        ),
        modules::model::ModuleRoute::new(
            "app",
            "/api/app/public-institutions/version",
            &["GET"],
            get(citizenapp::public_institution::public_institutions_version),
        ),
    ]
}

/// 公共路由、模块真实路由或宿主转发路由在同一入口组装，保留原中间件顺序。
fn build_routes(state: AppState) -> Router {
    let host = state.modules.local.is_none();
    let mut public = Router::new();
    let mut auth = Router::new();
    let mut admin = Router::new();
    let mut app = Router::new();
    let mut routes = if host {
        platform_routes()
    } else {
        platform_routes()
            .into_iter()
            .filter(|r| {
                matches!(
                    r.description.path.as_str(),
                    "/api/health"
                        | "/api/admin/actions/prepare"
                        | "/api/admin/actions/commit"
                        | "/api/admin/chain/submit"
                )
            })
            .collect()
    };
    if let Some(local) = &state.modules.local {
        routes.extend(local.routes());
    }
    for route in routes {
        let router = match route.description.group.as_str() {
            "auth" => &mut auth,
            "admin" => &mut admin,
            "app" => &mut app,
            _ => &mut public,
        };
        *router = std::mem::take(router).route(&route.description.path, route.handler);
    }
    if host {
        for (owner, route) in state.modules.routes() {
            let methods = route
                .methods
                .iter()
                .map(|method| match method.as_str() {
                    "GET" => axum::routing::MethodFilter::GET.or(axum::routing::MethodFilter::HEAD),
                    "POST" => axum::routing::MethodFilter::POST,
                    "PATCH" => axum::routing::MethodFilter::PATCH,
                    "PUT" => axum::routing::MethodFilter::PUT,
                    "DELETE" => axum::routing::MethodFilter::DELETE,
                    "HEAD" => axum::routing::MethodFilter::HEAD,
                    "OPTIONS" => axum::routing::MethodFilter::OPTIONS,
                    _ => panic!("unsupported module route method"),
                })
                .reduce(axum::routing::MethodFilter::or)
                .expect("validated module route requires a method");
            let limit = route.body_limit;
            let handler = axum::routing::on(
                methods,
                move |axum::extract::State(state): axum::extract::State<AppState>,
                      request: axum::extract::Request| {
                    let owner = owner.clone();
                    async move { modules::forward_request(&state, &owner, request, limit).await }
                },
            );
            let router = match route.group.as_str() {
                "auth" => &mut auth,
                "admin" => &mut admin,
                "app" => &mut app,
                _ => &mut public,
            };
            *router = std::mem::take(router).route(&route.path, handler);
        }
    } else {
        public = public.route("/internal/workspace", get(modules::internal_workspace));
    }
    admin = admin.route_layer(middleware::from_fn_with_state(
        state.clone(),
        authentication::login::require_admin_session_middleware,
    ));
    let mut router = Router::new()
        .merge(public)
        .merge(auth)
        .merge(admin)
        .merge(app);
    if host {
        for (id, frontend) in state.modules.static_frontends() {
            let service = Router::new()
                .fallback_service(tower_http::services::ServeDir::new(frontend))
                .layer(middleware::from_fn(
                    crate::security_http::static_cache_headers,
                ));
            router = router.nest_service(&format!("/modules/{id}"), service);
        }
        router = router
            .layer(middleware::from_fn_with_state(
                state.clone(),
                global_rate_limit_middleware,
            ))
            .layer(build_cors_layer());
        let frontend_dist = std::env::var("ONCHINA_FRONTEND_DIST")
            .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/frontend/dist").to_string());
        let frontend_index = format!("{frontend_dist}/index.html");
        let frontend_service = Router::new()
            .fallback_service(
                tower_http::services::ServeDir::new(&frontend_dist)
                    .not_found_service(tower_http::services::ServeFile::new(frontend_index)),
            )
            .layer(middleware::from_fn(
                crate::security_http::static_cache_headers,
            ))
            .layer(middleware::from_fn(missing_api_or_frontend));
        router = router.fallback_service(frontend_service);
    }
    router.with_state(state)
}

// 已移出的模块没有业务路由；API缺失必须返回原错误格式，不能回成SPA的HTML。
async fn missing_api_or_frontend(
    request: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    if request.uri().path().starts_with("/api/") {
        return api_error(
            StatusCode::NOT_FOUND,
            1004,
            "endpoint or module unavailable",
        );
    }
    next.run(request).await
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/communication_routes_route_tests.rs"]
mod route_tests;
