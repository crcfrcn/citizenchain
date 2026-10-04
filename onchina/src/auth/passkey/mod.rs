//! WebAuthn passkey 鉴权:注册 / 断言 ceremony + 一次性断言令牌。
//!
//! 三档鉴权的 Passkey / PasskeyColdSign 档 step-up:管理员先完成 passkey 断言换取一次性
//! assertion 令牌,提交重要 / 特殊操作时携 `X-Passkey-Assertion` 头消费。
//! - RP 配置取 env `ONCHINA_PASSKEY_RP_ID` / `ONCHINA_PASSKEY_ORIGIN`,默认固定 onchina.local HTTPS。
//! - 独立 WebAuthn 协议,绝不复用 QR_V1 / GMB / AdminSignedPayload。
//! - 断言失败 / RP 未配一律拒,绝不降档到 Session。

// Passkey 校验失败沿用统一 Axum Response，保持 step-up 鉴权所有拒绝分支同源。
#![allow(clippy::result_large_err)]

mod store;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use webauthn_rs::prelude::{
    PasskeyAuthentication, PasskeyRegistration, PublicKeyCredential, RegisterPublicKeyCredential,
    Url, Webauthn, WebauthnBuilder,
};

use crate::auth::login::require_admin_any;
use crate::core::response::ApiResponse;
use crate::{api_error, AppState};

const CEREMONY_TTL_SECONDS: i64 = 300;
const ASSERTION_TTL_SECONDS: i64 = 120;

/// X-Passkey-Assertion 头:携一次性断言令牌,Passkey / PasskeyColdSign 档提交时消费。
pub(crate) const PASSKEY_ASSERTION_HEADER: &str = "x-passkey-assertion";

/// 管理员 passkey user handle 的 uuid v5 命名空间(由 account_id 稳定派生,不入库)。
const PASSKEY_USER_NS: Uuid = Uuid::from_bytes([
    0x6f, 0x6e, 0x63, 0x68, 0x69, 0x6e, 0x61, 0x70, 0x61, 0x73, 0x6b, 0x65, 0x79, 0x75, 0x69, 0x64,
]);

/// 从 env 构造 WebAuthn(RP id / origin);未配或非法 → Err(fail-closed)。
fn build_webauthn_from(rp_id: &str, origin: &str) -> Result<Webauthn, String> {
    let rp_origin = Url::parse(origin.trim())
        .map_err(|e| format!("invalid ONCHINA_PASSKEY_ORIGIN `{origin}`: {e}"))?;
    WebauthnBuilder::new(rp_id.trim(), &rp_origin)
        .map_err(|e| format!("webauthn rp config invalid: {e}"))?
        .rp_name("onchina")
        .build()
        .map_err(|e| format!("webauthn build failed: {e}"))
}

fn build_webauthn() -> Result<Webauthn, String> {
    let rp_id =
        std::env::var("ONCHINA_PASSKEY_RP_ID").unwrap_or_else(|_| "onchina.local".to_string());
    let origin = std::env::var("ONCHINA_PASSKEY_ORIGIN")
        .unwrap_or_else(|_| "https://onchina.local:8964".to_string());
    build_webauthn_from(rp_id.trim(), origin.trim())
}

fn admin_user_uuid(account_id: &str) -> Uuid {
    Uuid::new_v5(&PASSKEY_USER_NS, account_id.as_bytes())
}

#[derive(Serialize)]
pub(crate) struct PasskeyBeginOutput<T: Serialize> {
    ceremony_id: String,
    challenge: T,
}

#[derive(Deserialize)]
pub(crate) struct PasskeyRegisterFinishInput {
    ceremony_id: String,
    credential: RegisterPublicKeyCredential,
}

#[derive(Deserialize)]
pub(crate) struct PasskeyAssertFinishInput {
    ceremony_id: String,
    credential: PublicKeyCredential,
}

#[derive(Serialize)]
pub(crate) struct PasskeyAssertionOutput {
    assertion_id: String,
    expire_at: i64,
}

#[derive(Serialize)]
pub(crate) struct PasskeyStatusOutput {
    registered: bool,
}

fn passkey_config_error() -> Response {
    api_error(
        StatusCode::SERVICE_UNAVAILABLE,
        5003,
        "passkey not configured",
    )
}

fn passkey_error(err: String) -> Response {
    if let Some(rest) = err.strip_prefix("http:forbidden:") {
        return api_error(StatusCode::FORBIDDEN, 2003, rest);
    }
    if let Some(rest) = err.strip_prefix("http:bad_request:") {
        return api_error(StatusCode::BAD_REQUEST, 1001, rest);
    }
    tracing::error!(error = %err, "passkey operation failed");
    api_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        5001,
        "passkey operation failed",
    )
}

/// 开始注册:为登录管理员生成 passkey 注册挑战。
pub(crate) async fn register_begin(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let webauthn = match build_webauthn() {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(error = %err, "passkey config invalid");
            return passkey_config_error();
        }
    };
    let account_id = ctx.account_id.clone();
    let user_uuid = admin_user_uuid(&account_id);
    let now = Utc::now();
    let result = state.db.with_client(move |conn| {
        store::cleanup_passkey_state_conn(conn, now)?;
        let existing = store::list_credentials_for_admin_conn(conn, account_id.as_str())?;
        let exclude = existing
            .iter()
            .map(|p| p.cred_id().clone())
            .collect::<Vec<_>>();
        let (ccr, reg_state) = webauthn
            .start_passkey_registration(
                user_uuid,
                account_id.as_str(),
                account_id.as_str(),
                if exclude.is_empty() {
                    None
                } else {
                    Some(exclude)
                },
            )
            .map_err(|e| format!("start passkey registration failed: {e}"))?;
        let ceremony_id = format!("pk-reg-{}", Uuid::new_v4());
        let state_json = serde_json::to_value(&reg_state)
            .map_err(|e| format!("encode reg state failed: {e}"))?;
        store::insert_ceremony_conn(
            conn,
            ceremony_id.as_str(),
            account_id.as_str(),
            "REG",
            &state_json,
            now + Duration::seconds(CEREMONY_TTL_SECONDS),
        )?;
        Ok((ceremony_id, ccr))
    });
    match result {
        Ok((ceremony_id, challenge)) => Json(ApiResponse {
            code: 0,
            message: "ok".to_string(),
            data: PasskeyBeginOutput {
                ceremony_id,
                challenge,
            },
        })
        .into_response(),
        Err(err) => passkey_error(err),
    }
}

/// 完成注册:校验认证器响应并落库 passkey 凭证。
pub(crate) async fn register_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PasskeyRegisterFinishInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let webauthn = match build_webauthn() {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(error = %err, "passkey config invalid");
            return passkey_config_error();
        }
    };
    let account_id = ctx.account_id.clone();
    let now = Utc::now();
    let result = state.db.with_client(move |conn| {
        let Some(state_json) = store::take_ceremony_conn(
            conn,
            input.ceremony_id.as_str(),
            account_id.as_str(),
            "REG",
            now,
        )?
        else {
            return Err("http:forbidden:passkey ceremony invalid or expired".to_string());
        };
        let reg_state: PasskeyRegistration = serde_json::from_value(state_json)
            .map_err(|e| format!("decode reg state failed: {e}"))?;
        let passkey = webauthn
            .finish_passkey_registration(&input.credential, &reg_state)
            .map_err(|e| format!("http:bad_request:passkey registration verify failed: {e}"))?;
        let credential_id = hex::encode(passkey.cred_id().as_ref());
        store::insert_credential_conn(conn, credential_id.as_str(), account_id.as_str(), &passkey)?;
        Ok(())
    });
    match result {
        Ok(()) => Json(ApiResponse {
            code: 0,
            message: "ok".to_string(),
            data: "passkey registered",
        })
        .into_response(),
        Err(err) => passkey_error(err),
    }
}

/// 开始断言:为登录管理员生成 passkey 断言挑战(无已注册凭证则拒)。
pub(crate) async fn assert_begin(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let webauthn = match build_webauthn() {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(error = %err, "passkey config invalid");
            return passkey_config_error();
        }
    };
    let account_id = ctx.account_id.clone();
    let now = Utc::now();
    let result = state.db.with_client(move |conn| {
        store::cleanup_passkey_state_conn(conn, now)?;
        let passkeys = store::list_credentials_for_admin_conn(conn, account_id.as_str())?;
        if passkeys.is_empty() {
            return Err("http:forbidden:no passkey registered".to_string());
        }
        let (rcr, auth_state) = webauthn
            .start_passkey_authentication(&passkeys)
            .map_err(|e| format!("start passkey authentication failed: {e}"))?;
        let ceremony_id = format!("pk-auth-{}", Uuid::new_v4());
        let state_json = serde_json::to_value(&auth_state)
            .map_err(|e| format!("encode auth state failed: {e}"))?;
        store::insert_ceremony_conn(
            conn,
            ceremony_id.as_str(),
            account_id.as_str(),
            "AUTH",
            &state_json,
            now + Duration::seconds(CEREMONY_TTL_SECONDS),
        )?;
        Ok((ceremony_id, rcr))
    });
    match result {
        Ok((ceremony_id, challenge)) => Json(ApiResponse {
            code: 0,
            message: "ok".to_string(),
            data: PasskeyBeginOutput {
                ceremony_id,
                challenge,
            },
        })
        .into_response(),
        Err(err) => passkey_error(err),
    }
}

/// 完成断言:校验认证器响应,更新凭证 counter,签发一次性断言令牌。
pub(crate) async fn assert_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PasskeyAssertFinishInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let webauthn = match build_webauthn() {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(error = %err, "passkey config invalid");
            return passkey_config_error();
        }
    };
    let account_id = ctx.account_id.clone();
    let now = Utc::now();
    let result = state.db.with_client(move |conn| {
        let Some(state_json) = store::take_ceremony_conn(
            conn,
            input.ceremony_id.as_str(),
            account_id.as_str(),
            "AUTH",
            now,
        )?
        else {
            return Err("http:forbidden:passkey ceremony invalid or expired".to_string());
        };
        let auth_state: PasskeyAuthentication = serde_json::from_value(state_json)
            .map_err(|e| format!("decode auth state failed: {e}"))?;
        let auth_result = webauthn
            .finish_passkey_authentication(&input.credential, &auth_state)
            .map_err(|e| format!("http:forbidden:passkey authentication verify failed: {e}"))?;
        // counter 递增 / backup 状态变化时回写匹配凭证(webauthn-rs 已做 counter 回退安全校验)。
        if auth_result.needs_update() {
            let mut passkeys = store::list_credentials_for_admin_conn(conn, account_id.as_str())?;
            for passkey in passkeys.iter_mut() {
                if passkey.update_credential(&auth_result) == Some(true) {
                    let credential_id = hex::encode(passkey.cred_id().as_ref());
                    store::update_credential_conn(conn, credential_id.as_str(), passkey)?;
                }
            }
        }
        let assertion_id = format!("pk-assert-{}", Uuid::new_v4());
        let expire_at = now + Duration::seconds(ASSERTION_TTL_SECONDS);
        store::insert_assertion_conn(conn, assertion_id.as_str(), account_id.as_str(), expire_at)?;
        Ok((assertion_id, expire_at.timestamp()))
    });
    match result {
        Ok((assertion_id, expire_at)) => Json(ApiResponse {
            code: 0,
            message: "ok".to_string(),
            data: PasskeyAssertionOutput {
                assertion_id,
                expire_at,
            },
        })
        .into_response(),
        Err(err) => passkey_error(err),
    }
}

/// 查询当前管理员是否已注册 passkey(操作列红点 / 登录默认跳转用)。
pub(crate) async fn passkey_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let account_id = ctx.account_id.clone();
    let result = state
        .db
        .with_client(move |conn| store::admin_has_credential_conn(conn, account_id.as_str()));
    match result {
        Ok(registered) => Json(ApiResponse {
            code: 0,
            message: "ok".to_string(),
            data: PasskeyStatusOutput { registered },
        })
        .into_response(),
        Err(err) => {
            let message = format!("query passkey status failed: {err}");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str())
        }
    }
}

/// passkey 断言已成功消费的凭证。
///
/// 字段私有且本模块不提供任何公开构造函数,全仓唯一产出点是
/// [`require_passkey_assertion`]。凡是要求"链上写必须先过 passkey"的下游入口
/// (如冷签会话创建)都把它作为必填参数,于是"忘记做 passkey 就发链交易"
/// 变成编译期错误,而不是靠每个 handler 自觉调用——那正是本轮审计发现
/// 8 个冷签会话创建点里有 4 个漏做 passkey 的根因。
#[derive(Debug)]
pub(crate) struct PasskeyProof(());

/// Passkey / PasskeyColdSign 档 step-up:消费 `X-Passkey-Assertion` 头的一次性断言令牌。
///
/// 头缺失 / 令牌无效过期 / RP 未配 → 拒(fail-closed,绝不降档到 Session)。
///
/// 成功时返回 [`PasskeyProof`],作为下游链上写入口的通行凭证。
pub(crate) fn require_passkey_assertion(
    state: &AppState,
    headers: &HeaderMap,
    account_id: &str,
) -> Result<PasskeyProof, Response> {
    // RP 未配置(部署缺 env / 非安全上下文)→ 直接拒,不放行。
    if let Err(err) = build_webauthn() {
        tracing::error!(error = %err, "passkey config invalid; rejecting step-up");
        return Err(passkey_config_error());
    }
    let assertion_id = headers
        .get(PASSKEY_ASSERTION_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| api_error(StatusCode::FORBIDDEN, 2003, "passkey assertion required"))?
        .to_string();
    let account_id = account_id.to_string();
    let now = Utc::now();
    let consumed = state
        .db
        .with_client(move |conn| {
            store::consume_assertion_conn(conn, assertion_id.as_str(), account_id.as_str(), now)
        })
        .map_err(|err| {
            let message = format!("passkey assertion check failed: {err}");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str())
        })?;
    if !consumed {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            2003,
            "passkey assertion invalid or expired",
        ));
    }
    Ok(PasskeyProof(()))
}

#[cfg(test)]
// Passkey 测试必须在任一步骤失败时立即暴露夹具问题，断言式解包仅限测试。
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn admin_user_uuid_is_stable_for_canonical_account_id() {
        let account_id = "0x1111111111111111111111111111111111111111111111111111111111111111";
        let a = admin_user_uuid(account_id);
        let b = admin_user_uuid(account_id);
        assert_eq!(a, b, "相同规范账户 ID 的 user handle 必须稳定");
        let c =
            admin_user_uuid("0x2222222222222222222222222222222222222222222222222222222222222222");
        assert_ne!(a, c, "不同账户 user handle 必须不同");
    }

    #[test]
    fn build_webauthn_config_validation() {
        // 直接调内层函数,不碰进程级 env(避免与并行测试争用)。
        assert!(
            build_webauthn_from("localhost", "not a url").is_err(),
            "非法 origin 必须 fail-closed"
        );
        assert!(
            build_webauthn_from("onchina.local", "https://onchina.local:8964").is_ok(),
            "onchina.local HTTPS 默认配置必须合法"
        );
    }

    #[test]
    fn passkey_register_and_authenticate_round_trip() {
        use webauthn_authenticator_rs::softpasskey::SoftPasskey;
        use webauthn_authenticator_rs::WebauthnAuthenticator;
        use webauthn_rs::prelude::Url;

        let webauthn =
            build_webauthn_from("onchina.local", "https://onchina.local:8964").expect("build");
        let origin = Url::parse("https://onchina.local:8964").unwrap();
        // falsify_uv=true:软认证器提供 UserVerification(配置要求 UV)。
        let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));

        // 注册往返:server 出挑战 → 软认证器签 → server 验证落凭证。
        let user_id = admin_user_uuid("0xtestadmin");
        let (ccr, reg_state) = webauthn
            .start_passkey_registration(user_id, "0xtestadmin", "0xtestadmin", None)
            .expect("start registration");
        let reg = authenticator
            .do_registration(origin.clone(), ccr)
            .expect("authenticator register");
        let passkey = webauthn
            .finish_passkey_registration(&reg, &reg_state)
            .expect("finish registration");

        // 断言往返:server 出挑战 → 软认证器签 → server 验证 → 命中注册凭证。
        let (rcr, auth_state) = webauthn
            .start_passkey_authentication(std::slice::from_ref(&passkey))
            .expect("start authentication");
        let auth = authenticator
            .do_authentication(origin, rcr)
            .expect("authenticator authenticate");
        let result = webauthn
            .finish_passkey_authentication(&auth, &auth_state)
            .expect("finish authentication");
        assert_eq!(
            result.cred_id(),
            passkey.cred_id(),
            "断言结果必须命中注册的凭证"
        );
    }
}
