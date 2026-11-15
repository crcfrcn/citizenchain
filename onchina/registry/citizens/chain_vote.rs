//! 投票资格查询 handler。
//!
//! 本接口只返回 OnChina 本地公民档案的即时状态,方便 CitizenApp 在提交交易前提示用户。
//! 链端投票资格以 runtime `identity` 的链上状态为唯一真源,交易执行时再次校验。
//!
//! 无 token 鉴权：只按规范账户 ID 返回本人档案摘要，不签发链端可消费凭证。

use crate::citizens::admin_entry::CitizenQueries;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::*;
use onchina::chain_runtime::normalize_account_id;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppVoteEligibilityInput {
    pub(crate) account_id: String,
    #[serde(serialize_with = "onchina::response::serialize_decimal", deserialize_with = "onchina::response::deserialize_decimal")]
    pub(crate) proposal_id: u128,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppVoteEligibilityOutput {
    account_id: String,
    #[serde(serialize_with = "onchina::response::serialize_decimal")]
    proposal_id: u128,
    cid_number: String,
    citizen_status: CitizenStatus,
    identity_status: CitizenStatus,
    vote_status: CitizenStatus,
    eligible: bool,
    province_code: String,
    city_code: String,
    town_code: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vote_eligibility_preserves_u128_proposal_ids_as_decimal_strings() {
        for proposal_id in [0, 9_007_199_254_740_993, u64::MAX as u128 + 1, u128::MAX] {
            let output = AppVoteEligibilityOutput {
                account_id: format!("0x{}", "07".repeat(32)),
                proposal_id,
                cid_number: "CN001-CTZN-TEST-2026".into(),
                citizen_status: CitizenStatus::Normal,
                identity_status: CitizenStatus::Normal,
                vote_status: CitizenStatus::Normal,
                eligible: true,
                province_code: "001".into(),
                city_code: "001".into(),
                town_code: "001".into(),
            };
            let json = serde_json::to_value(output).unwrap();
            assert_eq!(json["proposalId"], proposal_id.to_string());
            let input: AppVoteEligibilityInput = serde_json::from_value(serde_json::json!({
                "accountId": json["accountId"],
                "proposalId": json["proposalId"],
            }))
            .unwrap();
            assert_eq!(input.proposal_id, proposal_id);
        }
    }
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

    onchina::audit_runtime::append_audit_log(
        &state,
        "APP_VOTE_ELIGIBILITY",
        "app",
        Some(account_id.clone()),
        serde_json::json!({
            "proposalId": proposal_id.to_string(),
            "cidNumber": record.cid_number,
            "eligible": eligible,
            "actorIp": actor_ip_from_headers(&headers),
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
