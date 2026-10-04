//! 公民人数查询 handler。
//!
//! 本接口只返回 OnChina 本地公民档案统计,用于 CitizenApp 展示或诊断。
//! 链端联合投票人口快照由 runtime 从 `citizen-identity` 链上状态按 scope 读取。
//!
//! 无 token 鉴权:只返回聚合人数,不包含个人档案字段。

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::core::chain_runtime::normalize_account_id;
use crate::*;

#[derive(Deserialize)]
pub(crate) struct AppVotersCountQuery {
    pub(crate) account_id: String,
}

#[derive(Serialize)]
struct AppVotersCountOutput {
    eligible_total: u64,
    account_id: String,
}

/// `GET /api/app/voters/count?account_id=0x...`
pub(crate) async fn app_voters_count(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AppVotersCountQuery>,
) -> impl IntoResponse {
    let Some(account_id) = normalize_account_id(query.account_id.as_str()) else {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "account_id must be lowercase 0x plus 64 hexadecimal characters",
        );
    };

    let eligible_total = match state.db.with_client(|conn| {
        let row = conn
            .query_one(
                "SELECT COUNT(*)::BIGINT
                 FROM citizens
                 WHERE citizen_status = 'NORMAL'
                   AND voting_eligible = true",
                &[],
            )
            .map_err(|e| format!("query eligible voters failed: {e}"))?;
        let total: i64 = row.get(0);
        Ok(u64::try_from(total).unwrap_or(0))
    }) {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(error = %err, "query voters count failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "voters query failed",
            );
        }
    };

    crate::core::runtime_ops::append_audit_log(
        &state,
        "APP_VOTERS_COUNT",
        "app",
        Some(account_id.clone()),
        serde_json::json!({
            "eligible_total": eligible_total,
            "actor_ip": actor_ip_from_headers(&headers),
        }),
    );

    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: AppVotersCountOutput {
            eligible_total,
            account_id,
        },
    })
    .into_response()
}
