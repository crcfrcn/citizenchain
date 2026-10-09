//! 联邦注册局管理员查看市注册局管理员列表与内部变更辅助函数。
//!
//! 联邦注册局管理员/市注册局管理员只通过 admins 结构化表查询同省域市注册局管理员,不做全量内存过滤。

// 鉴权失败需要原样返回统一 Axum Response，避免为单个辅助函数引入第二套错误包装。
#![allow(clippy::result_large_err)]

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use postgres::Client;
use std::collections::BTreeMap;

use crate::auth::login::admin_person_names;
use crate::auth::repo;
use crate::*;

fn balance_lookup_key(account_id: &str) -> String {
    crate::crypto::pubkey::normalize_account_id(account_id).unwrap_or_default()
}

fn balance_fen(balances: &BTreeMap<String, Option<String>>, account_id: &str) -> Option<String> {
    balances
        .get(balance_lookup_key(account_id).as_str())
        .cloned()
        .flatten()
}

pub(crate) const MAX_ADMIN_PERSON_NAME_BYTES: usize = 128;
pub(crate) const MAX_CITY_REGISTRY_ADMINS_PER_CITY: usize = 30;

pub(crate) async fn list_city_registry_admins(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if crate::core::chain_runtime::is_subordinate_registry(&ctx.institution_code)
        && ctx
            .scope_city_name
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
    {
        return api_error(StatusCode::FORBIDDEN, 1003, "admin city scope missing");
    }
    if ctx
        .scope_province_name
        .as_deref()
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return api_error(StatusCode::FORBIDDEN, 1003, "admin province scope missing");
    }
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let offset = query.offset.unwrap_or(0);
    let actor_province_name = ctx.scope_province_name.clone();
    let actor_city_name =
        if crate::core::chain_runtime::is_subordinate_registry(&ctx.institution_code) {
            ctx.scope_city_name.clone()
        } else {
            None
        };
    let result = state.db.with_client(move |conn| {
        // 省作用域校验保留(缺省即登录投影错误);列表按机构码 + 市过滤(每节点单省,省隐含)。
        actor_province_name
            .as_deref()
            .ok_or_else(|| "admin province scope missing".to_string())?;
        let (total, admins) = repo::list_city_registry_admins_by_scope_conn(
            conn,
            actor_city_name.as_deref(),
            limit,
            offset,
        )?;
        let rows = admins
            .into_iter()
            .map(|user| city_registry_row_from_user_conn(conn, &user))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CityRegistryAdminListOutput {
            total,
            limit,
            offset,
            rows,
        })
    });
    let mut data = match result {
        Ok(v) => v,
        Err(err) => {
            let message = format!("query city registry admins failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };
    let balance_account_ids = data
        .rows
        .iter()
        .map(|row| row.account_id.clone())
        .collect::<Vec<_>>();
    let balance_by_account_id = match crate::core::chain_runtime::fetch_account_balances_onchain(
        &balance_account_ids,
    )
    .await
    {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(error = %err, "chain balance unavailable listing city registry admins");
            BTreeMap::new()
        }
    };
    for row in &mut data.rows {
        row.balance_fen = balance_fen(&balance_by_account_id, row.account_id.as_str());
    }
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data,
    })
    .into_response()
}

pub(crate) fn can_manage_city_registry(
    actor_province_name: Option<&str>,
    city_registry: &AdminUser,
) -> bool {
    let Some(scope) = actor_province_name else {
        return false;
    };
    let Some(city_code) = crate::cid::china::city_code_by_name(scope, &city_registry.city_name)
    else {
        return false;
    };
    city_code != "000"
}

pub(crate) fn find_city_registry_by_id_conn(
    conn: &mut Client,
    id: u64,
) -> Result<Option<AdminUser>, String> {
    repo::get_admin_by_id_and_registry_org_conn(conn, id, "CREG")
}

pub(crate) fn city_registry_row_from_user_conn(
    conn: &mut Client,
    city_registry: &AdminUser,
) -> Result<CityRegistryAdminRow, String> {
    let (creator_family_name, creator_given_name) =
        creator_person_names_conn(conn, city_registry.creator_account_id.as_str())?;
    Ok(CityRegistryAdminRow {
        id: city_registry.id,
        account_id: city_registry.account_id.clone(),
        family_name: city_registry.family_name.clone(),
        given_name: city_registry.given_name.clone(),
        balance_fen: None,
        institution_code: city_registry.institution_code.clone(),
        built_in: city_registry.built_in,
        creator_account_id: city_registry.creator_account_id.clone(),
        creator_family_name,
        creator_given_name,
        created_at: city_registry.created_at,
        city_name: city_registry.city_name.clone(),
    })
}

pub(crate) fn count_city_registry_admins_in_city_conn(
    conn: &mut Client,
    province: &str,
    city: &str,
) -> Result<usize, String> {
    let _ = province;
    let city = city.trim();
    repo::count_city_registry_admins_by_city_conn(conn, city)
}

pub(crate) fn creator_person_names_conn(
    conn: &mut Client,
    creator_account_id: &str,
) -> Result<(String, String), String> {
    let Some(creator) = repo::get_admin_by_account_id_conn(conn, creator_account_id)? else {
        return Ok(("管理".to_string(), "员".to_string()));
    };
    // 创建者姓名只作展示，禁止从本地表反推授权省份。
    Ok(admin_person_names(&creator))
}

pub(crate) fn ensure_city_in_province(
    province_name: &str,
    city: &str,
) -> Result<(String, String), axum::response::Response> {
    let province_name = province_name.trim();
    if province_name.is_empty() {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "admin province scope missing",
        ));
    }
    let city = city.trim();
    if city.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, 1001, "city is required"));
    }
    let Some(city_code) = crate::cid::china::city_code_by_name(province_name, city) else {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "city not found in province",
        ));
    };
    if city_code == "000" {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "province placeholder city (000) is not allowed",
        ));
    }
    Ok((province_name.to_string(), city.to_string()))
}
