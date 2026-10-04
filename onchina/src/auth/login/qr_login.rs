//! 管理员二维码登录 handler。
//!
//! 只承接 QR_V1 登录签名请求生成、手机扫码签名、网页轮询结果;普通登录仍在 `handler.rs`。

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{Duration, Utc};
use tracing::warn;
use uuid::Uuid;

use crate::auth::repo;
use crate::crypto::pubkey::same_account_id;
use crate::*;

use super::model::*;
use super::onchain_gate;
use super::signature::{admin_person_names, extract_domain_from_origin, verify_admin_signature};
use super::LOGIN_SIGN_REQUEST_TTL_SECONDS;

const LOGIN_QR_SYSTEM: &str = "onchina";

pub(crate) async fn admin_auth_qr_sign_request(
    State(state): State<AppState>,
    Json(input): Json<AdminQrSignRequestInput>,
) -> impl IntoResponse {
    let origin = input.origin.unwrap_or_default().trim().to_string();
    if origin.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, 1001, "origin is required");
    }
    let session_id = input.session_id.unwrap_or_default().trim().to_string();
    if session_id.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, 1001, "session_id is required");
    }
    let derived_domain = extract_domain_from_origin(&origin)
        .or_else(|| input.domain.clone())
        .unwrap_or_default();
    if derived_domain.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, 1001, "domain is required");
    }
    // 登录第 1 步只收钱包码：管理员私钥保管在离线的 CitizenWallet，钱包码由它自己
    // 就能出示。二维码不携带 CID 与昵称——后端从链上管理员名册读取身份字段，并在
    // 同一个 finalized 区块按分层规则解析当前可签名账户，禁止信任二维码自述身份。
    let scanned_account_id = match crate::core::qr::parse_account_id_code(&input.identity_qr) {
        Ok(value) => value,
        Err(error) => {
            let message = format!("identity_qr must be a complete QR_V1 account_id_code: {error}");
            return api_error(StatusCode::BAD_REQUEST, 1001, &message);
        }
    };
    let account_id = match onchain_gate::validate_login_identity(&scanned_account_id).await {
        Ok(value) => value,
        Err(error) => return onchain_gate::gate_error_response(error),
    };

    let now = Utc::now();
    let expire_at = now + Duration::seconds(LOGIN_SIGN_REQUEST_TTL_SECONDS);
    let challenge_id = Uuid::new_v4().to_string();
    // challenge_text:客户端生成 k=2 登录签名响应时的原文(与 CitizenWallet 端的
    // buildSignatureMessage(kind=signResponse, ...) 拼接规则保持一致)。
    // 注意 <principal> 位置由客户端签名时填入 account_id，后端验证时同样
    // 以 account_id 为 principal 重新拼接。这里保存的 challenge_text 仅作
    // 回放保护用的唯一 token,实际验证在 admin_auth_qr_complete 中重建。
    let challenge_text = format!(
        "{}|{}|{}|{}|{}|",
        crate::core::qr::QR_V1,
        crate::core::qr::QrKind::SignResponse.code(),
        challenge_id,
        LOGIN_QR_SYSTEM,
        expire_at.timestamp()
    );
    // 用户码先确定目标账户，b.u 必须携带该账户的 32 字节公钥；钱包不得任选账户签名。
    let login_body = match crate::core::qr::login_request_body(LOGIN_QR_SYSTEM, &account_id) {
        Ok(value) => value,
        Err(error) => {
            let message = format!("build targeted login request failed: {error}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, &message);
        }
    };
    let login_qr_payload = serde_json::to_string(&crate::core::qr::SignRequestEnvelope::new(
        challenge_id.clone(),
        now.timestamp(),
        expire_at.timestamp(),
        login_body,
    ))
    .unwrap_or_default();

    if let Err(err) = repo::insert_login_sign_request(
        &state.db,
        &LoginSignRequest {
            challenge_id: challenge_id.clone(),
            account_id,
            challenge_text: challenge_text.clone(),
            challenge_token: String::new(),
            qr_aud: String::new(),
            qr_origin: String::new(),
            origin: origin.clone(),
            domain: derived_domain.clone(),
            session_id: session_id.clone(),
            nonce: String::new(),
            issued_at: now,
            expire_at,
            consumed: false,
        },
    ) {
        let message = format!("insert qr sign request failed: {err}");
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
    }

    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: AdminQrSignRequestOutput {
            challenge_id,
            challenge_payload: challenge_text,
            login_qr_payload,
            origin,
            domain: derived_domain,
            session_id,
            expire_at: expire_at.timestamp(),
        },
    })
    .into_response()
}

pub(crate) async fn admin_auth_qr_complete(
    State(state): State<AppState>,
    Json(input): Json<AdminQrCompleteInput>,
) -> impl IntoResponse {
    if input.challenge_id.trim().is_empty()
        || input.account_id.trim().is_empty()
        || input.signature.trim().is_empty()
    {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "challenge_id, account_id, signature are required",
        );
    }

    let now = Utc::now();
    let challenge_id = input.challenge_id.trim().to_string();
    let client_session_id = input.session_id.clone();
    let Some(account_id) = crate::crypto::pubkey::normalize_account_id(input.account_id.as_str())
    else {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "account_id must be lowercase 0x plus 64 hexadecimal characters",
        );
    };
    let signature = input.signature.trim().to_string();
    let result = state.db.with_client(move |conn| {
        repo::cleanup_login_state_conn(conn, now)?;
        let Some(mut challenge) = repo::get_login_sign_request_conn(conn, challenge_id.as_str())?
        else {
            return Err("http:not_found:sign request not found".to_string());
        };
        if challenge.consumed {
            return Err("http:conflict:sign request already consumed".to_string());
        }
        if let Some(client_sid) = client_session_id.as_ref() {
            if challenge.session_id != client_sid.trim() {
                return Err("http:forbidden:sign request session mismatch".to_string());
            }
        }
        if now > challenge.expire_at {
            return Err("http:gone:sign request expired".to_string());
        }
        let session_id = challenge.session_id.clone();
        let challenge_expire_at = challenge.expire_at.timestamp();
        let verify_public_key = account_id.clone();
        if !same_account_id(challenge.account_id.as_str(), verify_public_key.as_str()) {
            return Err("http:forbidden:account_id must match targeted account_id".to_string());
        }
        // 重建完整签名原文,与 CitizenWallet 端 k=2 登录签名响应规则一致。
        let verify_message = crate::core::qr::build_signature_message(
            crate::core::qr::QrKind::SignResponse,
            challenge_id.as_str(),
            Some(LOGIN_QR_SYSTEM),
            Some(challenge_expire_at),
            &verify_public_key,
        );
        if !verify_admin_signature(&verify_public_key, &verify_message, signature.as_str()) {
            warn!(
                request = %challenge_id,
                account_id = %challenge.account_id,
                "qr login signature verify failed"
            );
            return Err("http:unprocessable:login signature verify failed".to_string());
        }
        // 响应只证明持有目标账户私钥，禁止用响应字段改写挑战绑定账户。
        challenge.consumed = true;
        let verified_account_id = challenge.account_id.clone();
        repo::update_login_sign_request_conn(conn, &challenge)?;
        Ok((session_id, verified_account_id))
    });

    let (session_id, verified_account_id) = match result {
        Ok(v) => v,
        Err(err) if err == "http:not_found:sign request not found" => {
            return api_error(StatusCode::NOT_FOUND, 1004, "sign request not found");
        }
        Err(err) if err == "http:conflict:sign request already consumed" => {
            return api_error(StatusCode::CONFLICT, 1007, "sign request already consumed");
        }
        Err(err) if err == "http:forbidden:sign request session mismatch" => {
            return api_error(StatusCode::FORBIDDEN, 1003, "sign request session mismatch");
        }
        Err(err) if err == "http:gone:sign request expired" => {
            return api_error(StatusCode::GONE, 1007, "sign request expired");
        }
        Err(err) if err == "http:forbidden:account_id must match targeted account_id" => {
            return api_error(
                StatusCode::FORBIDDEN,
                1003,
                "account_id must match targeted account_id",
            );
        }
        Err(err) if err == "http:unprocessable:login signature verify failed" => {
            return api_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                2004,
                "login signature verify failed",
            );
        }
        Err(err) => {
            let message = format!("complete qr login failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };

    // 链上集合鉴权;未绑定节点时返回候选机构,由浏览器二次确认后再签发会话。
    let outcome =
        match onchain_gate::issue_session_after_onchain_gate(&state, &verified_account_id, now)
            .await
        {
            Ok(v) => v,
            Err(err) => return onchain_gate::gate_error_response(err),
        };
    let (access_token, expire_at, output) = match outcome {
        onchain_gate::GateOutcome::Session {
            access_token,
            expire_at,
            admin,
        } => (access_token, expire_at, *admin),
        onchain_gate::GateOutcome::BindingRequired(binding) => {
            return Json(ApiResponse {
                code: 0,
                message: "ok".to_string(),
                data: AdminLoginCompleteOutput {
                    status: "BINDING_REQUIRED".to_string(),
                    access_token: None,
                    expire_at: None,
                    admin: None,
                    binding: Some(binding),
                },
            })
            .into_response();
        }
    };

    // 写入 QR 轮询结果,供网页端取回 access_token。
    let challenge_id_for_result = input.challenge_id.trim().to_string();
    let qr_result = QrLoginResultRecord {
        session_id,
        access_token: access_token.clone(),
        expire_at,
        account_id: output.account_id.clone(),
        institution_code: output.institution_code.clone(),
        created_at: now,
    };
    if let Err(err) = state.db.with_client(move |conn| {
        repo::insert_qr_login_result_conn(conn, challenge_id_for_result.as_str(), &qr_result)
    }) {
        let message = format!("persist qr login result failed: {err}");
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
    }

    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: AdminLoginCompleteOutput {
            status: "SUCCESS".to_string(),
            access_token: Some(access_token),
            expire_at: Some(expire_at.timestamp()),
            admin: Some(output),
            binding: None,
        },
    })
    .into_response()
}

pub(crate) async fn admin_auth_qr_result(
    State(state): State<AppState>,
    Query(query): Query<AdminQrResultQuery>,
) -> impl IntoResponse {
    if query.challenge_id.trim().is_empty() || query.session_id.trim().is_empty() {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "challenge_id and session_id are required",
        );
    }

    let now = Utc::now();
    let challenge_id = query.challenge_id.trim().to_string();
    let session_id = query.session_id.trim().to_string();
    let result = state.db.with_client(move |conn| {
        repo::cleanup_login_state_conn(conn, now)?;
        let result = repo::get_qr_login_result_conn(conn, challenge_id.as_str())?;
        let challenge = repo::get_login_sign_request_conn(conn, challenge_id.as_str())?;
        Ok((result, challenge))
    });
    let (qr_result, challenge) = match result {
        Ok(v) => v,
        Err(err) => {
            let message = format!("query qr login result failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };

    if let Some(result) = qr_result {
        if result.session_id != query.session_id.trim() {
            return api_error(StatusCode::FORBIDDEN, 1003, "challenge session mismatch");
        }
        let access_token_for_check = result.access_token.clone();
        let session_is_current = state.db.with_client(move |conn| {
            let Some(session) = repo::get_admin_session_conn(conn, &access_token_for_check)? else {
                return Ok(None);
            };
            let Some(binding) = repo::get_active_node_binding_conn(conn)? else {
                return Ok(None);
            };
            if session.candidate_id != binding.candidate_id
                || session.institution_code != binding.institution_code
            {
                return Ok(None);
            }
            Ok(Some(binding.institution_cid_number))
        });
        let institution_cid_number = match session_is_current {
            Ok(Some(value)) => value,
            Ok(None) => {
                return api_error(
                    StatusCode::UNAUTHORIZED,
                    1002,
                    "login session is no longer valid",
                );
            }
            Err(err) => {
                let message = format!("validate qr login session failed: {err}");
                return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
            }
        };
        let admin = match repo::get_admin_by_account_id(&state.db, &result.account_id) {
            Ok(v) => v,
            Err(err) => {
                let message = format!("query admin failed: {err}");
                return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
            }
        };
        let scope_account_id = result.account_id.clone();
        let institution_code_for_scope = result.institution_code.clone();
        let (province, scope_city_name, scope_town_name) = match state.db.with_client(move |conn| {
            repo::derive_admin_scope_conn(
                conn,
                scope_account_id.as_str(),
                institution_code_for_scope.as_str(),
            )
        }) {
            Ok(v) => v,
            Err(err) => {
                let message = format!("query admin scope failed: {err}");
                return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
            }
        };
        if province.as_deref().map(str::trim).unwrap_or("").is_empty() {
            return api_error(StatusCode::FORBIDDEN, 2002, "admin province scope missing");
        }
        let cid_short_name = repo::resolve_home_cid_short_name(
            &state.db,
            &result.institution_code,
            province.as_deref(),
            scope_city_name.as_deref(),
        )
        .unwrap_or(None);
        let capabilities = crate::platform::capability::capabilities_for(&result.institution_code);
        let workspace_modules =
            crate::domains::membership::workspace_modules_for(&institution_cid_number).await;
        let workspace = crate::workspace::build_institution_workspace(
            &result.institution_code,
            cid_short_name.as_deref(),
            capabilities,
            workspace_modules,
        );
        let (family_name, given_name) = admin
            .as_ref()
            .map(admin_person_names)
            .unwrap_or_else(|| ("管理".to_string(), "员".to_string()));
        return Json(ApiResponse {
            code: 0,
            message: "ok".to_string(),
            data: AdminQrResultOutput {
                status: "SUCCESS".to_string(),
                message: "login success".to_string(),
                access_token: Some(result.access_token.clone()),
                expire_at: Some(result.expire_at.timestamp()),
                admin: Some(AdminIdentifyOutput {
                    account_id: result.account_id.clone(),
                    institution_cid_number,
                    institution_code: result.institution_code.clone(),
                    admin_level: crate::core::chain_runtime::admin_level_label_for(
                        &result.institution_code,
                    ),
                    capabilities,
                    workspace,
                    family_name,
                    given_name,
                    scope_province_name: province,
                    scope_city_name,
                    scope_town_name,
                    cid_short_name,
                }),
            },
        })
        .into_response();
    }

    let Some(challenge) = challenge else {
        return api_error(StatusCode::NOT_FOUND, 1004, "challenge not found");
    };
    if challenge.session_id != session_id {
        return api_error(StatusCode::FORBIDDEN, 1003, "challenge session mismatch");
    }
    if now > challenge.expire_at {
        return Json(ApiResponse {
            code: 0,
            message: "ok".to_string(),
            data: AdminQrResultOutput {
                status: "EXPIRED".to_string(),
                message: "challenge expired".to_string(),
                access_token: None,
                expire_at: None,
                admin: None,
            },
        })
        .into_response();
    }

    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: AdminQrResultOutput {
            status: "PENDING".to_string(),
            message: "waiting mobile scan".to_string(),
            access_token: None,
            expire_at: None,
            admin: None,
        },
    })
    .into_response()
}
