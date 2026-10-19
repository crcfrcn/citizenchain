//! 公民列表 / 公开身份查询 handlers
//!
//! 公民查询能力属于 citizens 模块,不属于权限范围规则。
//! 因此本文件承接后台公民列表和公开身份查询入口。

// 公民 handler 的失败分支统一携带完整 Axum Response，避免公开与管理入口形成双轨错误模型。
#![allow(clippy::result_large_err)]

use crate::registry::citizens::model::{citizen_row_from_record, citizen_status_text};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};

use crate::*;

pub(crate) async fn admin_list_citizens(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CitizensQuery>,
) -> impl IntoResponse {
    let auth_ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let (scope_province_code, scope_city_code) =
        match resolve_citizen_query_scope(&auth_ctx, &query) {
            Ok(v) => v,
            Err(resp) => return resp,
        };

    let keyword = query.keyword.unwrap_or_default();
    let page_size = query.page_size.or(query.limit).unwrap_or(50).clamp(1, 100);
    if query.offset.unwrap_or(0) > 0 {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "offset pagination is not supported",
        );
    }
    let page = match state.db.list_citizens_page(
        keyword.as_str(),
        scope_province_code.as_deref(),
        scope_city_code.as_deref(),
        query.cursor.as_deref(),
        page_size,
    ) {
        Ok(v) => v,
        Err(e) if e == "invalid page cursor" => {
            return api_error(StatusCode::BAD_REQUEST, 1001, "invalid page cursor");
        }
        Err(e) => {
            tracing::warn!(error = %e, "admin_list_citizens failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "citizen query failed",
            );
        }
    };

    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: page,
    })
    .into_response()
}

fn resolve_citizen_query_scope(
    auth_ctx: &crate::authentication::login::AdminAuthContext,
    query: &CitizensQuery,
) -> Result<(Option<String>, Option<String>), axum::response::Response> {
    crate::registry::citizens::chain_identity::ensure_registry_admin(auth_ctx)?;
    let scope = crate::authorization::get_visible_scope(auth_ctx);
    if !scope.can_write {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "当前登录无公民办理权限",
        ));
    }
    let province_name = query
        .province_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .or_else(|| scope.locked_province_name.clone())
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, 1001, "province_name is required"))?;
    let city_name = query
        .city_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .or_else(|| scope.locked_city_name.clone())
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, 1001, "city_name is required"))?;
    if !scope.includes_province(province_name.as_str()) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "province_name out of current admin scope",
        ));
    }
    if !scope.includes_city(city_name.as_str()) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "city_name out of current admin scope",
        ));
    }
    let Some(province) = crate::cid::china::provinces()
        .iter()
        .find(|p| p.province_name == province_name)
    else {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "unknown province_name",
        ));
    };
    let Some(city) = province.cities.iter().find(|c| c.city_name == city_name) else {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "unknown city_name",
        ));
    };
    Ok((
        Some(province.province_code.to_string()),
        Some(city.city_code.to_string()),
    ))
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct LegalRepresentativeCitizenQuery {
    pub q: Option<String>,
    pub page_size: Option<usize>,
    pub target_cid_number: Option<String>,
    pub province_name: Option<String>,
    pub city_name: Option<String>,
    pub institution: Option<String>,
    pub education_type: Option<String>,
    pub parent_cid_number: Option<String>,
}

fn legal_representative_scope_from_existing_target(
    state: &AppState,
    auth_ctx: &crate::authentication::login::AdminAuthContext,
    target_cid_number: &str,
) -> Result<
    crate::institutions::subjects::service::LegalRepresentativeCitizenScope,
    axum::response::Response,
> {
    let Some((target, _)) = (match state.db.get_institution_with_accounts(target_cid_number) {
        Ok(v) => v,
        Err(err) => {
            let message = format!("query institution failed: {err}");
            return Err(api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                5001,
                message.as_str(),
            ));
        }
    }) else {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            1004,
            "institution not found",
        ));
    };
    crate::institutions::subjects::http::ensure_institution_visible_to_admin(&target, auth_ctx)?;

    let parent = match target.parent_cid_number.as_deref() {
        Some(parent_cid) if !parent_cid.trim().is_empty() => {
            match state.db.get_institution_with_accounts(parent_cid) {
                Ok(v) => v.map(|(parent, _)| parent),
                Err(err) => {
                    let message = format!("query parent institution failed: {err}");
                    return Err(api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        5001,
                        message.as_str(),
                    ));
                }
            }
        }
        _ => None,
    };
    Ok(
        crate::institutions::subjects::service::resolve_legal_representative_scope_for_institution(
            &target,
            parent.as_ref(),
        ),
    )
}

fn legal_representative_scope_from_create_context(
    state: &AppState,
    auth_ctx: &crate::authentication::login::AdminAuthContext,
    query: &LegalRepresentativeCitizenQuery,
) -> Result<
    crate::institutions::subjects::service::LegalRepresentativeCitizenScope,
    axum::response::Response,
> {
    let province_name = query
        .province_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, 1001, "province_name is required"))?;
    let city_name = query
        .city_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, 1001, "city_name is required"))?;
    let scope = crate::authorization::get_visible_scope(auth_ctx);
    if !scope.includes_province(province_name) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "province_name out of current admin scope",
        ));
    }
    if !scope.includes_city(city_name) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "city_name out of current admin scope",
        ));
    }
    let institution = query
        .institution
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, 1001, "institution is required"))?;
    let Some(province_code) = crate::cid::china::province_code_by_name(province_name) else {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "unknown province_name",
        ));
    };
    let Some(city_code) = crate::cid::china::city_code_by_name(province_name, city_name) else {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "unknown city_name",
        ));
    };

    let parent = match query.parent_cid_number.as_deref().map(str::trim) {
        Some(parent_cid) if !parent_cid.is_empty() => {
            let Some((parent, _)) = (match state.db.get_institution_with_accounts(parent_cid) {
                Ok(v) => v,
                Err(err) => {
                    let message = format!("query parent institution failed: {err}");
                    return Err(api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        5001,
                        message.as_str(),
                    ));
                }
            }) else {
                return Err(api_error(StatusCode::NOT_FOUND, 1004, "所属法人机构不存在"));
            };
            if !crate::institutions::subjects::unincorporated_org::can_attach_to_parent(
                parent.institution_code.as_str(),
            ) {
                return Err(api_error(
                    StatusCode::BAD_REQUEST,
                    1001,
                    crate::institutions::subjects::unincorporated_org::parent_subject_requirement_message(),
                ));
            }
            Some(parent)
        }
        _ if crate::institutions::subjects::unincorporated_org::requires_parent(institution) => {
            return Err(api_error(StatusCode::BAD_REQUEST, 1001, "请先选择所属法人"));
        }
        _ => None,
    };

    Ok(
        crate::institutions::subjects::service::resolve_legal_representative_scope_for_codes(
            institution,
            query.education_type.as_deref().map(str::trim),
            province_code,
            city_code,
            parent.as_ref(),
        ),
    )
}

pub(crate) async fn admin_search_legal_representative_citizens(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LegalRepresentativeCitizenQuery>,
) -> impl IntoResponse {
    let auth_ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if let Err(resp) = crate::registry::citizens::chain_identity::ensure_registry_admin(&auth_ctx) {
        return resp;
    }
    let q = query.q.as_deref().map(str::trim).unwrap_or("").to_string();
    if q.is_empty() {
        return Json(ApiResponse {
            code: 0,
            message: "ok".to_string(),
            data: Vec::<String>::new(),
        })
        .into_response();
    }
    let legal_representative_scope = match query
        .target_cid_number
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        Some(target_cid_number) => {
            match legal_representative_scope_from_existing_target(
                &state,
                &auth_ctx,
                target_cid_number,
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            }
        }
        None => match legal_representative_scope_from_create_context(&state, &auth_ctx, &query) {
            Ok(v) => v,
            Err(resp) => return resp,
        },
    };
    let page_size = query.page_size.unwrap_or(20).clamp(1, 50);
    let rows = match state.db.search_legal_representative_citizens_in_scope(
        &q,
        page_size,
        &legal_representative_scope,
    ) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "legal representative citizen search failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "citizen query failed",
            );
        }
    };
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: rows,
    })
    .into_response()
}

pub(crate) async fn public_identity_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PublicIdentitySearchQuery>,
) -> impl IntoResponse {
    // 查询结果仅含公开信息（CID 码等），无需 token 认证。
    // 全局 rate limiter 已防滥用。
    // 公开查询只返回公民档案已登记后的公开字段。
    let identity_code = query.identity_code.as_deref().map(str::trim).unwrap_or("");
    let account_id = query.account_id.as_deref().map(str::trim).unwrap_or("");
    if identity_code.is_empty() && account_id.is_empty() {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "identity_code or account_id is required",
        );
    }
    if !account_id.is_empty() && crate::crypto::pubkey::normalize_account_id(account_id).is_none() {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "account_id must be lowercase 0x plus 64 hexadecimal characters",
        );
    }

    let actor_ip = actor_ip_from_headers(&headers);
    let request_id = request_id_from_headers(&headers);
    let found = match state.db.with_client({
        let identity_code = identity_code.to_string();
        let account_id = account_id.to_string();
        move |conn| {
            let row = conn
                .query_opt(
                    "SELECT cid_number, account_id
                     FROM citizens
                     WHERE (
                            ($1::text <> '' AND cid_number = $1)
                            OR ($2::text <> '' AND account_id = $2)
                       )
                     ORDER BY created_at DESC
                     LIMIT 1",
                    &[&identity_code, &account_id],
                )
                .map_err(|e| format!("public citizen lookup failed: {e}"))?;
            Ok(row.map(|row| (row.get::<_, String>(0), row.get::<_, Option<String>>(1))))
        }
    }) {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(error = %err, "public_identity_search failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "identity query failed",
            );
        }
    };
    let output = PublicIdentitySearchOutput {
        found: found.is_some(),
        identity_code: found.as_ref().map(|r| r.0.clone()),
        account_id: found.as_ref().and_then(|r| r.1.clone()),
    };
    crate::core::runtime_ops::append_audit_log(
        &state,
        "PUBLIC_IDENTITY_SEARCH",
        "public",
        output.account_id.clone(),
        serde_json::json!({
            "found": output.found,
            "request_id": request_id.clone(),
            "actor_ip": actor_ip.clone(),
        }),
    );
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: output,
    })
    .into_response()
}
