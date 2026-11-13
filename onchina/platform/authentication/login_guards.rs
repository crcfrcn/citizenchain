//! 登录会话鉴权守卫。
//!
//! 普通读取只检查已签发会话，不重新核验链上管理员资格。
//! 业务模块通过 `require_admin_any` 获取认证上下文;
//! 写操作的冷钱包扫码签名(PasskeyColdSign 档)由 admins::actions 的安全 grant 单独校验。

// 登录守卫直接携带统一拒绝响应，保证所有调用方复用完全相同的状态码和错误体。
#![allow(clippy::result_large_err)]

use axum::http::{HeaderMap, StatusCode};
use chrono::{Duration, Utc};
use std::sync::atomic::{AtomicI64, Ordering};

use crate::authentication::repo;
use crate::*;

use super::model::AdminAuthContext;

/// 会话恢复只读本地会话及绑定；资格与地域已在登录阶段验证。
pub(crate) fn admin_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<super::model::AdminSession, axum::response::Response> {
    let token = bearer_token(headers)
        .ok_or_else(|| api_error(StatusCode::UNAUTHORIZED, 1002, "admin auth required"))?;
    let now = Utc::now();
    let idle_timeout_minutes = std::env::var("ONCHINA_ADMIN_IDLE_TIMEOUT_MINUTES")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(10);
    static LAST_CLEANUP: AtomicI64 = AtomicI64::new(0);
    let last = LAST_CLEANUP.load(Ordering::Relaxed);
    let cleanup = now.timestamp() - last > 60
        && LAST_CLEANUP
            .compare_exchange(last, now.timestamp(), Ordering::Relaxed, Ordering::Relaxed)
            .is_ok();
    let result = state.db.with_client(move |conn| {
        if cleanup {
            repo::cleanup_admin_sessions_conn(conn, now, idle_timeout_minutes)?;
        }
        let Some(mut session) = repo::get_admin_session_conn(conn, &token)? else {
            return Err("http:unauthorized:invalid access token".to_string());
        };
        let expired = now > session.expires_at
            || (crate::chain_runtime::is_subordinate_registry(&session.institution_code)
                && now > session.last_active_at + Duration::minutes(idle_timeout_minutes));
        if expired {
            conn.execute(
                "DELETE FROM session WHERE token_hash=$1",
                &[&repo::admin_session_token_hash(&token)],
            )
            .map_err(|error| error.to_string())?;
            return Err("http:unauthorized:access token expired".to_string());
        }
        let binding = repo::get_active_node_binding_conn(conn)?;
        if !binding.as_ref().is_some_and(|binding| {
            session.candidate_id == binding.candidate_id
                && session.institution_code == binding.institution_code
                && session.institution_cid_number == binding.institution_cid_number
        }) {
            conn.execute(
                "DELETE FROM session WHERE token_hash=$1",
                &[&repo::admin_session_token_hash(&token)],
            )
            .map_err(|error| error.to_string())?;
            return Err("http:unauthorized:invalid access token".to_string());
        }
        session.last_active_at = now;
        repo::touch_admin_session_conn(conn, &session)?;
        Ok(session)
    });
    result.map_err(|error| match error.as_str() {
        "http:unauthorized:invalid access token" => {
            api_error(StatusCode::UNAUTHORIZED, 1002, "invalid access token")
        }
        "http:unauthorized:access token expired" => {
            api_error(StatusCode::UNAUTHORIZED, 1002, "access token expired")
        }
        _ => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            5001,
            &format!("admin session failed: {error}"),
        ),
    })
}

/// 设置初始化、会话检查等入口使用有效会话，不要求已有凭证。
pub fn require_admin_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminAuthContext, axum::response::Response> {
    admin_session(state, headers).map(|session| session.context())
}

/// 所有业务读取和写入都先完成强制设置；此处不读链、不消费Passkey。
pub fn require_admin_any(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminAuthContext, axum::response::Response> {
    let session = admin_session(state, headers)?;
    if session.setup_required {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            2003,
            "passkey setup required",
        ));
    }
    Ok(session.context())
}

pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let auth = headers.get("authorization")?.to_str().ok()?.trim();
    let token = auth.strip_prefix("Bearer ")?;
    if token.trim().is_empty() {
        return None;
    }
    Some(token.trim().to_string())
}
