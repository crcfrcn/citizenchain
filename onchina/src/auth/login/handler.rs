//! 管理员会话检查、登出与节点机构绑定接口。
//!
//! 登录只保留 `qr_login.rs` 中的定向二维码流程，本文件不再承载旁路挑战登录。

use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;

use crate::auth::repo;
use crate::*;

use super::guards::{admin_auth, bearer_token};
use super::model::*;
use super::onchain_gate;

pub(crate) async fn require_admin_session_middleware(
    State(state): State<AppState>,
    request: Request,
    next: middleware::Next,
) -> Response {
    if let Err(resp) = admin_auth(&state, request.headers()) {
        return resp;
    }
    next.run(request).await
}

pub(crate) async fn admin_auth_check(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let ctx = match admin_auth(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let capabilities = crate::platform::capability::capabilities_for(&ctx.institution_code);
    let workspace_modules =
        crate::domains::membership::workspace_modules_for(&ctx.institution_cid_number).await;
    let workspace = crate::workspace::build_institution_workspace(
        &ctx.institution_code,
        ctx.cid_short_name.as_deref(),
        capabilities,
        workspace_modules,
    );
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: AdminAuthOutput {
            ok: true,
            account_id: ctx.account_id,
            institution_cid_number: ctx.institution_cid_number,
            institution_code: ctx.institution_code,
            admin_level: ctx.admin_level,
            capabilities,
            workspace,
            family_name: ctx.family_name,
            given_name: ctx.given_name,
            scope_province_name: ctx.scope_province_name,
            scope_city_name: ctx.scope_city_name,
            scope_town_name: ctx.scope_town_name,
            cid_short_name: ctx.cid_short_name,
        },
    })
    .into_response()
}

/// 主动登出:从结构化会话表删除当前 session。
pub(crate) async fn admin_logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let token = match bearer_token(&headers) {
        Some(t) => t,
        None => return api_error(StatusCode::BAD_REQUEST, 1001, "missing token"),
    };
    if let Err(err) = repo::delete_admin_session(&state.db, token.as_str()) {
        let message = format!("delete session failed: {err}");
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
    }
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: "logged out",
    })
    .into_response()
}

pub(crate) async fn admin_auth_confirm_node_binding(
    State(state): State<AppState>,
    Json(input): Json<NodeBindingConfirmInput>,
) -> impl IntoResponse {
    let now = Utc::now();
    let (access_token, expire_at, admin) =
        match onchain_gate::confirm_node_binding_after_onchain_gate(
            &state,
            input.binding_challenge_id.as_str(),
            input.candidate_id.as_str(),
            now,
        )
        .await
        {
            Ok(v) => v,
            Err(err) => return onchain_gate::gate_error_response(err),
        };
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: AdminVerifyOutput {
            access_token,
            expire_at: expire_at.timestamp(),
            admin,
        },
    })
    .into_response()
}
