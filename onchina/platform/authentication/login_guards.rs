//! 校验已签发管理员会话，并向业务处理提供认证上下文。

#![allow(clippy::result_large_err)]

use axum::http::{HeaderMap, StatusCode};
use chrono::{Duration, Utc};
use std::sync::atomic::{AtomicI64, Ordering};

use crate::authentication::repo;
use crate::*;

use super::model::AdminAuthContext;

/// 本地会话不授予当前资格；HTTP中间件及广播前复核最终块人员绑定、岗位与辖区。
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
        // 节点归属只控制宿主初始化状态，不把其缓存操作者作为人员授权真源。
        if repo::get_active_node_binding_conn(conn)?.is_none() {
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

/// 本地就绪门不消费Passkey；完整HTTP入口先经中间件复核链上任职，处理函数保留原业务范围。
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

/// 每次受保护请求固定最终块，复核当前人员绑定、所选岗位及链上有效任职。
pub(crate) async fn validate_current_assignment(
    _state: &AppState,
    ctx: &AdminAuthContext,
) -> Result<(), axum::response::Response> {
    #[cfg(test)]
    if let Some(snapshot) = _state
        .db
        .role_snapshot
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
        return if ctx.account_id == snapshot.account_id
            && ctx.person_cid == snapshot.person_cid
            && ctx.binding_revision == snapshot.binding_revision
            && ctx.role_code == snapshot.role_code
            && ctx.institution_cid_number == snapshot.institution_cid_number
            && ctx.institution_code == snapshot.institution_code
            && ctx.scope_province_name == snapshot.scope_province_name
            && ctx.scope_city_name == snapshot.scope_city_name
            && ctx.scope_town_name == snapshot.scope_town_name
        {
            Ok(())
        } else {
            Err(api_error(
                StatusCode::UNAUTHORIZED,
                1002,
                "current person assignment changed",
            ))
        };
    }
    let result = async {
        let identity = crate::chain_runtime::identity_from_binding_parts(
            &ctx.institution_code,
            Some(&ctx.institution_cid_number),
        )?;
        let view = crate::chain_runtime::FinalizedChainView::connect().await?;
        let assignments = crate::institutions::fetch_active_assignments_at(
            &identity,
            &view,
            Some(&ctx.account_id),
        )
        .await?;
        let snapshot =
            crate::institutions::registration_snapshot_at(&ctx.institution_cid_number, &view)
                .await?;
        let scope = crate::chain_runtime::role_scope(
            &snapshot.institution.institution_code,
            &ctx.institution_cid_number,
            &ctx.role_code,
            &snapshot.institution.town_code,
        )?;
        Ok::<_, String>(
            assignments.iter().any(|a| ctx.matches_assignment(a))
                && scope
                    == (
                        ctx.scope_province_name.clone(),
                        ctx.scope_city_name.clone(),
                        ctx.scope_town_name.clone(),
                    ),
        )
    }
    .await;
    match result {
        Ok(true) => Ok(()),
        Ok(false) => Err(api_error(
            StatusCode::UNAUTHORIZED,
            1002,
            "current person assignment changed",
        )),
        Err(e) => {
            tracing::warn!(error=%e,"read current assignment failed");
            Err(api_error(
                StatusCode::SERVICE_UNAVAILABLE,
                1503,
                "current assignment unavailable",
            ))
        }
    }
}

/// 本地资料写入也要有当前岗位的对应业务发起权限；不因“已登录”授予写权。
pub(crate) async fn validate_selected_permission(
    _state: &AppState,
    ctx: &AdminAuthContext,
    module: Option<&[u8]>,
    action: Option<u32>,
) -> Result<(), axum::response::Response> {
    let allows = |p: &entitybase::RoleBusinessPermission<Vec<u8>, Vec<u8>, Vec<u8>>| {
        p.role_subject.cid_number == ctx.institution_cid_number.as_bytes()
            && p.role_subject.role_code == ctx.role_code.as_bytes()
            && p.operation == entitybase::RolePermissionOperation::Propose
            && module.is_none_or(|m| p.business_action_id.module_tag == m)
            && action.is_none_or(|a| p.business_action_id.action_code == a)
    };
    #[cfg(test)]
    if _state
        .db
        .role_snapshot
        .lock()
        .map_err(|_| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                5001,
                "test snapshot unavailable",
            )
        })?
        .is_some()
    {
        validate_current_assignment(_state, ctx).await?;
        return if _state
            .db
            .role_permissions
            .lock()
            .map_err(|_| {
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    5001,
                    "test permissions unavailable",
                )
            })?
            .iter()
            .any(allows)
        {
            Ok(())
        } else {
            Err(api_error(
                StatusCode::FORBIDDEN,
                1003,
                "当前岗位没有该业务发起权限",
            ))
        };
    }
    let result = async {
        let view = crate::chain_runtime::FinalizedChainView::connect().await?;
        let identity = crate::chain_runtime::identity_from_binding_parts(
            &ctx.institution_code,
            Some(&ctx.institution_cid_number),
        )?;
        let rows = crate::institutions::fetch_active_assignments_at(
            &identity,
            &view,
            Some(&ctx.account_id),
        )
        .await?;
        if !rows.iter().any(|a| ctx.matches_assignment(a)) {
            return Ok::<_, String>(false);
        }
        let snapshot =
            crate::institutions::registration_snapshot_at(&ctx.institution_cid_number, &view)
                .await?;
        let scope = crate::chain_runtime::role_scope(
            &snapshot.institution.institution_code,
            &ctx.institution_cid_number,
            &ctx.role_code,
            &snapshot.institution.town_code,
        )?;
        if scope
            != (
                ctx.scope_province_name.clone(),
                ctx.scope_city_name.clone(),
                ctx.scope_town_name.clone(),
            )
        {
            return Ok(false);
        }
        Ok(snapshot.permissions.iter().any(|(role, perms)| {
            role == ctx.role_code.as_bytes()
                && perms.iter().any(|p| {
                    allows(p)
                        && (module.is_some()
                            || p.business_action_id.module_tag == snapshot.module_tag())
                })
        }))
    }
    .await;
    match result {
        Ok(true) => Ok(()),
        Ok(false) => Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "当前岗位没有该业务发起权限",
        )),
        Err(e) => {
            tracing::warn!(error=%e,"read current role permission failed");
            Err(api_error(
                StatusCode::SERVICE_UNAVAILABLE,
                1503,
                "current role permission unavailable",
            ))
        }
    }
}
