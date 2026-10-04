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
use postgres::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::action_sign::{
    hash_json, payload_hash_for_text, signed_payload_text, verify_account_signature,
    AdminSignedPayload, ADMIN_ACTION_TTL_SECONDS,
};
use crate::auth::city_registry_admins::{
    can_manage_city_registry, city_registry_row_from_user_conn,
    count_city_registry_admins_in_city_conn, ensure_city_in_province,
    find_city_registry_by_id_conn, MAX_ADMIN_PERSON_NAME_BYTES, MAX_CITY_REGISTRY_ADMINS_PER_CITY,
};
use crate::auth::login::AdminAuthContext;
use crate::auth::operation_auth::{
    ensure_action_role_allowed, parse_action_type, AdminActionType, AdminOperationAuth,
};
use crate::auth::repo;
use crate::auth::security_model::{AdminActionChallenge, AdminSecurityGrant};
use crate::core::qr::build_sign_request;
use crate::crypto::pubkey::{normalize_account_id, same_account_id};
use crate::*;

const ADMIN_SECURITY_GRANT_TTL_SECONDS: i64 = 120;
pub(crate) const ADMIN_SECURITY_GRANT_HEADER: &str = "x-cid-security-grant";

#[derive(Debug, Deserialize)]
pub(crate) struct PrepareAdminActionInput {
    action_type: AdminActionType,
    payload: serde_json::Value,
}

#[derive(Debug, Deserialize)]
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
pub(crate) struct PrepareAdminActionOutput {
    action_id: String,
    action_type: String,
    actor_cid_number: String,
    sign_request: Option<String>,
    payload_hash: String,
    auth_type: AdminOperationAuth,
    expires_at: i64,
}

#[derive(Debug, Deserialize, Serialize)]
struct CityRegistryIdPayload {
    id: u64,
}

struct ActionPreview {
    before_hash: String,
    after_hash: String,
    target: String,
    auth_type: AdminOperationAuth,
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
    use crate::core::chain_runtime;
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
        binding.frg_province_code.as_deref(),
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
    if let Err(resp) = ensure_action_role_allowed(&ctx, &input.action_type) {
        return resp;
    }
    if input.action_type.auth_type() != AdminOperationAuth::PasskeyColdSign {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "only cold-sign actions can be prepared",
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
        move |conn| preview_action_conn(conn, &ctx, &action_type, &payload)
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
                qr_proto: crate::core::qr::QR_V1,
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
                crate::core::qr::action_onchina_admin(),
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
    // ── 冷签 step-up:冷钱包扫码签名 + signer ∈ 本机构链上 Active 集合。
    //    所有可 commit 动作(Session 已在上方拒绝)一律走此校验。
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
    if let Err(resp) = ensure_signer_on_chain_admin(&state.db, &actor_cid_number, account_id).await
    {
        return resp;
    }
    let result = state.db.with_client({
        let ctx = ctx.clone();
        let challenge = challenge.clone();
        move |conn| {
            repo::cleanup_security_state_conn(conn, now)?;
            let mut current = repo::get_action_challenge_conn(
                conn,
                challenge.action_id.as_str(),
                ctx.account_id.as_str(),
            )?
            .ok_or_else(|| "http:not_found:admin action not found".to_string())?;
            if current.consumed || now > current.expires_at {
                return Err("http:unprocessable:admin action expired".to_string());
            }
            if action_type.is_governance() {
                recheck_preview_conn(conn, &ctx, &current)?;
                let applied = apply_action_conn(conn, &ctx, &current)?;
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
            }
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

pub(crate) fn require_admin_security_grant(
    state: &AppState,
    headers: &HeaderMap,
    ctx: &AdminAuthContext,
    action_type: AdminActionType,
    target: &str,
    request_payload: Option<&serde_json::Value>,
    // 返回 `PasskeyProof` 而不是 `()`:该凭证是下游冷签会话创建入口
    // (`Db::insert_chain_sign_session`)的必填参数,把"链上写必须先过 passkey"
    // 从运行期约定升级成编译期约束,调用方拿不到凭证就发不出链交易。
) -> Result<crate::auth::passkey::PasskeyProof, axum::response::Response> {
    // 本地写(Passkey):会话 + passkey 断言 + 角色校验;不再有只会话的写动作。
    if action_type.auth_type() == AdminOperationAuth::Passkey {
        let passkey =
            crate::auth::passkey::require_passkey_assertion(state, headers, &ctx.account_id)?;
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
) -> Result<(AdminSecurityGrant, crate::auth::passkey::PasskeyProof), axum::response::Response> {
    // PasskeyColdSign 档:先消费 passkey 断言(fail-closed,绝不降档),再消费冷签 grant。
    let passkey = crate::auth::passkey::require_passkey_assertion(state, headers, &ctx.account_id)?;
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
    let now = Utc::now();
    let result = state.db.with_client(move |conn| {
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
        Ok(consumed)
    });
    match result {
        Ok(grant) => Ok((grant, passkey)),
        Err(err) => Err(admin_action_error(err)),
    }
}

fn preview_action_conn(
    conn: &mut Client,
    ctx: &AdminAuthContext,
    action_type: &AdminActionType,
    payload: &serde_json::Value,
) -> Result<ActionPreview, String> {
    match action_type {
        AdminActionType::CreateCityRegistry => {
            let input: CreateCityRegistryAdminInput = serde_json::from_value(payload.clone())
                .map_err(|_| "http:bad_request:invalid create payload".to_string())?;
            let (account_id, family_name, given_name, city, creator_account_id) =
                validate_create_city_registry_conn(conn, ctx, &input)?;
            let after = json!({
                "institution_code": "CREG",
                "account_id": account_id,
                "family_name": family_name,
                "given_name": given_name,
                "city_name": city,
                "creator_account_id": creator_account_id,
            });
            let after_hash = hash_json(&after);
            Ok(ActionPreview {
                before_hash: "none".to_string(),
                after_hash: after_hash.clone(),
                target: account_id.clone(),
                auth_type: action_type.auth_type(),
            })
        }
        AdminActionType::DeleteCityRegistry => {
            let input: CityRegistryIdPayload = serde_json::from_value(payload.clone())
                .map_err(|_| "http:bad_request:invalid delete payload".to_string())?;
            let city_registry = require_manageable_city_registry_conn(conn, ctx, input.id)?;
            let before = city_registry_row_from_user_conn(conn, &city_registry)?;
            let after =
                json!({ "deleted": true, "id": input.id, "account_id": city_registry.account_id });
            let before_hash = hash_serialized(&before);
            let after_hash = hash_json(&after);
            Ok(ActionPreview {
                before_hash,
                after_hash,
                target: city_registry.account_id,
                auth_type: action_type.auth_type(),
            })
        }
        AdminActionType::NodeBindingUnbind => preview_node_binding_unbind_conn(conn, action_type),
        AdminActionType::InstitutionCreate => {
            precheck_institution_create_scope(ctx, payload)?;
            preview_security_action(action_type, payload)
        }
        AdminActionType::InstitutionCreateAccount
        | AdminActionType::InstitutionDeleteAccount
        | AdminActionType::InstitutionDeleteDocument => {
            precheck_institution_target_scope_conn(conn, ctx, payload)?;
            preview_security_action(action_type, payload)
        }
        _ => preview_security_action(action_type, payload),
    }
}

/// 对已存在机构的特殊操作(建/删账户、删机构文档)在 prepare 阶段预检管辖权,
/// 与 accounts handler 的 get_visible_scope 校验等价。文档删除流程的业务 handler 自身不含
/// 省/市校验,此预检即为其唯一管辖权闸:越权管理员拿不到一次性 grant,无法跨省操作他机构。
fn precheck_institution_target_scope_conn(
    conn: &mut Client,
    ctx: &AdminAuthContext,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let cid_number = payload
        .get("target")
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("cid_number").and_then(|v| v.as_str()))
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "http:bad_request:cid_number is required".to_string())?;
    let Some((inst, _accounts)) = Db::get_institution_with_accounts_conn(conn, cid_number)? else {
        return Err("http:not_found:institution not found".to_string());
    };
    let scope = crate::scope::rules::get_visible_scope(ctx);
    if !scope.includes_province(&inst.province_name)
        || !scope.includes_city(&inst.city_name)
        || !scope.includes_town(&inst.town_name)
    {
        return Err("http:forbidden:institution out of current admin scope".to_string());
    }
    Ok(())
}

/// 新建机构在 prepare 阶段预检省/市/镇管辖权,与 create_institution_inner 的
/// locked_province/locked_city 校验逐字段等价:scope 锁定省/市/镇时,申报省/市/镇必须留空或
/// 等于锁定值(留空交业务 handler 用锁定值回填),不会比 handler 更严而误拒。
/// 管理员阈值由链端按严格多数自动计算,prepare 不再接收 threshold 字段。
fn precheck_institution_create_scope(
    ctx: &AdminAuthContext,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let scope = crate::scope::rules::get_visible_scope(ctx);
    check_locked_field(
        scope.locked_province_name.as_deref(),
        payload.get("province_name").and_then(|v| v.as_str()),
        "province",
    )?;
    check_locked_field(
        scope.locked_city_name.as_deref(),
        payload.get("city_name").and_then(|v| v.as_str()),
        "city",
    )?;
    check_locked_field(
        scope.locked_town_name.as_deref(),
        payload.get("town_name").and_then(|v| v.as_str()),
        "town",
    )?;
    let admins = payload
        .get("admins")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "http:bad_request:admins is required".to_string())?;
    if admins.len() < 2 {
        return Err(
            "http:bad_request:institution admins must contain at least two accounts".to_string(),
        );
    }
    let mut normalized_account_ids: Vec<String> = Vec::with_capacity(admins.len());
    for item in admins {
        let raw = item
            .get("account_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| "http:bad_request:account_id is required".to_string())?;
        let Some(normalized) = normalize_account_id(raw) else {
            return Err("http:bad_request:account_id format invalid".to_string());
        };
        if normalized_account_ids
            .iter()
            .any(|account| same_account_id(account, normalized.as_str()))
        {
            return Err("http:bad_request:duplicate account_id".to_string());
        }
        normalized_account_ids.push(normalized);
    }
    Ok(())
}

/// scope 锁定某行政维度时,申报值必须留空(交 handler 回填)或逐字等于锁定值,
/// 否则视为越权。锁定为 None(该档不限此维度)时不校验。
fn check_locked_field(
    locked: Option<&str>,
    requested: Option<&str>,
    field: &str,
) -> Result<(), String> {
    if let (Some(locked), Some(requested)) =
        (locked, requested.map(str::trim).filter(|v| !v.is_empty()))
    {
        if requested != locked {
            return Err(format!("http:forbidden:{field} out of current admin scope"));
        }
    }
    Ok(())
}

fn preview_security_action(
    action_type: &AdminActionType,
    payload: &serde_json::Value,
) -> Result<ActionPreview, String> {
    let target = payload
        .get("target")
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("cid_number").and_then(|v| v.as_str()))
        .or_else(|| payload.get("cid_number").and_then(|v| v.as_str()))
        .or_else(|| payload.get("challenge_id").and_then(|v| v.as_str()))
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

fn validate_create_city_registry_conn(
    conn: &mut Client,
    ctx: &AdminAuthContext,
    input: &CreateCityRegistryAdminInput,
) -> Result<(String, String, String, String, String), String> {
    let Some(account_id) = normalize_account_id(input.account_id.as_str()) else {
        return Err("http:bad_request:account_id format invalid".to_string());
    };
    let family_name = validate_person_name(input.family_name.as_str(), "family_name")?;
    let given_name = validate_person_name(input.given_name.as_str(), "given_name")?;
    let creator_account_id = match input.creator_account_id.as_deref().map(str::trim) {
        None | Some("") => ctx.account_id.clone(),
        Some(raw) => {
            let Some(normalized) = normalize_account_id(raw) else {
                return Err("http:bad_request:creator_account_id format invalid".to_string());
            };
            if !same_account_id(normalized.as_str(), ctx.account_id.as_str()) {
                return Err(
                    "http:forbidden:FederalRegistry can only create city registry admins under itself"
                        .to_string(),
                );
            }
            normalized
        }
    };
    if let Some(existing) = repo::get_admin_by_account_id_conn(conn, account_id.as_str())? {
        return Err(duplicate_admin_account_error(&existing.institution_code));
    }
    let province_name = ctx
        .scope_province_name
        .as_deref()
        .ok_or_else(|| "http:forbidden:admin province scope missing".to_string())?;
    let (province, city) = ensure_city_in_province(province_name, input.city_name.as_str())
        .map_err(response_to_string)?;
    if count_city_registry_admins_in_city_conn(conn, province.as_str(), city.as_str())?
        >= MAX_CITY_REGISTRY_ADMINS_PER_CITY
    {
        return Err("http:conflict:city admin city limit reached".to_string());
    }
    Ok((
        account_id,
        family_name,
        given_name,
        city,
        creator_account_id,
    ))
}

fn validate_person_name(name: &str, field: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(format!("http:bad_request:{field} is required"));
    }
    if name.len() > MAX_ADMIN_PERSON_NAME_BYTES {
        return Err(format!("http:bad_request:{field} too long"));
    }
    Ok(name.to_string())
}

fn require_manageable_city_registry_conn(
    conn: &mut Client,
    ctx: &AdminAuthContext,
    id: u64,
) -> Result<AdminUser, String> {
    let city_registry = find_city_registry_by_id_conn(conn, id)?
        .ok_or_else(|| "http:not_found:city admin not found".to_string())?;
    if !can_manage_city_registry(ctx.scope_province_name.as_deref(), &city_registry) {
        return Err("http:forbidden:cannot manage other province city registry admins".to_string());
    }
    Ok(city_registry)
}

fn recheck_preview_conn(
    conn: &mut Client,
    ctx: &AdminAuthContext,
    challenge: &AdminActionChallenge,
) -> Result<(), String> {
    let action_type = parse_action_type(challenge.action_type.as_str())
        .map_err(|_| "http:bad_request:unknown action_type".to_string())?;
    let preview = preview_action_conn(conn, ctx, &action_type, &challenge.request_payload)?;
    if preview.before_hash != challenge.before_hash {
        return Err("http:conflict:admin action state changed".to_string());
    }
    Ok(())
}

fn apply_action_conn(
    conn: &mut Client,
    ctx: &AdminAuthContext,
    challenge: &AdminActionChallenge,
) -> Result<serde_json::Value, String> {
    let action_type = parse_action_type(challenge.action_type.as_str())
        .map_err(|_| "http:bad_request:unknown action_type".to_string())?;
    match action_type {
        AdminActionType::CreateCityRegistry => {
            let input: CreateCityRegistryAdminInput =
                serde_json::from_value(challenge.request_payload.clone())
                    .map_err(|_| "http:bad_request:invalid create payload".to_string())?;
            apply_create_city_registry_conn(conn, ctx, &input)
        }
        AdminActionType::DeleteCityRegistry => {
            let input: CityRegistryIdPayload =
                serde_json::from_value(challenge.request_payload.clone())
                    .map_err(|_| "http:bad_request:invalid delete payload".to_string())?;
            apply_delete_city_registry_conn(conn, ctx, &input)
        }
        AdminActionType::NodeBindingUnbind => apply_node_binding_unbind_conn(conn),
        _ => Err(
            "http:bad_request:business action cannot be applied by admin governance endpoint"
                .to_string(),
        ),
    }
}

fn preview_node_binding_unbind_conn(
    conn: &mut Client,
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
        "binding_id": binding_id,
        "candidate_id": candidate_id,
        "institution_code": institution_code,
    });
    Ok(ActionPreview {
        before_hash: hash_serialized(&binding),
        after_hash: hash_json(&after),
        target: binding.candidate_id,
        auth_type: action_type.auth_type(),
    })
}

fn apply_node_binding_unbind_conn(conn: &mut Client) -> Result<serde_json::Value, String> {
    let Some(binding) = repo::get_active_node_binding_conn(conn)? else {
        return Err("http:conflict:node binding missing".to_string());
    };
    let changed = repo::deactivate_active_node_binding_conn(conn)?;
    if changed == 0 {
        return Err("http:conflict:node binding already inactive".to_string());
    }
    let removed_sessions = repo::delete_all_admin_sessions_conn(conn)?;
    Ok(json!({
        "binding_id": binding.binding_id,
        "candidate_id": binding.candidate_id,
        "institution_code": binding.institution_code,
        "status": "INACTIVE",
        "removed_sessions": removed_sessions,
    }))
}

fn apply_create_city_registry_conn(
    conn: &mut Client,
    ctx: &AdminAuthContext,
    input: &CreateCityRegistryAdminInput,
) -> Result<serde_json::Value, String> {
    let (account_id, family_name, given_name, city, creator_account_id) =
        validate_create_city_registry_conn(conn, ctx, input)?;
    let now = Utc::now();
    let row = AdminUser {
        id: repo::next_admin_id_conn(conn)?,
        account_id: account_id.clone(),
        family_name,
        given_name,
        institution_code: "CREG".to_string(),
        built_in: false,
        creator_account_id,
        created_at: now,
        updated_at: Some(now),
        city_name: city,
    };
    repo::upsert_admin_conn(conn, &row)?;
    serde_json::to_value(city_registry_row_from_user_conn(conn, &row)?)
        .map_err(|e| format!("encode city admin failed: {e}"))
}

fn apply_delete_city_registry_conn(
    conn: &mut Client,
    ctx: &AdminAuthContext,
    input: &CityRegistryIdPayload,
) -> Result<serde_json::Value, String> {
    let city_registry = require_manageable_city_registry_conn(conn, ctx, input.id)?;
    let account_id = city_registry.account_id.clone();
    repo::delete_admin_runtime_state_conn(conn, account_id.as_str())?;
    conn.execute("DELETE FROM admins WHERE account_id = $1", &[&account_id])
        .map_err(|e| format!("delete city admin failed: {e}"))?;
    Ok(json!({ "deleted": true, "account_id": account_id }))
}

fn hash_serialized<T: Serialize>(value: &T) -> String {
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

fn duplicate_admin_account_error(institution_code: &str) -> String {
    if crate::core::chain_runtime::is_tier1_registry(institution_code) {
        "http:conflict:admin account_id already exists as federal admin"
    } else {
        "http:conflict:admin account_id already exists as city admin"
    }
    .to_string()
}

fn response_to_string(_resp: axum::response::Response) -> String {
    "http:bad_request:invalid request".to_string()
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
