//! 校验已签发管理员会话，并向业务处理提供认证上下文。

#![allow(clippy::result_large_err)]

use axum::http::{HeaderMap, StatusCode};
use chrono::{Duration, Utc};
use std::sync::atomic::{AtomicI64, Ordering};

use crate::authentication::repo;
use crate::*;

use super::model::AdminAuthContext;

/// 本地会话不授予当前资格；HTTP中间件及广播前复核最终块钱包、版本与辖区。
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

/// 本地就绪门不消费Passkey；完整HTTP入口先经中间件复核链上钱包，处理函数保留原业务范围。
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

/// 受保护请求/首次广播只接受当前实际钱包和在用版本；旧快照不提供业务权限。
pub(crate) async fn validate_current_wallet(
    state: &AppState,
    ctx: &AdminAuthContext,
) -> Result<(), axum::response::Response> {
    // 测试快照来自各自隔离Db实例，不能影响正式二进制或其他测试会话。
    #[cfg(test)]
    if let Some(snapshot) = state
        .db
        .wallet_snapshot
        .lock()
        .map_err(|_| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                5001,
                "test snapshot unavailable",
            )
        })?
        .clone()
    {
        let equal = ctx.account_id == snapshot.account_id
            && ctx.wallet_revision == snapshot.wallet_revision
            && ctx.wallet_namespace == snapshot.wallet_namespace
            && ctx.institution_cid_number == snapshot.institution_cid_number
            && ctx.institution_code == snapshot.institution_code
            && ctx.scope_province_name == snapshot.scope_province_name
            && ctx.scope_city_name == snapshot.scope_city_name
            && ctx.scope_town_name == snapshot.scope_town_name;
        return if equal {
            Ok(())
        } else {
            Err(api_error(
                StatusCode::UNAUTHORIZED,
                1002,
                "institution wallet or version is no longer active",
            ))
        };
    }
    let result = async {
        let identity = crate::chain_runtime::identity_from_binding_parts(
            &ctx.institution_code,
            Some(&ctx.institution_cid_number),
        )?;
        let finalized = crate::chain_runtime::FinalizedChainView::connect().await?;
        let admins = crate::chain_runtime::fetch_active_admins_onchain_at(&identity, &finalized, Some(&ctx.account_id))
            .await?
            .unwrap_or_default();
        let Some(admin) = admins.iter().find(|a| a.account_id == ctx.account_id) else {
            return Ok(false);
        };
        if !ctx.matches_wallet(admin) {
            return Ok(false);
        }
        if ctx.institution_code == "FRG" {
            let provinces = crate::institutions::fetch_frg_province_codes_for_admin(
                &finalized,
                identity.cid_number.as_bytes(),
                admin.role_account_id,
            )
            .await?;
            if provinces.len() != 1 {
                return Ok(false);
            }
            let membership = crate::chain_runtime::ActiveAdminMembership {
                institution_code: primitives::cid::code::institution_code_from_cid_number(
                    &ctx.institution_cid_number,
                )
                .ok_or("invalid institution CID")?,
                cid_number: identity.cid_number,
                frg_province_code: Some(provinces[0]),
            };
            let candidate = state.db.with_client(|conn| {
                super::onchain_gate::candidate_from_membership_conn(conn, &membership)
            })?;
            if candidate.scope_province_name != ctx.scope_province_name
                || candidate.scope_city_name != ctx.scope_city_name
                || candidate.scope_town_name != ctx.scope_town_name
            {
                return Ok(false);
            }
        }
        Ok::<bool, String>(true)
    }
    .await;
    match result {
        Ok(true) => Ok(()),
        Ok(false) => Err(api_error(
            StatusCode::UNAUTHORIZED,
            1002,
            "institution wallet or version is no longer active",
        )),
        Err(error) => {
            tracing::warn!(%error,"current institution wallet check failed");
            Err(api_error(
                StatusCode::SERVICE_UNAVAILABLE,
                1503,
                "current institution wallet unavailable",
            ))
        }
    }
}
