//! 机构信息查询(chain pull)。
//!
//! 公开只读接口直接查询 `subjects/accounts` 结构化表。

use crate::core::response::ApiResponse;
use crate::institution::subjects::service::institution_account_kind_label;
use crate::*;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub(crate) struct AppInstitutionDetail {
    pub(crate) cid_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cid_full_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cid_short_name: Option<String>,
    pub(crate) category: crate::cid::InstitutionCategory,
    pub(crate) p1: String,
    pub(crate) province_name: String,
    pub(crate) city_name: String,
    pub(crate) province_code: String,
    pub(crate) city_code: String,
    pub(crate) institution_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) private_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) partnership_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) has_legal_personality: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) parent_cid_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) legal_representative:
        Option<crate::institution::subjects::model::LegalRepresentative>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AppInstitutionSearchQuery {
    pub q: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Serialize, Clone)]
pub(crate) struct AppInstitutionSearchRow {
    pub(crate) cid_number: String,
    pub(crate) cid_full_name: Option<String>,
    pub(crate) cid_short_name: Option<String>,
    pub(crate) category: crate::cid::InstitutionCategory,
    pub(crate) province_name: String,
    pub(crate) city_name: String,
}

#[derive(Serialize)]
pub(crate) struct AppAccountEntry {
    pub(crate) account_name: String,
    pub(crate) account_id: Option<String>,
    pub(crate) account_kind: &'static str,
    pub(crate) can_close: bool,
}

#[derive(Serialize)]
pub(crate) struct AppInstitutionAccounts {
    pub(crate) cid_number: String,
    pub(crate) cid_full_name: String,
    pub(crate) cid_short_name: String,
    pub(crate) accounts: Vec<AppAccountEntry>,
}

fn parse_category(value: &str) -> crate::cid::InstitutionCategory {
    match value {
        "GOV_INSTITUTION" => crate::cid::InstitutionCategory::GovInstitution,
        _ => crate::cid::InstitutionCategory::PrivateInstitution,
    }
}

pub(crate) async fn app_search_institutions(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<AppInstitutionSearchQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(20).clamp(1, 50) as i64;
    let q = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 128)
        .map(str::to_string);
    let rows = match state.db.with_client(move |conn| {
        let rows = conn
            .query(
                // 行政区名字按 code 现场从 china.sqlite 派生,DTO 仍带省/市名字;
                // 库里只存 province_code/city_code,排序也按 code(同省/市稳定有序)。
                "SELECT cid_number, cid_full_name, cid_short_name, category, province_code, city_code
                 FROM subjects
                 WHERE kind IN ('PUBLIC', 'PRIVATE')
                   AND status = 'ACTIVE'
                   AND (
                        $1::text IS NULL
                        OR cid_number ILIKE '%' || $1 || '%'
                        OR COALESCE(cid_full_name, '') ILIKE '%' || $1 || '%'
                        OR COALESCE(cid_short_name, '') ILIKE '%' || $1 || '%'
                   )
                 ORDER BY province_code ASC, city_code ASC, COALESCE(cid_short_name, '') ASC,
                          COALESCE(cid_full_name, '') ASC, cid_number ASC
                 LIMIT $2",
                &[&q, &limit],
            )
            .map_err(|e| format!("search institutions failed: {e}"))?;
        Ok(rows
            .iter()
            .map(|row| {
                let province_code: String = row.get(4);
                let city_code: Option<String> = row.get(5);
                let (province_name, city_name, _town_name) =
                    crate::cid::china::area_display_names(
                        province_code.as_str(),
                        city_code.as_deref(),
                        None,
                    );
                AppInstitutionSearchRow {
                    cid_number: row.get(0),
                    cid_full_name: row.get(1),
                    cid_short_name: row.get(2),
                    category: parse_category(row.get::<_, String>(3).as_str()),
                    province_name,
                    city_name,
                }
            })
            .collect::<Vec<_>>())
    }) {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(error = %err, "app search institutions failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "institution query failed",
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

pub(crate) async fn app_get_institution(
    State(state): State<AppState>,
    Path(cid_number): Path<String>,
) -> impl IntoResponse {
    let Some((inst, _)) = (match state.db.get_institution_with_accounts(&cid_number) {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(error = %err, "query institution failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "institution query failed",
            );
        }
    }) else {
        return api_error(StatusCode::NOT_FOUND, 1004, "institution not found");
    };
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: AppInstitutionDetail {
            cid_number: inst.cid_number,
            cid_full_name: inst.cid_full_name,
            cid_short_name: inst.cid_short_name,
            category: inst.category,
            p1: inst.p1,
            province_name: inst.province_name,
            city_name: inst.city_name,
            province_code: inst.province_code,
            city_code: inst.city_code,
            institution_code: inst.institution_code,
            private_type: inst.private_type,
            partnership_kind: inst.partnership_kind,
            has_legal_personality: inst.has_legal_personality,
            parent_cid_number: inst.parent_cid_number,
            legal_representative: inst.legal_representative,
        },
    })
    .into_response()
}

pub(crate) async fn app_list_accounts(
    State(state): State<AppState>,
    Path(cid_number): Path<String>,
) -> impl IntoResponse {
    let cid_number = cid_number.trim().to_string();
    if cid_number.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, 1001, "cid_number is required");
    }
    // 机构展示名仍取本地 subjects 投影;账户明细从链上真源(InstitutionAccounts)读取。
    let Some((inst, _)) = (match state.db.get_institution_with_accounts(&cid_number) {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(error = %err, "query institution accounts failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "accounts query failed",
            );
        }
    }) else {
        return api_error(StatusCode::NOT_FOUND, 1004, "institution not found");
    };
    let code = match crate::institution::admins::code_bytes(&inst.institution_code) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let chain_accounts =
        match crate::core::chain_runtime::institution_accounts_lookup(&code, &cid_number).await {
            Ok(v) => v,
            Err(err) => {
                tracing::error!(error = %err, "read chain institution accounts failed");
                return api_error(StatusCode::BAD_GATEWAY, 1004, "accounts query failed");
            }
        };
    let accounts = chain_accounts
        .iter()
        .map(|account| {
            let account_name = String::from_utf8_lossy(&account.account_name).into_owned();
            let kind =
                institution_account_kind_label(&cid_number, &account_name).unwrap_or("named");
            AppAccountEntry {
                account_id: Some(format!("0x{}", hex::encode(account.account_id))),
                account_kind: kind,
                can_close: kind == "named",
                account_name,
            }
        })
        .collect::<Vec<_>>();
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: AppInstitutionAccounts {
            cid_number,
            cid_full_name: inst.cid_full_name.unwrap_or_default(),
            cid_short_name: inst.cid_short_name.unwrap_or_default(),
            accounts,
        },
    })
    .into_response()
}
