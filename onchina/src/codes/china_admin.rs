//! 行政区划管理员只读接口。

// 参数校验辅助函数直接返回统一 Axum Response，错误仅发生在请求拒绝路径。
#![allow(clippy::result_large_err)]

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Serialize;

use crate::cid::china::provinces;
use crate::cid::model::{AdminCidCitiesQuery, AdminCidTownsQuery, CidCityItem, CidTownItem};
use crate::*;

fn ok<T: Serialize>(data: T) -> axum::response::Response {
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data,
    })
    .into_response()
}

fn trimmed(value: &str, field: &str) -> Result<String, axum::response::Response> {
    let value = value.trim();
    if value.is_empty() {
        let message = format!("{field} is required");
        return Err(api_error(StatusCode::BAD_REQUEST, 1001, message.as_str()));
    }
    Ok(value.to_string())
}

pub(crate) async fn admin_china_cities(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminCidCitiesQuery>,
) -> impl IntoResponse {
    let _admin_ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let province_name = match trimmed(&query.province_name, "province_name") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let Some(p) = provinces()
        .iter()
        .find(|p| p.province_name == province_name)
    else {
        return api_error(StatusCode::NOT_FOUND, 1004, "province not found");
    };
    let mut rows: Vec<CidCityItem> = p
        .cities
        .iter()
        .map(|c| CidCityItem {
            city_name: c.city_name.to_string(),
            city_code: c.city_code.to_string(),
        })
        .collect();
    rows.sort_by(|a, b| a.city_code.cmp(&b.city_code));
    ok(rows)
}

pub(crate) async fn admin_china_towns(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminCidTownsQuery>,
) -> impl IntoResponse {
    let _admin_ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let province_name = match trimmed(&query.province_name, "province_name") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let city_code = match trimmed(&query.city_code, "city_code") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let Some(p) = provinces()
        .iter()
        .find(|p| p.province_name == province_name)
    else {
        return api_error(StatusCode::NOT_FOUND, 1004, "province not found");
    };
    let Some(c) = p
        .cities
        .iter()
        .find(|c| c.city_code.eq_ignore_ascii_case(city_code.as_str()))
    else {
        return api_error(StatusCode::NOT_FOUND, 1004, "city not found");
    };
    let mut rows: Vec<CidTownItem> = c
        .towns
        .iter()
        .map(|t| CidTownItem {
            town_name: t.town_name.to_string(),
            town_code: t.town_code.to_string(),
        })
        .collect();
    rows.sort_by(|a, b| a.town_code.cmp(&b.town_code));
    ok(rows)
}
