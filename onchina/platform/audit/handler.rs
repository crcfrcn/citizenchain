//! 审计日志 list handler(注册局管理员只读)
//!
//! 审计日志是独立后台能力,不属于权限范围规则本身,统一位于 `onchina/platform/audit`；
//! 通用辖区校验继续由 platform/authorization 提供。

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogEntry {
    pub seq: u64,
    pub action: String,
    pub actor_account_id: Option<String>,
    pub target_cid: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub actor_ip: Option<String>,
    pub result: String,
    /// 结构化事实字段(JSON 对象,键小写蛇形,值为系统原值);人话翻译归前端渲染器。
    pub detail: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogsQuery {
    pub action: Option<String>,
    pub actor_account_id: Option<String>,
    pub target_cid: Option<String>,
    pub keyword: Option<String>,
    pub limit: Option<usize>,
}

/// 联邦注册局管理员 / 市注册局管理员 均可访问审计日志(只读)。
pub async fn admin_list_audit_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AuditLogsQuery>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if let Err(resp) = crate::authorization::rules::ensure_registry_admin(&ctx) {
        return resp;
    }

    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let action = query.action.unwrap_or_default().trim().to_uppercase();
    let actor = query
        .actor_account_id
        .unwrap_or_default()
        .trim()
        .to_string();
    if !actor.is_empty() && crate::crypto::pubkey::normalize_account_id(&actor).is_none() {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "actor_account_id must be lowercase 0x plus 64 hexadecimal characters",
        );
    }
    let target_cid = query.target_cid.unwrap_or_default().trim().to_string();
    let keyword = query.keyword.unwrap_or_default().trim().to_lowercase();
    let province_code = ctx
        .scope_province_name
        .as_deref()
        .and_then(crate::codes::china::province_code_by_name);
    let city_code = match (
        ctx.scope_province_name.as_deref(),
        ctx.scope_city_name.as_deref(),
    ) {
        (Some(province), Some(city)) => crate::codes::china::city_code_by_name(province, city),
        _ => None,
    };
    let result = state.db.with_client(move |conn| {
        let limit_i64 = i64::try_from(limit).map_err(|_| "limit too large".to_string())?;
        let rows = conn
            .query(
                "SELECT row_id, action_kind, actor_account, target_cid, detail_json, created_at
                 FROM audit
                 WHERE ($1::text IS NULL OR province_code = $1)
                   AND ($2::text IS NULL OR city_code = $2)
                   AND ($3::text = '' OR action_kind = $3)
                   AND ($4::text = '' OR actor_account = $4)
                   AND ($5::text = '' OR target_cid = $5)
                   AND (
                        $6::text = ''
                        OR lower(detail_json::text) LIKE '%' || $6 || '%'
                        OR lower(action_kind) LIKE '%' || $6 || '%'
                        OR lower(COALESCE(actor_account, '')) LIKE '%' || $6 || '%'
                        OR lower(COALESCE(target_cid, '')) LIKE '%' || $6 || '%'
                   )
                 ORDER BY created_at DESC, row_id DESC
                 LIMIT $7",
                &[
                    &province_code,
                    &city_code,
                    &action,
                    &actor,
                    &target_cid,
                    &keyword,
                    &limit_i64,
                ],
            )
            .map_err(|e| format!("query audit logs failed: {e}"))?;
        let mut output = Vec::with_capacity(rows.len());
        for row in rows {
            let seq: i64 = row.get(0);
            output.push(AuditLogEntry {
                seq: u64::try_from(seq).unwrap_or(0),
                action: row.get(1),
                actor_account_id: row.get(2),
                target_cid: row.get(3),
                request_id: None,
                actor_ip: None,
                result: "SUCCESS".to_string(),
                detail: row.get(4),
                created_at: row.get(5),
            });
        }
        Ok(output)
    });
    let rows = match result {
        Ok(v) => v,
        Err(err) => {
            let message = format!("query audit logs failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };

    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: rows,
    })
    .into_response()
}
