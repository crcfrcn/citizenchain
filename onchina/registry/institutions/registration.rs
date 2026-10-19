//! 机构注册与列表 HTTP 入口。
//!
//! 旧 `PublicManage/PrivateManage` call 5 直接创建路径已经关闭。创建接口当前明确返回
//! `501`，不得生成旧冷签载荷或落本地机构投影；待基础业务模块实现原子提交管理员、
//! LR、初始治理岗位、权限、任职与投票规则后，再在本入口接入新协议。

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};

use crate::authentication::login::{require_admin_any, AdminAuthContext};
use crate::crypto::pubkey::{normalize_account_id, same_account_id};
use crate::cid::china::{city_code_by_name, province_code_by_name};
use crate::institutions::subjects::model::{
    CreateInstitutionInput, InstitutionListFilter, InstitutionListRow,
};
use crate::authorization::get_visible_scope;
use crate::*;

pub(crate) async fn create_institution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CreateInstitutionInput>,
) -> impl IntoResponse {
    create_institution_inner(state, headers, input, false).await
}

pub(crate) async fn create_private_institution(
    state: AppState,
    headers: HeaderMap,
    input: CreateInstitutionInput,
) -> Response {
    create_institution_inner(state, headers, input, true).await
}

async fn create_institution_inner(
    _state: AppState,
    _headers: HeaderMap,
    _input: CreateInstitutionInput,
    _allow_private: bool,
) -> Response {
    api_error(
        StatusCode::NOT_IMPLEMENTED,
        1004,
        "旧机构直接创建入口已关闭；需等待原子提交初始岗位、权限、任职和投票规则的新业务模块",
    )
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct ListInstitutionQuery {
    pub category: Option<String>,
    pub private_type: Option<String>,
    pub province_name: Option<String>,
    pub city_name: Option<String>,
    pub q: Option<String>,
    pub cursor: Option<String>,
    pub page_size: Option<usize>,
}

pub(crate) async fn list_institutions(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<ListInstitutionQuery>,
) -> impl IntoResponse {
    list_institutions_inner(state, headers, query, false).await
}

pub(crate) async fn list_private_institutions(
    state: AppState,
    headers: HeaderMap,
    query: ListInstitutionQuery,
) -> Response {
    list_institutions_inner(state, headers, query, true).await
}

async fn list_institutions_inner(
    state: AppState,
    headers: HeaderMap,
    query: ListInstitutionQuery,
    allow_private: bool,
) -> Response {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if let Err(resp) = crate::authorization::rules::ensure_registry_admin(&ctx) {
        return resp;
    }
    let scope = get_visible_scope(&ctx);
    let filter = match query.category.as_deref() {
        Some("PRIVATE_INSTITUTION") => {
            if !allow_private {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    1001,
                    "私权机构必须使用 /api/private/<type> 查询",
                );
            }
            InstitutionListFilter::Private
        }
        Some("GOV_INSTITUTION") => InstitutionListFilter::Gov,
        Some("EDUCATION_FORM") => InstitutionListFilter::Education,
        _ => {
            return api_error(
                StatusCode::BAD_REQUEST,
                1001,
                "institution category is required",
            );
        }
    };
    let page_size = query.page_size.unwrap_or(50).clamp(1, 100);
    let empty_page = || PageResult::<InstitutionListRow> {
        items: Vec::new(),
        page_size,
        next_cursor: None,
        has_more: false,
        manifest_version: None,
        catalog_status: None,
    };
    if let (Some(locked), Some(requested)) = (&scope.locked_province_name, &query.province_name) {
        if locked != requested {
            return Json(ApiResponse {
                code: 0,
                message: "ok".to_string(),
                data: empty_page(),
            })
            .into_response();
        }
    }
    if let (Some(locked), Some(requested)) = (&scope.locked_city_name, &query.city_name) {
        if locked != requested {
            return Json(ApiResponse {
                code: 0,
                message: "ok".to_string(),
                data: empty_page(),
            })
            .into_response();
        }
    }
    let Some(province_name) = scope
        .locked_province_name
        .as_deref()
        .or(query.province_name.as_deref())
    else {
        return api_error(StatusCode::FORBIDDEN, 1003, "province scope required");
    };
    let Some(province_code) = province_code_by_name(province_name) else {
        return api_error(StatusCode::BAD_REQUEST, 1001, "unknown province");
    };
    let city_code = match scope
        .locked_city_name
        .as_deref()
        .or(query.city_name.as_deref())
    {
        Some(city_name) => match city_code_by_name(province_name, city_name) {
            Some(code) => Some(code),
            None => return api_error(StatusCode::BAD_REQUEST, 1001, "unknown city"),
        },
        None => None,
    };
    // 联邦(Tier1)省组管理员按市查看私权机构:先把该市链上私权机构 drill-in 投影进联邦本地库,
    // 再列。链读投影失败(如链不可达)只告警不阻断,列表仍返回本地已有(fail-open 展示)。
    if matches!(filter, InstitutionListFilter::Private)
        && crate::core::chain_runtime::is_tier1_registry(ctx.institution_code.as_str())
    {
        if let Some(city) = city_code {
            let scope = crate::institutions::catalog::projection::NodeScope {
                province_code: province_code.to_string(),
                city_code: city.to_string(),
            };
            if let Err(err) =
                crate::registry::projection::drill_in_project_private_scope(&state.db, &scope).await
            {
                tracing::warn!(error = %err, "federal drill-in private projection failed");
            }
        }
    }
    let page = match state.db.list_institutions_exact(
        filter,
        query.private_type.as_deref(),
        province_code,
        city_code,
        query.q.as_deref().unwrap_or(""),
        query.cursor.as_deref(),
        page_size,
    ) {
        Ok(v) => v,
        Err(e) if e == "invalid page cursor" => {
            return api_error(StatusCode::BAD_REQUEST, 1001, "invalid page cursor");
        }
        Err(err) => {
            let message = format!("institution query failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: page,
    })
    .into_response()
}

/// 注册局既有创建范围预检；保留原有字段校验与错误返回。
pub(crate) fn precheck_institution_create_scope(
    ctx: &AdminAuthContext,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let scope = crate::authorization::rules::get_visible_scope(ctx);
    check_locked_field(
        scope.locked_province_name.as_deref(),
        payload.get("province_name").and_then(|v| v.as_str()),
        "province",
    )?;
    check_locked_field(
        scope.locked_city_name.as_deref(),
        payload.get("city_name").and_then(|v| v.as_str()),
        "city",
    )?;
    check_locked_field(
        scope.locked_town_name.as_deref(),
        payload.get("town_name").and_then(|v| v.as_str()),
        "town",
    )?;
    let admins = payload
        .get("admins")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "http:bad_request:admins is required".to_string())?;
    if admins.len() < 2 {
        return Err(
            "http:bad_request:institution admins must contain at least two accounts".to_string(),
        );
    }
    let mut normalized_account_ids: Vec<String> = Vec::with_capacity(admins.len());
    for item in admins {
        let raw = item
            .get("account_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| "http:bad_request:account_id is required".to_string())?;
        let Some(normalized) = normalize_account_id(raw) else {
            return Err("http:bad_request:account_id format invalid".to_string());
        };
        if normalized_account_ids
            .iter()
            .any(|account| same_account_id(account, normalized.as_str()))
        {
            return Err("http:bad_request:duplicate account_id".to_string());
        }
        normalized_account_ids.push(normalized);
    }
    Ok(())
}

fn check_locked_field(
    locked: Option<&str>,
    requested: Option<&str>,
    field: &str,
) -> Result<(), String> {
    if let (Some(locked), Some(requested)) =
        (locked, requested.map(str::trim).filter(|v| !v.is_empty()))
    {
        if requested != locked {
            return Err(format!("http:forbidden:{field} out of current admin scope"));
        }
    }
    Ok(())
}
