//! 登录会话鉴权守卫。
//!
//! 会话与管理员身份只读取结构化表。
//! 业务模块通过 `require_admin_any` 获取认证上下文;
//! 写操作的冷钱包扫码签名(PasskeyColdSign 档)由 admins::actions 的安全 grant 单独校验。

// 登录守卫直接携带统一拒绝响应，保证所有调用方复用完全相同的状态码和错误体。
#![allow(clippy::result_large_err)]

use axum::http::{HeaderMap, StatusCode};
use chrono::{Duration, Utc};
use std::sync::atomic::{AtomicI64, Ordering};

use crate::auth::repo;
use crate::*;

use super::model::AdminAuthContext;
use super::signature::admin_person_names;

pub(super) fn admin_auth(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminAuthContext, axum::response::Response> {
    let Some(token) = bearer_token(headers) else {
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            1002,
            "admin auth required",
        ));
    };

    let now = Utc::now();
    let city_idle_timeout_minutes = std::env::var("ONCHINA_ADMIN_IDLE_TIMEOUT_MINUTES")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(10);

    // 清理动作节流到 60 秒一次,避免高频鉴权请求都执行批量删除。
    static LAST_CLEANUP: AtomicI64 = AtomicI64::new(0);
    let now_ts = now.timestamp();
    let last = LAST_CLEANUP.load(Ordering::Relaxed);
    let should_cleanup = now_ts - last > 60
        && LAST_CLEANUP
            .compare_exchange(last, now_ts, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok();

    let result = state.db.with_client(move |conn| {
        if should_cleanup {
            repo::cleanup_admin_sessions_conn(conn, now, city_idle_timeout_minutes)?;
        }
        let Some(mut session) = repo::get_admin_session_conn(conn, token.as_str())? else {
            return Err("http:unauthorized:invalid access token".to_string());
        };

        let idle_expired =
            crate::core::chain_runtime::is_subordinate_registry(&session.institution_code)
                && now > session.last_active_at + Duration::minutes(city_idle_timeout_minutes);
        if now > session.expire_at || idle_expired {
            conn.execute("DELETE FROM admin_sessions WHERE token = $1", &[&token])
                .map_err(|e| format!("delete expired admin session failed: {e}"))?;
            return Err("http:unauthorized:access token expired".to_string());
        }

        let Some(binding) = repo::get_active_node_binding_conn(conn)? else {
            conn.execute("DELETE FROM admin_sessions WHERE token = $1", &[&token])
                .map_err(|e| format!("delete unbound admin session failed: {e}"))?;
            return Err("http:unauthorized:invalid access token".to_string());
        };
        // 会话必须锁定到签发时的链上机构候选；解绑、重绑或机构切换后对应会话立即失效。
        if session.candidate_id != binding.candidate_id
            || session.institution_code != binding.institution_code
        {
            conn.execute("DELETE FROM admin_sessions WHERE token = $1", &[&token])
                .map_err(|e| format!("delete binding-mismatched admin session failed: {e}"))?;
            return Err("http:unauthorized:invalid access token".to_string());
        }

        let admin = repo::get_admin_by_account_id_conn(conn, &session.account_id)?
            .ok_or_else(|| "http:forbidden:admin not found".to_string())?;
        if admin.institution_code != session.institution_code {
            conn.execute("DELETE FROM admin_sessions WHERE token = $1", &[&token])
                .map_err(|e| format!("delete institution-mismatched admin session failed: {e}"))?;
            return Err("http:unauthorized:invalid access token".to_string());
        }
        session.last_active_at = now;
        repo::touch_admin_session_conn(conn, &session)?;
        let institution_code = admin.institution_code.clone();
        let is_frg = crate::core::chain_runtime::is_tier1_registry(&institution_code);
        let admin_level = crate::core::chain_runtime::admin_level_label_for(&institution_code);

        // 省/市/镇作用域:与登录签发(onchain_gate)共用 derive_admin_scope_conn 单一来源,口径一致。
        let (scope_province_name, scope_city_name, scope_town_name) =
            repo::derive_admin_scope_conn(conn, &admin.account_id, &admin.institution_code)?;

        // 全国级机构(NATIONAL,联邦注册局除外)无省维度;其余(含 FRG)必须有省作用域。
        let national_no_province = !is_frg && admin_level.as_deref() == Some("NATIONAL");
        if !national_no_province
            && scope_province_name
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
        {
            return Err("http:forbidden:admin province scope missing".to_string());
        }

        let (family_name, given_name) = admin_person_names(&admin);
        let cid_short_name = repo::resolve_home_cid_short_name_conn(
            conn,
            &admin.institution_code,
            scope_province_name.as_deref(),
            scope_city_name.as_deref(),
        )?;
        Ok(AdminAuthContext {
            account_id: admin.account_id,
            institution_cid_number: binding.institution_cid_number,
            institution_code,
            admin_level,
            family_name,
            given_name,
            scope_province_name,
            scope_city_name,
            scope_town_name,
            cid_short_name,
        })
    });

    match result {
        Ok(ctx) => Ok(ctx),
        Err(err) if err == "http:unauthorized:invalid access token" => Err(api_error(
            StatusCode::UNAUTHORIZED,
            1002,
            "invalid access token",
        )),
        Err(err) if err == "http:unauthorized:access token expired" => Err(api_error(
            StatusCode::UNAUTHORIZED,
            1002,
            "access token expired",
        )),
        Err(err) if err == "http:forbidden:admin not found" => {
            Err(api_error(StatusCode::FORBIDDEN, 2002, "admin not found"))
        }
        Err(err) if err == "http:forbidden:admin province scope missing" => Err(api_error(
            StatusCode::FORBIDDEN,
            2002,
            "admin province scope missing",
        )),
        Err(err) => {
            let message = format!("admin auth failed: {err}");
            Err(api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                5001,
                message.as_str(),
            ))
        }
    }
}

pub(crate) fn require_admin_any(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminAuthContext, axum::response::Response> {
    admin_auth(state, headers)
}

pub(super) fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let auth = headers.get("authorization")?.to_str().ok()?.trim();
    let token = auth.strip_prefix("Bearer ")?;
    if token.trim().is_empty() {
        return None;
    }
    Some(token.trim().to_string())
}
