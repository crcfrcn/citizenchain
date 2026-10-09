//! 投票资格查询 handler。
//!
//! 本接口只返回 OnChina 本地公民档案的即时状态,方便 CitizenApp 在提交交易前提示用户。
//! 链端投票资格以 runtime `citizen-identity` 的链上状态为唯一真源,交易执行时再次校验。
//!
//! 无 token 鉴权：只按规范账户 ID 返回本人档案摘要，不签发链端可消费凭证。

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::core::chain_runtime::normalize_account_id;
use crate::*;

#[derive(Deserialize)]
pub(crate) struct AppVoteEligibilityInput {
    pub(crate) account_id: String,
    pub(crate) proposal_id: u64,
}

#[derive(Serialize)]
struct AppVoteEligibilityOutput {
    account_id: String,
    proposal_id: u64,
    cid_number: String,
    citizen_status: CitizenStatus,
    identity_status: CitizenStatus,
    vote_status: CitizenStatus,
    eligible: bool,
    province_code: String,
    city_code: String,
    town_code: String,
}

/// `POST /api/app/vote/eligibility`
pub(crate) async fn app_vote_eligibility(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<AppVoteEligibilityInput>,
) -> impl IntoResponse {
    let Some(account_id) = normalize_account_id(input.account_id.as_str()) else {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "account_id must be lowercase 0x plus 64 hexadecimal characters",
        );
    };
    let proposal_id = input.proposal_id;

    let record = match state.db.find_citizen_by_account_id(&account_id) {
        Ok(Some(record)) => record,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 1004, "citizen archive not found"),
        Err(err) => {
            tracing::error!(error = %err, "query vote eligibility failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "vote eligibility query failed",
            );
        }
    };
    let identity_status = record.computed_identity_status();
    let vote_status = record.computed_vote_status();
    let eligible = vote_status == CitizenStatus::Normal;
    if !eligible {
        return api_error(StatusCode::FORBIDDEN, 1003, "citizen not vote eligible");
    }

    crate::core::runtime_ops::append_audit_log(
        &state,
        "APP_VOTE_ELIGIBILITY",
        "app",
        Some(account_id.clone()),
        serde_json::json!({
            "proposal_id": proposal_id,
            "cid_number": record.cid_number,
            "eligible": eligible,
            "actor_ip": actor_ip_from_headers(&headers),
        }),
    );

    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: AppVoteEligibilityOutput {
            account_id,
            proposal_id,
            cid_number: record.cid_number,
            citizen_status: record.citizen_status,
            identity_status,
            vote_status,
            eligible,
            province_code: record.province_code,
            city_code: record.city_code,
            town_code: record.town_code,
        },
    })
    .into_response()
}
