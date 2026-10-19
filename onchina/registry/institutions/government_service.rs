//! 注册局辖区内公权机构列表查询；公共投影由平台提供。
use crate::*;
use crate::indexer::db::government::{chain_projection_ready, current_chain_projection_version};
use axum::response::{IntoResponse, Response};
pub(crate) fn list_official(
    state: &AppState, query: &super::handler::ListOfficialInstitutionQuery,
    province_code: &str, city_code: Option<&str>, town_code: Option<&str>, offset: usize, page_size: usize,
) -> Response {
    let keyword = query.q.as_deref().map(str::trim).unwrap_or("");
    let institution_code_filter = query
        .institution_code
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let mut page = match state.db.list_official_institutions_scope(
        province_code,
        city_code,
        town_code,
        keyword,
        institution_code_filter,
        offset,
        page_size,
    ) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "official institution list failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "official institution query failed",
            );
        }
    };
    page.manifest_version = current_chain_projection_version(&state.db);
    page.catalog_status = Some("OK".to_string());
    if page.items.is_empty() {
        match chain_projection_ready(&state.db) {
            Ok(false) => {
                return api_error(
                    StatusCode::CONFLICT,
                    1005,
                    "chain public institution projection is not initialized",
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, "official chain projection check failed");
            }
            Ok(true) => {}
        }
    }
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: page,
    })
    .into_response()
}
