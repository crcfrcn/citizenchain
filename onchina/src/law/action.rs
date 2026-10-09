//! 立法链交易签名准备。
//!
//! 本模块只负责把已经完成权限与辖区前置校验的 `ChainCall` 转为统一链签名会话：
//! OnChina 展示请求二维码，CitizenWallet 只签名一次并显示响应二维码，OnChina 回扫后
//! 统一通过 `/api/admin/chain/submit` 验签、dry-run、提交并等待进块。

use super::model::ProposeLawInput;
use super::service::{build_propose_law_call, build_representative_vote_call};
use crate::auth::passkey::PasskeyProof;
use crate::core::institution_call::ChainCall;
use crate::domains::citizens::occupy::{ChainSignSession, SESSION_TTL_SECS};
use crate::{api_error, AppState};
use axum::http::StatusCode;
use axum::response::Response;
use chrono::{Duration, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use uuid::Uuid;

pub(crate) const PURPOSE_LEGISLATION_PROPOSE: &str = "LEGISLATION_PROPOSE";
pub(crate) const PURPOSE_LEGISLATION_REPRESENTATIVE_VOTE: &str = "LEGISLATION_REPRESENTATIVE_VOTE";

/// 一次立法链签名会话的完整描述。
///
/// 把「请求前缀 / 用途 / 发起管理员 / 归属机构 / 待签链调用 / 操作上下文」收成一体,
/// 避免 `prepare_legislation_sign` 变成长参数列表——那样既容易在调用点串位,
/// 也会触发 `clippy::too_many_arguments`(手写函数不走 FRAME ABI 豁免)。
struct LegislationSignRequest<'a> {
    /// request_id 前缀,区分提案与表决两类会话。
    request_prefix: &'a str,
    /// 会话用途常量,落库后决定 submit 阶段的投影分支。
    purpose: &'a str,
    /// 发起管理员的钱包账户 account_id。
    account_id: &'a str,
    /// 发起机构 CID。
    institution_cid_number: &'a str,
    /// 待冷签的链调用(call_data + 动作码)。
    chain: ChainCall,
    /// 落库的业务上下文,供 submit 阶段还原操作语义。
    operation_context: Value,
}

/// 所有立法链交易 prepare 接口统一返回请求编号和请求二维码载荷。
#[derive(Debug, Serialize)]
pub(crate) struct LegislationSignOutput {
    request_id: String,
    sign_request: String,
}

/// 准备法律案提案签名会话；路由机构 CID 仍由 handler 注入的数据库解析器提供。
pub(crate) async fn prepare_propose_law_sign(
    state: &AppState,
    input: &ProposeLawInput,
    proposer_code: [u8; 4],
    account_id: &str,
    institution_cid_number: &str,
    resolve_cid_number: impl Fn(&[u8; 4]) -> Option<String>,
    passkey: &PasskeyProof,
) -> Result<LegislationSignOutput, Response> {
    let chain = build_propose_law_call(input, proposer_code, resolve_cid_number)
        .map_err(|error| api_error(StatusCode::UNPROCESSABLE_ENTITY, 2001, error.code()))?;
    prepare_legislation_sign(
        state,
        LegislationSignRequest {
            request_prefix: "leg-propose",
            purpose: PURPOSE_LEGISLATION_PROPOSE,
            account_id,
            institution_cid_number,
            chain,
            operation_context: json!({
                "tier": input.tier,
                "scope_code": input.scope_code,
                "vote_type": input.vote_type,
            }),
        },
        passkey,
    )
    .await
}

/// 准备代表机构表决签名会话。
pub(crate) async fn prepare_representative_vote_sign(
    state: &AppState,
    proposal_id: u64,
    voter_role_code: &str,
    approve: bool,
    account_id: &str,
    institution_cid_number: &str,
    passkey: &PasskeyProof,
) -> Result<LegislationSignOutput, Response> {
    prepare_legislation_sign(
        state,
        LegislationSignRequest {
            request_prefix: "leg-representative-vote",
            purpose: PURPOSE_LEGISLATION_REPRESENTATIVE_VOTE,
            account_id,
            institution_cid_number,
            chain: build_representative_vote_call(proposal_id, voter_role_code, approve),
            operation_context: json!({
                "proposal_id": proposal_id,
                "voter_role_code": voter_role_code,
                "approve": approve,
            }),
        },
        passkey,
    )
    .await
}

/// 统一读取实时 nonce/runtime/创世哈希，保存短期会话并生成完整审阅载荷。
async fn prepare_legislation_sign(
    state: &AppState,
    request: LegislationSignRequest<'_>,
    passkey: &PasskeyProof,
) -> Result<LegislationSignOutput, Response> {
    let LegislationSignRequest {
        request_prefix,
        purpose,
        account_id,
        institution_cid_number,
        chain,
        operation_context,
    } = request;
    let prepared = crate::core::chain_submit::prepare_signing(&chain.call_data, account_id)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, purpose, "prepare legislation signing failed");
            api_error(
                StatusCode::BAD_GATEWAY,
                5002,
                "链签名载荷准备失败(链不可用)",
            )
        })?;
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::seconds(SESSION_TTL_SECS);
    let request_id = format!("{request_prefix}-{}", Uuid::new_v4());
    let sign_request = crate::core::qr::build_sign_request_bytes(
        request_id.as_str(),
        issued_at.timestamp(),
        expires_at.timestamp(),
        account_id,
        &prepared.payload,
        chain.action,
    )?;
    let session = ChainSignSession {
        request_id: request_id.clone(),
        purpose: purpose.to_string(),
        account_id: account_id.to_string(),
        call_data: chain.call_data,
        nonce: prepared.nonce,
        signing_hash: prepared.signing_hash_hex,
        context: json!({
            "cid_number": institution_cid_number,
            "operation": operation_context,
        }),
        expires_at,
        consumed_at: None,
    };
    state
        .db
        .insert_chain_sign_session(&session, passkey)
        .map_err(|error| {
            tracing::error!(error = %error, purpose, "insert legislation chain sign session failed");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                5001,
                "立法链签名会话保存失败",
            )
        })?;
    Ok(LegislationSignOutput {
        request_id,
        sign_request,
    })
}
