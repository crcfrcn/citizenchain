//! 管理员安全动作:会话 + QR_V1 冷钱包扫码签名(PasskeyColdSign 档)。
//!
//! 管理员治理动作、业务安全授权和短期挑战全部使用结构化表;
//! PasskeyColdSign 档 commit 校验冷钱包签名且 signer 须 ∈ 本机构链上 Active 集合。

// 管理员动作统一返回完整 Axum Response 以保留状态码和错误体，错误路径不参与高频成功态分配。
#![allow(clippy::result_large_err)]

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::authentication::action_sign::{
    hash_json, payload_hash_for_text, signed_payload_text, verify_account_signature,
    AdminSignedPayload, ADMIN_ACTION_TTL_SECONDS,
};
use crate::authentication::login::AdminAuthContext;
use crate::authentication::operation_auth::{
    ensure_action_role_allowed, parse_action_type, AdminActionType, AdminOperationAuth,
};
use crate::authentication::repo;
use crate::authentication::security_model::{AdminActionChallenge, AdminSecurityGrant};
use crate::crypto::pubkey::same_account_id;
use crate::qr::build_sign_request;
use crate::*;

const ADMIN_SECURITY_GRANT_TTL_SECONDS: i64 = 120;
pub(crate) const ADMIN_SECURITY_GRANT_HEADER: &str = "x-cid-security-grant";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrepareAdminActionInput {
    action_type: AdminActionType,
    payload: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommitAdminActionInput {
    action_id: String,
    /// 冷钱包扫码签名(PasskeyColdSign 档必填;Session 动作不走 commit)。
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    signature: Option<String>,
    #[serde(default)]
    payload_hash: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminSecurityGrantOutput {
    grant_id: String,
    action_type: String,
    actor_cid_number: String,
    auth_type: AdminOperationAuth,
    target: String,
    expires_at: i64,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum CommitAdminActionOutput {
    Applied(serde_json::Value),
    Grant(AdminSecurityGrantOutput),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrepareAdminActionOutput {
    action_id: String,
    action_type: String,
    actor_cid_number: String,
    sign_request: Option<String>,
    payload_hash: String,
    auth_type: AdminOperationAuth,
    expires_at: i64,
}

pub struct ActionPreview {
    pub before_hash: String,
    pub after_hash: String,
    pub target: String,
    pub auth_type: AdminOperationAuth,
}

/// 从节点唯一 active 绑定读取当前机构 CID，并与登录上下文机构严格对齐。
pub(crate) fn actor_cid_number_for_context(
    db: &Db,
    ctx: &AdminAuthContext,
) -> Result<String, axum::response::Response> {
    let binding = repo::active_node_binding(db).map_err(|err| {
        tracing::error!(error = %err, "query active institution binding failed");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            5001,
            "node binding query failed",
        )
    })?;
    let binding =
        binding.ok_or_else(|| api_error(StatusCode::FORBIDDEN, 2002, "node binding missing"))?;
    if binding.institution_code != ctx.institution_code {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            2002,
            "node binding institution mismatch",
        ));
    }
    let cid_number = binding.institution_cid_number;
    if cid_number.is_empty()
        || cid_number.len() > primitives::core_const::CID_NUMBER_MAX_BYTES as usize
    {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            2002,
            "node binding actor_cid_number missing",
        ));
    }
    Ok(cid_number)
}

/// 校验账号 ∈ 本机构链上 Active 管理员集合(冷签 step-up 与替换目标校验共用)。
async fn ensure_account_id_on_chain_admin(
    db: &Db,
    actor_cid_number: &str,
    account_id: &str,
    message: &'static str,
) -> Result<(), axum::response::Response> {
    use crate::chain_runtime;
    let binding = repo::active_node_binding(db).map_err(|err| {
        tracing::error!(error = %err, "query node binding failed");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            5001,
            "node binding query failed",
        )
    })?;
    let Some(binding) = binding else {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            2002,
            "node binding missing",
        ));
    };
    if binding.institution_cid_number != actor_cid_number {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            2002,
            "actor_cid_number does not match node binding",
        ));
    }
    let identity = chain_runtime::identity_from_binding_parts(
        &binding.institution_code,
        Some(actor_cid_number),
    )
    .map_err(|err| {
        tracing::error!(error = %err, "node binding is invalid");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            5001,
            "node binding invalid",
        )
    })?;
    let onchain = chain_runtime::fetch_active_admins_onchain(&identity)
        .await
        .map_err(|err| {
            tracing::warn!(error = %err, "chain unreachable during action commit");
            api_error(StatusCode::BAD_GATEWAY, 5002, "chain unreachable")
        })?
        .ok_or_else(|| api_error(StatusCode::FORBIDDEN, 2002, "not an on-chain admin"))?;
    let normalized = chain_runtime::normalize_account_id(account_id)
        .ok_or_else(|| api_error(StatusCode::FORBIDDEN, 2002, message))?;
    if !onchain
        .iter()
        .any(|admin| same_account_id(&admin.account_id, normalized.as_str()))
    {
        return Err(api_error(StatusCode::FORBIDDEN, 2002, message));
    }
    Ok(())
}

/// 本地敏感写也复核当前链上成员，不能利用后台会话清退间隔继续写入。
pub async fn require_current_admin(
    state: &AppState,
    ctx: &AdminAuthContext,
) -> Result<(), axum::response::Response> {
    ensure_signer_on_chain_admin(&state.db, &ctx.institution_cid_number, &ctx.account_id).await
}

/// 校验扫码签名 signer ∈ 本机构链上 Active 管理员集合(冷签 step-up,与登录同源)。
async fn ensure_signer_on_chain_admin(
    db: &Db,
    actor_cid_number: &str,
    account_id: &str,
) -> Result<(), axum::response::Response> {
    ensure_account_id_on_chain_admin(db, actor_cid_number, account_id, "not an on-chain admin")
        .await
}

pub(crate) async fn prepare_admin_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PrepareAdminActionInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if let Some(owner) = state.modules.owner_of_action(input.action_type.as_str()) {
        if !state.modules.local_owns(&owner) {
            return crate::modules::forward_json(
                &state,
                &owner,
                "/api/admin/actions/prepare",
                &headers,
                &input,
            )
            .await;
        }
        if let Some(module) = &state.modules.local {
            if let Err(response) = module.authorize_action(&ctx, &input.action_type) {
                return response;
            }
        }
    } else if input.action_type.requires_module() {
        return api_error(StatusCode::SERVICE_UNAVAILABLE, 1503, "module unavailable");
    }
    if let Err(resp) = ensure_action_role_allowed(&ctx, &input.action_type) {
        return resp;
    }
    if !input.action_type.can_prepare() {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "action does not support prepare",
        );
    }
    let actor_cid_number = match actor_cid_number_for_context(&state.db, &ctx) {
        Ok(value) => value,
        Err(resp) => return resp,
    };
    let preview = match state.db.with_client({
        let ctx = ctx.clone();
        let action_type = input.action_type.clone();
        let payload = input.payload.clone();
        let module = state.modules.local.clone();
        move |conn| match &module {
            Some(module) => module.preview_admin(conn, &ctx, &action_type, &payload),
            None => preview_action_conn(conn, &ctx, &action_type, &payload),
        }
    }) {
        Ok(v) => v,
        Err(err) => return admin_action_error(err),
    };
    let now = Utc::now();
    let expires_at = now + Duration::seconds(ADMIN_ACTION_TTL_SECONDS);
    let action_id = format!("cid-admin-action-{}", Uuid::new_v4());
    let province = ctx.scope_province_name.clone().unwrap_or_default();
    let request_hash = hash_json(&input.payload);
    let (payload_text, payload_hash, sign_request) =
        if preview.auth_type == AdminOperationAuth::PasskeyColdSign {
            let payload_text = signed_payload_text(AdminSignedPayload {
                domain: "onchina_admin_governance",
                qr_proto: crate::qr::QR_V1,
                action_id: action_id.as_str(),
                action_type: input.action_type.as_str(),
                account_id: ctx.account_id.as_str(),
                actor_cid_number: actor_cid_number.as_str(),
                actor_province_name: province.as_str(),
                target: preview.target.as_str(),
                request_hash: request_hash.as_str(),
                before_hash: preview.before_hash.as_str(),
                after_hash: preview.after_hash.as_str(),
                expires_at: expires_at.timestamp(),
            });
            let payload_hash = payload_hash_for_text(payload_text.as_str());
            let sign_request = match build_sign_request(
                action_id.as_str(),
                now.timestamp(),
                expires_at.timestamp(),
                ctx.account_id.as_str(),
                payload_text.as_str(),
                crate::qr::action_onchina_admin(),
            ) {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            (payload_text, payload_hash, Some(sign_request))
        } else {
            (String::new(), request_hash.clone(), None)
        };
    let challenge = AdminActionChallenge {
        action_id: action_id.clone(),
        action_type: input.action_type.as_str().to_string(),
        actor_account_id: ctx.account_id.clone(),
        actor_institution_code: ctx.institution_code.clone(),
        actor_cid_number: actor_cid_number.clone(),
        actor_province_name: province,
        actor_city_name: ctx.scope_city_name.clone(),
        auth_type: preview.auth_type,
        target: preview.target,
        payload_text,
        payload_hash: payload_hash.clone(),
        before_hash: preview.before_hash,
        after_hash: preview.after_hash,
        request_payload: input.payload,
        issued_at: now,
        expires_at,
        consumed: false,
    };
    if let Err(err) = repo::insert_action_challenge(&state.db, &challenge) {
        let message = format!("insert admin action failed: {err}");
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
    }
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: PrepareAdminActionOutput {
            action_id,
            action_type: input.action_type.as_str().to_string(),
            actor_cid_number,
            sign_request,
            payload_hash,
            auth_type: preview.auth_type,
            expires_at: expires_at.timestamp(),
        },
    })
    .into_response()
}

pub(crate) async fn commit_admin_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CommitAdminActionInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let dispatch_challenge = match state.db.with_client(|conn| {
        repo::get_action_challenge_conn(conn, &input.action_id, &ctx.account_id)
    }) {
        Ok(Some(challenge)) => challenge,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 1004, "admin action not found"),
        Err(err) => return admin_action_error(err),
    };
    if let Some(owner) = state
        .modules
        .owner_of_action(&dispatch_challenge.action_type)
    {
        if !state.modules.local_owns(&owner) {
            return crate::modules::forward_json(
                &state,
                &owner,
                "/api/admin/actions/commit",
                &headers,
                &input,
            )
            .await;
        }
    } else if parse_action_type(&dispatch_challenge.action_type)
        .is_ok_and(|action| action.requires_module())
    {
        return api_error(StatusCode::SERVICE_UNAVAILABLE, 1503, "module unavailable");
    }
    let now = Utc::now();
    let challenge = match state.db.with_client({
        let action_id = input.action_id.clone();
        let actor_account_id = ctx.account_id.clone();
        move |conn| {
            repo::cleanup_security_state_conn(conn, now)?;
            repo::get_action_challenge_conn(conn, action_id.as_str(), actor_account_id.as_str())
        }
    }) {
        Ok(Some(v)) => v,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 1004, "admin action not found"),
        Err(err) => {
            let message = format!("query admin action failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };
    if challenge.consumed || now > challenge.expires_at {
        return api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            2004,
            "admin action expired",
        );
    }
    if !same_account_id(challenge.actor_account_id.as_str(), ctx.account_id.as_str()) {
        return api_error(StatusCode::FORBIDDEN, 1003, "admin action owner mismatch");
    }
    let actor_cid_number = match actor_cid_number_for_context(&state.db, &ctx) {
        Ok(value) => value,
        Err(resp) => return resp,
    };
    if challenge.actor_cid_number != actor_cid_number {
        return api_error(
            StatusCode::FORBIDDEN,
            1003,
            "admin action actor_cid_number mismatch",
        );
    }
    let action_type = match parse_action_type(challenge.action_type.as_str()) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if state
        .modules
        .owner_of_action(action_type.as_str())
        .is_some()
    {
        if let Some(module) = &state.modules.local {
            if let Err(response) = module.authorize_action(&ctx, &action_type) {
                return response;
            }
        }
    }
    if let Err(resp) = ensure_action_role_allowed(&ctx, &action_type) {
        return resp;
    }
    // 防御:只读档(Session)不产生可 commit 的动作;历史残留 grant 一律拒绝。
    if challenge.auth_type == AdminOperationAuth::Session {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "login state action cannot be committed",
        );
    }
    if challenge.auth_type != action_type.auth_type() {
        return api_error(
            StatusCode::FORBIDDEN,
            1003,
            "admin action auth type mismatch",
        );
    }
    // 本地解绑只校验 Passkey；冷签动作继续验证原钱包签名，不增加第二次冷签。
    if challenge.auth_type == AdminOperationAuth::PasskeyColdSign {
        let account_id = match input.account_id.as_deref() {
            Some(v) => v,
            None => return api_error(StatusCode::BAD_REQUEST, 1001, "account_id is required"),
        };
        let signature = match input.signature.as_deref() {
            Some(v) => v,
            None => return api_error(StatusCode::BAD_REQUEST, 1001, "signature is required"),
        };
        let payload_hash = match input.payload_hash.as_deref() {
            Some(v) => v,
            None => return api_error(StatusCode::BAD_REQUEST, 1001, "payload_hash is required"),
        };
        if let Err(resp) = verify_account_signature(
            ctx.account_id.as_str(),
            account_id,
            signature,
            payload_hash,
            challenge.payload_hash.as_str(),
            challenge.payload_text.as_str(),
        ) {
            return resp;
        }
        if let Err(resp) =
            ensure_signer_on_chain_admin(&state.db, &actor_cid_number, account_id).await
        {
            return resp;
        }
    }
    if action_type.is_governance() {
        // 此分支会直接写业务数据，必须在写入前消费一次 Passkey。
        if let Err(resp) = crate::authentication::passkey::require_passkey_assertion(
            &state,
            &headers,
            &ctx.account_id,
        ) {
            return resp;
        }
        if challenge.auth_type == AdminOperationAuth::Passkey {
            if let Err(resp) =
                ensure_signer_on_chain_admin(&state.db, &actor_cid_number, &ctx.account_id).await
            {
                return resp;
            }
        }
    }
    let result = state.db.with_client({
        let ctx = ctx.clone();
        let challenge = challenge.clone();
        let module = state.modules.local.clone();
        move |conn| {
            let mut tx = conn.transaction().map_err(|e| format!("begin action commit failed: {e}"))?;
            repo::lock_node_binding_conn(&mut tx)?;
            // 同一挑战的并发提交串行化，绑定改变也不能与正式写入交错。
            tx.query_opt("SELECT payload_json->>'actionId' FROM challenge WHERE payload_json->>'actionId'=$1 AND payload_json->>'actorAccountId'=$2 FOR UPDATE", &[&challenge.action_id, &ctx.account_id])
                .map_err(|e| format!("lock action challenge failed: {e}"))?;
            let conn = &mut tx;
            let now = Utc::now(); // 等待事务锁后重新判断真实有效期。
            repo::cleanup_security_state_conn(conn, now)?;
            let binding = repo::get_active_node_binding_conn(conn)?
                .ok_or_else(|| "http:forbidden:node binding missing".to_string())?;
            if binding.institution_cid_number != ctx.institution_cid_number || binding.institution_code != ctx.institution_code {
                return Err("http:forbidden:node binding changed".to_string());
            }
            let mut current = repo::get_action_challenge_conn(
                conn,
                challenge.action_id.as_str(),
                ctx.account_id.as_str(),
            )?
            .ok_or_else(|| "http:not_found:admin action not found".to_string())?;
            if current.consumed || now > current.expires_at {
                return Err("http:unprocessable:admin action expired".to_string());
            }
            let output: Result<CommitAdminActionOutput, String> = if action_type.is_governance() {
                recheck_preview_conn(conn, &ctx, &current, module.as_deref())?;
                let applied = apply_action_conn(conn, &ctx, &current, module.as_deref())?;
                current.consumed = true;
                repo::upsert_action_challenge_conn(conn, &current)?;
                Ok(CommitAdminActionOutput::Applied(applied))
            } else {
                let grant_id = format!("cid-admin-grant-{}", Uuid::new_v4());
                let grant_expires_at = now + Duration::seconds(ADMIN_SECURITY_GRANT_TTL_SECONDS);
                let grant = AdminSecurityGrant {
                    grant_id: grant_id.clone(),
                    action_type: action_type.as_str().to_string(),
                    actor_account_id: ctx.account_id.clone(),
                    actor_institution_code: ctx.institution_code.clone(),
                    actor_cid_number: current.actor_cid_number.clone(),
                    actor_province_name: ctx.scope_province_name.clone().unwrap_or_default(),
                    actor_city_name: ctx.scope_city_name.clone(),
                    auth_type: current.auth_type,
                    target: current.target.clone(),
                    payload_hash: hash_json(&current.request_payload),
                    issued_at: now,
                    expires_at: grant_expires_at,
                    consumed: false,
                };
                repo::insert_security_grant_conn(conn, &grant)?;
                current.consumed = true;
                repo::upsert_action_challenge_conn(conn, &current)?;
                Ok(CommitAdminActionOutput::Grant(AdminSecurityGrantOutput {
                    grant_id,
                    action_type: action_type.as_str().to_string(),
                    actor_cid_number: current.actor_cid_number,
                    auth_type: current.auth_type,
                    target: current.target,
                    expires_at: grant_expires_at.timestamp(),
                }))
            };
            let output = output?;
            tx.commit().map_err(|e| format!("commit admin action failed: {e}"))?;
            Ok(output)
        }
    });
    let output = match result {
        Ok(v) => v,
        Err(err) => return admin_action_error(err),
    };
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: output,
    })
    .into_response()
}

pub fn require_admin_security_grant(
    state: &AppState,
    headers: &HeaderMap,
    ctx: &AdminAuthContext,
    action_type: AdminActionType,
    target: &str,
    request_payload: Option<&serde_json::Value>,
    // 返回 `PasskeyProof` 而不是 `()`:该凭证是下游冷签会话创建入口
    // (`Db::insert_chain_sign_session`)的必填参数,把"链上写必须先过 passkey"
    // 从运行期约定升级成编译期约束,调用方拿不到凭证就发不出链交易。
) -> Result<crate::authentication::passkey::PasskeyProof, axum::response::Response> {
    ensure_action_role_allowed(ctx, &action_type)?;
    // 本地写(Passkey):会话 + passkey 断言 + 角色校验;不再有只会话的写动作。
    if action_type.auth_type() == AdminOperationAuth::Passkey {
        let passkey = crate::authentication::passkey::require_passkey_assertion(
            state,
            headers,
            &ctx.account_id,
        )?;
        ensure_action_role_allowed(ctx, &action_type)?;
        return Ok(passkey);
    }
    consume_admin_security_grant(state, headers, ctx, action_type, target, request_payload)
        .map(|(_, passkey)| passkey)
}

pub(crate) fn consume_admin_security_grant(
    state: &AppState,
    headers: &HeaderMap,
    ctx: &AdminAuthContext,
    action_type: AdminActionType,
    target: &str,
    request_payload: Option<&serde_json::Value>,
    // 同时返回冷签 grant 与 passkey 凭证:grant 供本次动作防重放,
    // 凭证供下游冷签会话创建入口做编译期校验,两者都不能丢。
) -> Result<
    (
        AdminSecurityGrant,
        crate::authentication::passkey::PasskeyProof,
    ),
    axum::response::Response,
> {
    ensure_action_role_allowed(ctx, &action_type)?;
    // PasskeyColdSign 档:先消费 passkey 断言(fail-closed,绝不降档),再消费冷签 grant。
    let passkey =
        crate::authentication::passkey::require_passkey_assertion(state, headers, &ctx.account_id)?;
    if action_type.auth_type() == AdminOperationAuth::Passkey {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "security grant is not available for passkey action",
        ));
    }
    let actor_cid_number = actor_cid_number_for_context(&state.db, ctx)?;
    let grant_id = headers
        .get(ADMIN_SECURITY_GRANT_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| api_error(StatusCode::FORBIDDEN, 1003, "security grant required"))?
        .to_string();
    let ctx = ctx.clone();
    let target = normalize_security_target(target);
    let request_payload = request_payload.cloned();
    let result = state.db.with_client(move |conn| {
        let mut tx = conn.transaction().map_err(|e| format!("begin grant consumption failed: {e}"))?;
        repo::lock_node_binding_conn(&mut tx)?;
        tx.query_opt("SELECT payload_json->>'grantId' FROM \"grant\" WHERE payload_json->>'grantId'=$1 AND payload_json->>'actorAccountId'=$2 FOR UPDATE", &[&grant_id, &ctx.account_id])
            .map_err(|e| format!("lock security grant failed: {e}"))?;
        let conn = &mut tx;
        let now = Utc::now(); // 排队等待不能延长一次性授权的有效期。
        let binding = repo::get_active_node_binding_conn(conn)?.ok_or("http:forbidden:node binding missing")?;
        if binding.institution_cid_number != actor_cid_number || binding.institution_code != ctx.institution_code {
            return Err("http:forbidden:security grant binding changed".into());
        }
        repo::cleanup_security_state_conn(conn, now)?;
        let Some(mut grant) =
            repo::get_security_grant_conn(conn, grant_id.as_str(), ctx.account_id.as_str())?
        else {
            return Err("http:forbidden:security grant not found".to_string());
        };
        if grant.consumed || now > grant.expires_at {
            return Err("http:forbidden:security grant expired".to_string());
        }
        if grant.action_type != action_type.as_str() {
            return Err("http:forbidden:security grant action mismatch".to_string());
        }
        if grant.auth_type != action_type.auth_type() {
            return Err("http:forbidden:security grant auth type mismatch".to_string());
        }
        if !same_account_id(grant.actor_account_id.as_str(), ctx.account_id.as_str())
            || grant.actor_institution_code != ctx.institution_code
            || grant.actor_cid_number != actor_cid_number
        {
            return Err("http:forbidden:security grant owner mismatch".to_string());
        }
        if grant.actor_province_name != ctx.scope_province_name.clone().unwrap_or_default()
            || grant.actor_city_name.as_deref() != ctx.scope_city_name.as_deref()
        {
            return Err("http:forbidden:security grant scope mismatch".to_string());
        }
        if grant.target != target {
            return Err("http:forbidden:security grant target mismatch".to_string());
        }
        if let Some(payload) = request_payload.as_ref() {
            if grant.payload_hash != hash_json(payload) {
                return Err("http:forbidden:security grant payload mismatch".to_string());
            }
        }
        let consumed = grant.clone();
        grant.consumed = true;
        repo::insert_security_grant_conn(conn, &grant)?;
        tx.commit().map_err(|e| format!("commit grant consumption failed: {e}"))?;
        Ok(consumed)
    });
    match result {
        Ok(grant) => Ok((grant, passkey)),
        Err(err) => Err(admin_action_error(err)),
    }
}

pub fn preview_action_conn(
    conn: &mut impl postgres::GenericClient,
    ctx: &AdminAuthContext,
    action_type: &AdminActionType,
    payload: &serde_json::Value,
) -> Result<ActionPreview, String> {
    match action_type {
        AdminActionType::NodeBindingUnbind => preview_node_binding_unbind_conn(conn, action_type),
        AdminActionType::InstitutionCreateAccount
        | AdminActionType::InstitutionDeleteAccount
        | AdminActionType::InstitutionDeleteDocument => {
            precheck_institution_target_scope_conn(conn, ctx, payload)?;
            preview_security_action(action_type, payload)
        }
        action if action.requires_module() => {
            Err("http:forbidden:module action unavailable".into())
        }
        _ => preview_security_action(action_type, payload),
    }
}

/// 对已存在机构的特殊操作(建/删账户、删机构文档)在 prepare 阶段预检管辖权,
/// 与业务 handler 共用准确机构与注册局辖区规则；prepare 和最终写入均独立校验。
pub fn precheck_institution_target_scope_conn(
    conn: &mut impl postgres::GenericClient,
    ctx: &AdminAuthContext,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let cid_number = payload
        .get("target")
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("cidNumber").and_then(|v| v.as_str()))
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "http:bad_request:cid_number is required".to_string())?;
    let Some((inst, _accounts)) = Db::get_institution_with_accounts_conn(conn, cid_number)? else {
        return Err("http:not_found:institution not found".to_string());
    };
    let scope = crate::authorization::rules::get_visible_scope(ctx);
    if !scope.includes_institution(
        &inst.cid_number,
        &inst.province_name,
        &inst.city_name,
        &inst.town_name,
    ) {
        return Err("http:forbidden:institution out of current admin scope".to_string());
    }
    Ok(())
}

pub fn preview_security_action(
    action_type: &AdminActionType,
    payload: &serde_json::Value,
) -> Result<ActionPreview, String> {
    let target = payload
        .get("target")
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("cidNumber").and_then(|v| v.as_str()))
        .or_else(|| payload.get("cidNumber").and_then(|v| v.as_str()))
        .or_else(|| payload.get("challengeId").and_then(|v| v.as_str()))
        .unwrap_or("*");
    let target = normalize_security_target(target);
    let request_hash = hash_json(payload);
    Ok(ActionPreview {
        before_hash: "security-grant".to_string(),
        after_hash: request_hash.clone(),
        target: target.clone(),
        auth_type: action_type.auth_type(),
    })
}

fn recheck_preview_conn(
    conn: &mut postgres::Transaction<'_>,
    ctx: &AdminAuthContext,
    challenge: &AdminActionChallenge,
    module: Option<&dyn crate::modules::model::ModuleBusiness>,
) -> Result<(), String> {
    let action = parse_action_type(&challenge.action_type)
        .map_err(|_| "http:bad_request:unknown action_type")?;
    let preview = match module {
        Some(module) => {
            module.preview_admin_transaction(conn, ctx, &action, &challenge.request_payload)?
        }
        None => preview_action_conn(conn, ctx, &action, &challenge.request_payload)?,
    };
    if preview.before_hash != challenge.before_hash {
        return Err("http:conflict:admin action state changed".into());
    }
    Ok(())
}

fn apply_action_conn(
    conn: &mut postgres::Transaction<'_>,
    ctx: &AdminAuthContext,
    challenge: &AdminActionChallenge,
    module: Option<&dyn crate::modules::model::ModuleBusiness>,
) -> Result<serde_json::Value, String> {
    let action = parse_action_type(&challenge.action_type)
        .map_err(|_| "http:bad_request:unknown action_type")?;
    if action == AdminActionType::NodeBindingUnbind {
        return apply_node_binding_unbind_conn(conn);
    }
    match module {
        Some(module) => module.apply_admin_transaction(conn, ctx, challenge),
        None => Err(
            "http:bad_request:business action cannot be applied by admin governance endpoint"
                .into(),
        ),
    }
}

pub fn preview_node_binding_unbind_conn(
    conn: &mut impl postgres::GenericClient,
    action_type: &AdminActionType,
) -> Result<ActionPreview, String> {
    let Some(binding) = repo::get_active_node_binding_conn(conn)? else {
        return Err("http:conflict:node binding missing".to_string());
    };
    let binding_id = binding.binding_id.clone();
    let candidate_id = binding.candidate_id.clone();
    let institution_code = binding.institution_code.clone();
    let after = json!({
        "unbind": true,
        "bindingId": binding_id,
        "candidateId": candidate_id,
        "institutionCode": institution_code,
    });
    Ok(ActionPreview {
        before_hash: hash_serialized(&binding),
        after_hash: hash_json(&after),
        target: binding.candidate_id,
        auth_type: action_type.auth_type(),
    })
}

pub fn apply_node_binding_unbind_conn(
    conn: &mut impl postgres::GenericClient,
) -> Result<serde_json::Value, String> {
    let Some(binding) = repo::get_active_node_binding_conn(conn)? else {
        return Err("http:conflict:node binding missing".to_string());
    };
    let changed = repo::deactivate_active_node_binding_conn(conn)?;
    if changed == 0 {
        return Err("http:conflict:node binding already inactive".to_string());
    }
    let removed_sessions = repo::delete_all_admin_sessions_conn(conn)?;
    Ok(json!({
        "bindingId": binding.binding_id,
        "candidateId": binding.candidate_id,
        "institutionCode": binding.institution_code,
        "status": "INACTIVE",
        "removedSessions": removed_sessions,
    }))
}

pub fn hash_serialized<T: Serialize>(value: &T) -> String {
    let encoded = serde_json::to_vec(value).unwrap_or_default();
    format!("0x{}", hex::encode(Sha256::digest(&encoded)))
}

fn normalize_security_target(target: &str) -> String {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        "*".to_string()
    } else {
        trimmed.to_string()
    }
}

fn admin_action_error(err: String) -> axum::response::Response {
    if let Some(message) = err.strip_prefix("http:bad_request:") {
        return api_error(StatusCode::BAD_REQUEST, 1001, message);
    }
    if let Some(message) = err.strip_prefix("http:forbidden:") {
        return api_error(StatusCode::FORBIDDEN, 1003, message);
    }
    if let Some(message) = err.strip_prefix("http:not_found:") {
        return api_error(StatusCode::NOT_FOUND, 1004, message);
    }
    if let Some(message) = err.strip_prefix("http:conflict:") {
        return api_error(StatusCode::CONFLICT, 1005, message);
    }
    if let Some(message) = err.strip_prefix("http:unprocessable:") {
        return api_error(StatusCode::UNPROCESSABLE_ENTITY, 2004, message);
    }
    let message = format!("admin action failed: {err}");
    api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/authentication_actions_action_tests.rs"]
mod action_tests;
