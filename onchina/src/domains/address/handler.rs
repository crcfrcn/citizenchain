// 地址 handler 的校验辅助函数直接返回统一 Axum Response，错误体不另建包装层。
#![allow(clippy::result_large_err)]

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};

use crate::{
    api_error,
    auth::login::AdminAuthContext,
    cid::china::{city_code_by_name, province_code_by_name, town_exists},
    core::response::ApiResponse,
    require_admin_any, AppState,
};

use super::{
    chain_call::build_address_chain_call,
    model::{AddressChainCallInput, AddressScopeQuery},
    repo,
};

fn ok<T: serde::Serialize>(data: T) -> axum::response::Response {
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data,
    })
    .into_response()
}

fn ensure_code_scope(
    ctx: &AdminAuthContext,
    province_code: &str,
    city_code: &str,
) -> Result<(), axum::response::Response> {
    let Some(scope_province_name) = ctx.scope_province_name.as_deref() else {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "admin province scope missing",
        ));
    };
    let Some(scope_province_code) = province_code_by_name(scope_province_name) else {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "admin province scope invalid",
        ));
    };
    if !scope_province_code.eq_ignore_ascii_case(province_code) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "province out of current admin scope",
        ));
    }
    if let Some(scope_city_name) = ctx.scope_city_name.as_deref() {
        let Some(scope_city_code) = city_code_by_name(scope_province_name, scope_city_name) else {
            return Err(api_error(
                StatusCode::FORBIDDEN,
                1003,
                "admin city scope invalid",
            ));
        };
        if !scope_city_code.eq_ignore_ascii_case(city_code) {
            return Err(api_error(
                StatusCode::FORBIDDEN,
                1003,
                "city out of current admin scope",
            ));
        }
    }
    Ok(())
}

fn checked_scope(
    ctx: &AdminAuthContext,
    query: &AddressScopeQuery,
) -> Result<(String, String, String), axum::response::Response> {
    let province_code = query.province_code.trim().to_string();
    let city_code = query.city_code.trim().to_string();
    let town_code = query.town_code.trim().to_string();
    if province_code.is_empty() || city_code.is_empty() || town_code.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "province_code, city_code and town_code are required",
        ));
    }
    ensure_code_scope(ctx, &province_code, &city_code)?;
    if !town_exists(&province_code, &city_code, &town_code) {
        return Err(api_error(StatusCode::BAD_REQUEST, 1001, "town not found"));
    }
    Ok((province_code, city_code, town_code))
}

pub(crate) async fn list_address_names(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AddressScopeQuery>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let (province_code, city_code, town_code) = match checked_scope(&ctx, &query) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    match repo::list_address_names(
        &province_code,
        &city_code,
        &town_code,
        query.page_size,
        query.cursor,
    ) {
        Ok(page) => ok(page),
        Err(err) => {
            tracing::warn!(error = %err, "address name query failed");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                5001,
                "address name query failed",
            )
        }
    }
}

pub(crate) async fn list_addresses(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AddressScopeQuery>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let (province_code, city_code, town_code) = match checked_scope(&ctx, &query) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let Some(address_name_code) = query
        .address_name_code
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    else {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "address_name_code is required",
        );
    };
    match repo::list_addresses(
        &province_code,
        &city_code,
        &town_code,
        address_name_code,
        query.page_size,
        query.cursor,
    ) {
        Ok(page) => ok(page),
        Err(err) => {
            tracing::warn!(error = %err, "address query failed");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                5001,
                "address query failed",
            )
        }
    }
}

pub(crate) async fn prepare_chain_call(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<AddressChainCallInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    // 链上写(PasskeyColdSign 档):本入口产出待冷签的地址上链 call,
    // 不经冷签会话表,拿不到 PasskeyProof 的类型约束,故在此显式断言。
    if let Err(resp) =
        crate::auth::passkey::require_passkey_assertion(&state, &headers, ctx.account_id.as_str())
    {
        return resp;
    }
    if let (Some(province_code), Some(city_code)) = (&input.province_code, &input.city_code) {
        if let Err(resp) = ensure_code_scope(&ctx, province_code.trim(), city_code.trim()) {
            return resp;
        }
    }
    let actor_cid_number =
        match crate::domains::citizens::chain_identity::active_registry_cid_number(&state) {
            Ok(cid_number) => cid_number,
            Err(resp) => return resp,
        };
    match build_address_chain_call(&actor_cid_number, &input) {
        Ok(output) => ok(output),
        Err(err) => api_error(StatusCode::BAD_REQUEST, 1001, err.as_str()),
    }
}
