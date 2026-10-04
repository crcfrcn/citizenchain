//! Indexer API 路由：按规范账户 ID 查询交易记录。

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{api_error, ApiResponse, AppState};

use super::db;

#[derive(Deserialize)]
pub(crate) struct TxQuery {
    pub limit: Option<i64>,
    pub before_id: Option<i64>,
    pub tx_type: Option<String>,
}

#[derive(Serialize)]
struct TxRecordOutput {
    id: i64,
    block_number: i64,
    tx_type: String,
    direction: &'static str,
    sender_account_id: Option<String>,
    recipient_account_id: Option<String>,
    amount_yuan: f64,
    fee_yuan: Option<f64>,
    block_timestamp: Option<String>,
}

#[derive(Serialize)]
struct TxListOutput {
    records: Vec<TxRecordOutput>,
    has_more: bool,
}

/// GET /api/app/accounts/:account_id/transactions
pub(crate) async fn account_transactions(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
    Query(query): Query<TxQuery>,
) -> impl IntoResponse {
    let Some(account_id) = crate::crypto::pubkey::normalize_account_id(&account_id) else {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "account_id must be lowercase 0x plus 64 hexadecimal characters",
        );
    };

    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    // 多查一条以判断 has_more
    let fetch_limit = limit + 1;

    let result = state.db.with_client(|conn| {
        db::query_tx_records(
            conn,
            &account_id,
            query.before_id,
            query.tx_type.as_deref(),
            fetch_limit,
        )
    });

    let rows = match result {
        Ok(r) => r,
        Err(err) => {
            tracing::warn!(error = %err, "query tx_records failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 1500, "query failed");
        }
    };

    let has_more = rows.len() as i64 > limit;
    let rows_to_return = if has_more {
        &rows[..limit as usize]
    } else {
        &rows
    };

    let records: Vec<TxRecordOutput> = rows_to_return
        .iter()
        .map(|r| {
            let direction = determine_direction(
                &account_id,
                r.sender_account_id.as_deref(),
                r.recipient_account_id.as_deref(),
            );
            TxRecordOutput {
                id: r.id,
                block_number: r.block_number,
                tx_type: r.tx_type.clone(),
                direction,
                sender_account_id: r.sender_account_id.clone(),
                recipient_account_id: r.recipient_account_id.clone(),
                amount_yuan: r.amount_fen as f64 / 100.0,
                fee_yuan: r.fee_fen.map(|f| f as f64 / 100.0),
                block_timestamp: r.block_timestamp.map(|ts| ts.to_rfc3339()),
            }
        })
        .collect();

    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: TxListOutput { records, has_more },
    })
    .into_response()
}

/// 判断交易方向：out = 我是付款方，in = 我是收款方。
fn determine_direction(my_account_id: &str, from: Option<&str>, to: Option<&str>) -> &'static str {
    if from.is_some_and(|f| f == my_account_id) {
        "out"
    } else if to.is_some_and(|t| t == my_account_id) {
        "in"
    } else {
        // 无 from/to 的系统事件（如 gov_issuance 汇总事件）
        "info"
    }
}
