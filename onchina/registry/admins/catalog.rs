//! 联邦注册局名册查询，保留原准确机构、任职、辖区和缓存规则。
pub(crate) type FederalAdminsCache = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, Vec<crate::registry::admins::model::FederalRegistryAdminRow>)>>>;
use axum::{extract::State, http::{HeaderMap, StatusCode}, response::IntoResponse, Json};
use chrono::Utc;
use std::collections::BTreeMap;
use crate::authentication::repo;
use crate::core::chain_runtime;
use crate::institutions::catalog::balance_fen;
use crate::*;
/// 原有名册展示缓存有效期，不作为授权真源。
const FEDERAL_ADMINS_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(30);
pub(crate) async fn list_federal_registry_admins(
    State(state): State<AppState>,
    axum::Extension(cache): axum::Extension<FederalAdminsCache>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if let Err(resp) = crate::authorization::rules::ensure_registry_admin(&ctx) {
        return resp;
    }
    let Some(province) = ctx
        .scope_province_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
    else {
        return api_error(StatusCode::FORBIDDEN, 1003, "admin province scope missing");
    };
    if chain_runtime::chain_province_code_by_name(&province).is_none() {
        let message = format!("province '{province}' is not a valid chain province");
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
    }
    // 命中短 TTL 缓存直接返回,跳过后续全量链读(全省任职扫描 + 批量余额),
    // 避免每次进 tab 都阻塞在链读上。锁不跨 await。
    {
        // 缓存锁即使被旧请求 poison 也只恢复当前内存缓存，不影响链上管理员授权真源。
        let cache = cache
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        if let Some((cached_at, rows)) = cache.get(&province) {
            if cached_at.elapsed() < FEDERAL_ADMINS_CACHE_TTL {
                return Json(ApiResponse {
                    code: 0,
                    message: "ok".to_string(),
                    data: rows.clone(),
                })
                .into_response();
            }
        }
    }
    let binding = match repo::active_node_binding(&state.db) {
        Ok(Some(binding)) => binding,
        Ok(None) => return api_error(StatusCode::FORBIDDEN, 2002, "not an on-chain admin"),
        Err(err) => {
            let message = format!("query node binding failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };
    let identity = match chain_runtime::identity_from_binding_parts(&binding.institution_code, Some(binding.institution_cid_number.as_str())) {
        Ok(identity) => identity,
        Err(err) => {
            let message = format!("node binding invalid: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };
    let province_groups =
        match crate::institutions::admins::chain_roles::fetch_all_federal_registry_assignments(
            &identity,
        )
        .await
        {
            Ok(v) => v,
            Err(err) => {
                tracing::warn!(error = %err, "chain unreachable listing all federal registry admins");
                return api_error(StatusCode::BAD_GATEWAY, 5002, "chain unreachable");
            }
        };
    let now = Utc::now();
    let tier1_code = chain_runtime::TIER1_REGISTRY_CODE.to_string();
    let balance_account_ids = province_groups
        .iter()
        .flat_map(|group| {
            group
                .assignments
                .iter()
                .map(|assignment| assignment.account_id.clone())
        })
        .collect::<Vec<_>>();
    let balance_by_account_id = match chain_runtime::fetch_account_balances_onchain(
        &balance_account_ids,
    )
    .await
    {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(error = %err, "chain balance unavailable listing federal registry admins");
            BTreeMap::new()
        }
    };
    let owner_cid = binding.institution_cid_number.clone();
    let result = state.db.with_client(move |conn| {
        let mut rows = Vec::with_capacity(balance_account_ids.len());
        for group in &province_groups {
            let province_name = group.province_name.clone();
            let _province_code = group.province_code;
            for assignment in &group.assignments {
                let account_id = &assignment.account_id;
                // 缓存缺失即按链上姓、名补一条 built_in 行，保证有本地 id 供换届定位。
                let admin = match repo::get_admin_by_account_id_conn(conn, account_id)? {
                    Some(mut admin) => {
                        admin.institution_cid_number = Some(owner_cid.clone());
                        admin.family_name = assignment.family_name.clone();
                        admin.given_name = assignment.given_name.clone();
                        repo::upsert_admin_conn(conn, &admin)?;
                        admin
                    }
                    None => {
                        let row = AdminUser {
                            id: repo::next_admin_id_conn(conn)?,
                            account_id: account_id.clone(),
                            family_name: assignment.family_name.clone(),
                            given_name: assignment.given_name.clone(),
                            institution_code: tier1_code.clone(),
                            institution_cid_number: Some(owner_cid.clone()),
                            built_in: true,
                            // 链上同步没有独立创建人；以该管理员自身账户作为可验证来源锚点。
                            creator_account_id: account_id.clone(),
                            created_at: now,
                            updated_at: None,
                            city_name: String::new(),
                        };
                        repo::upsert_admin_conn(conn, &row)?;
                        repo::get_admin_by_account_id_conn(conn, account_id)?
                            .ok_or_else(|| "federal admin cache backfill lost".to_string())?
                    }
                };
                rows.push(FederalRegistryAdminRow {
                    id: admin.id,
                    province_name: province_name.clone(),
                    account_id: admin.account_id,
                    family_name: assignment.family_name.clone(),
                    given_name: assignment.given_name.clone(),
                    role_code: assignment.role_code.clone(),
                    role_name: assignment.role_name.clone(),
                    term_required: assignment.term_required,
                    term_start: assignment.term_start,
                    term_end: assignment.term_end,
                    assignment_source: assignment.assignment_source,
                    assignment_source_label: assignment.assignment_source_label.clone(),
                    assignment_source_ref: assignment.assignment_source_ref.clone(),
                    balance_fen: balance_fen(&balance_by_account_id, account_id),
                    built_in: admin.built_in,
                    created_at: admin.created_at,
                    updated_at: admin.updated_at,
                });
            }
        }
        Ok(rows)
    });
    let rows = match result {
        Ok(v) => v,
        Err(err) => {
            let message = format!("query federal registry admins failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };
    // 回填短 TTL 缓存,窗口内重复打开直接命中,不再全量链读。锁不跨 await。
    {
        let mut cache = cache
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        cache.insert(province.clone(), (std::time::Instant::now(), rows.clone()));
    }
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: rows,
    })
    .into_response()
}
