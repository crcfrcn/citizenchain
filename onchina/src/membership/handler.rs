//! 平台会员价格查询与调价提案 handler。

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::login::{require_admin_any, AdminAuthContext};
use crate::auth::repo;
use crate::core::chain_runtime::{self, PlatformMembershipSnapshot};
use crate::crypto::pubkey::same_account_id;
use crate::*;

use super::chain_call::{
    build_propose_platform_price_call, PlatformMembershipLevel, PROPOSE_PLATFORM_PRICE_ACTION,
};

#[derive(Debug, Serialize)]
pub(crate) struct PlatformPriceOutput {
    platform_cid_number: String,
    freedom_price_fen: String,
    democracy_price_fen: String,
    spark_price_fen: String,
    finalized_block_hash: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProposePlatformPriceInput {
    proposer_role_code: String,
    membership_level: String,
    /// JSON 使用十进制字符串，避免浏览器数字精度改变链上金额。
    new_price_fen: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProposePlatformPriceOutput {
    request_id: String,
    sign_request: String,
}

async fn require_platform_admin(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(AdminAuthContext, PlatformMembershipSnapshot), axum::response::Response> {
    let ctx = require_admin_any(state, headers)?;
    let snapshot = recheck_platform_admin(state, &ctx).await?;
    Ok((ctx, snapshot))
}

/// 提交前再次检查准确机构 CID 与当前链上管理员集合，防止 prepare 后撤权仍可提交。
pub(crate) async fn recheck_platform_admin(
    state: &AppState,
    ctx: &AdminAuthContext,
) -> Result<PlatformMembershipSnapshot, axum::response::Response> {
    let snapshot = chain_runtime::fetch_platform_membership_snapshot()
        .await
        .map_err(|err| {
            tracing::warn!(error = %err, "read finalized platform membership state failed");
            api_error(StatusCode::BAD_GATEWAY, 5002, "chain state unavailable")
        })?;
    if snapshot.platform_cid_number.as_deref() != Some(ctx.institution_cid_number.as_str()) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            2003,
            "current institution is not the platform institution",
        ));
    }
    let binding = repo::active_node_binding(&state.db)
        .map_err(|_| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                5001,
                "binding query failed",
            )
        })?
        .ok_or_else(|| api_error(StatusCode::UNAUTHORIZED, 1002, "node is not bound"))?;
    if binding.institution_cid_number != ctx.institution_cid_number {
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            1002,
            "session institution binding mismatch",
        ));
    }
    let identity = chain_runtime::identity_from_binding_parts(
        &binding.institution_code,
        Some(binding.institution_cid_number.as_str()),
        binding.frg_province_code.as_deref(),
    )
    .map_err(|_| api_error(StatusCode::FORBIDDEN, 2003, "invalid institution binding"))?;
    let admins = chain_runtime::fetch_active_admins_onchain(&identity)
        .await
        .map_err(|_| api_error(StatusCode::BAD_GATEWAY, 5002, "chain admins unavailable"))?
        .ok_or_else(|| api_error(StatusCode::FORBIDDEN, 2002, "institution admins missing"))?;
    if !admins
        .iter()
        .any(|admin| same_account_id(&admin.account_id, ctx.account_id.as_str()))
    {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            2002,
            "admin is no longer active for this institution",
        ));
    }
    Ok(snapshot)
}

pub(crate) async fn platform_prices(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let (_ctx, snapshot) = match require_platform_admin(&state, &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Some(platform_cid_number) = snapshot.platform_cid_number else {
        return api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            5002,
            "platform CID is not bound",
        );
    };
    let (Some(freedom), Some(democracy), Some(spark)) = (
        snapshot.freedom_price_fen,
        snapshot.democracy_price_fen,
        snapshot.spark_price_fen,
    ) else {
        return api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            5002,
            "platform price is incomplete",
        );
    };
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: PlatformPriceOutput {
            platform_cid_number,
            freedom_price_fen: freedom.to_string(),
            democracy_price_fen: democracy.to_string(),
            spark_price_fen: spark.to_string(),
            finalized_block_hash: snapshot.block_hash,
        },
    })
    .into_response()
}

pub(crate) async fn propose_platform_price(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ProposePlatformPriceInput>,
) -> impl IntoResponse {
    let (ctx, _snapshot) = match require_platform_admin(&state, &headers).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    // 链上写(PasskeyColdSign 档):平台调价要发 extrinsic,会话 + passkey + 冷签缺一不可。
    let passkey = match crate::auth::passkey::require_passkey_assertion(
        &state,
        &headers,
        ctx.account_id.as_str(),
    ) {
        Ok(proof) => proof,
        Err(response) => return response,
    };
    let Some(membership_level) = PlatformMembershipLevel::parse(&input.membership_level) else {
        return api_error(StatusCode::BAD_REQUEST, 1001, "invalid membership_level");
    };
    let new_price_fen = match input.new_price_fen.trim().parse::<u128>() {
        Ok(value) if value > 0 => value,
        _ => {
            return api_error(
                StatusCode::BAD_REQUEST,
                1001,
                "new_price_fen must be positive",
            )
        }
    };
    let call = match build_propose_platform_price_call(
        ctx.institution_cid_number.as_str(),
        input.proposer_role_code.as_str(),
        membership_level,
        new_price_fen,
    ) {
        Ok(value) => value,
        Err(message) => return api_error(StatusCode::BAD_REQUEST, 1001, message.as_str()),
    };
    let prepared =
        match crate::core::chain_submit::prepare_signing(&call, ctx.account_id.as_str()).await {
            Ok(value) => value,
            Err(err) => {
                tracing::error!(error = %err, "prepare platform price signing failed");
                return api_error(
                    StatusCode::BAD_GATEWAY,
                    5002,
                    "链签名载荷准备失败(链不可用)",
                );
            }
        };
    let now = Utc::now();
    let expires_at = now + Duration::seconds(crate::domains::citizens::occupy::SESSION_TTL_SECS);
    let request_id = format!("platform-price-{}", Uuid::new_v4());
    let sign_request = match crate::core::qr::build_sign_request_bytes(
        request_id.as_str(),
        now.timestamp(),
        expires_at.timestamp(),
        ctx.account_id.as_str(),
        &prepared.payload,
        PROPOSE_PLATFORM_PRICE_ACTION,
    ) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let session = crate::domains::citizens::occupy::ChainSignSession {
        request_id: request_id.clone(),
        purpose: super::PURPOSE_PLATFORM_PRICE_PROPOSAL.to_string(),
        account_id: ctx.account_id.clone(),
        call_data: call,
        nonce: prepared.nonce,
        signing_hash: prepared.signing_hash_hex,
        context: serde_json::json!({
            "cid_number": ctx.institution_cid_number,
            "membership_level": input.membership_level,
            "new_price_fen": new_price_fen.to_string(),
        }),
        expires_at,
        consumed_at: None,
    };
    if let Err(err) = state.db.insert_chain_sign_session(&session, &passkey) {
        tracing::error!(error = %err, "insert platform price chain sign session failed");
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            5001,
            "平台调价签名会话保存失败",
        );
    }
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: ProposePlatformPriceOutput {
            request_id,
            sign_request,
        },
    })
    .into_response()
}
