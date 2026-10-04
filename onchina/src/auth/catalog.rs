use axum::{extract::State, http::HeaderMap, http::StatusCode, response::IntoResponse, Json};
use chrono::Utc;
use std::collections::BTreeMap;

use crate::auth::repo;
use crate::core::chain_runtime;
use crate::crypto::pubkey::same_account_id;
use crate::institution::subjects::http::resolve_created_by;
use crate::institution::subjects::model::InstitutionDetailOutput;
use crate::*;

/// 联邦注册局管理员列表链读结果缓存有效期。管理员任免变更后至多延迟该窗口可见。
const FEDERAL_ADMINS_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(30);

fn balance_lookup_key(account_id: &str) -> String {
    crate::crypto::pubkey::normalize_account_id(account_id).unwrap_or_default()
}

fn balance_fen(balances: &BTreeMap<String, Option<String>>, account_id: &str) -> Option<String> {
    balances
        .get(balance_lookup_key(account_id).as_str())
        .cloned()
        .flatten()
}

/// Tier1 创世注册局管理员列表(全量省级组,「全走链读」决策③)。
///
/// 权威管理员集合在 FRG 唯一 `AdminAccounts`，43 省边界来自 entity 省专员岗位；
/// 本地只回填显示和换届定位所需的管理员元数据；成员资格与省作用域始终来自链上。
pub(crate) async fn list_federal_registry_admins(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
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
        let cache = state
            .federal_admins_cache
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
    let identity = match chain_runtime::identity_from_binding_parts(
        &binding.institution_code,
        Some(binding.institution_cid_number.as_str()),
        None,
    ) {
        Ok(identity) => identity,
        Err(err) => {
            let message = format!("node binding invalid: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };
    let province_groups =
        match crate::institution::admins::chain_roles::fetch_all_federal_registry_assignments(
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
        let mut cache = state
            .federal_admins_cache
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

/// 普通机构只读“本机构管理员”列表。
///
/// 权威集合仍来自链上 Active 管理员集合;本地 admins 表只补展示姓名,不能决定准入。
pub(crate) async fn list_own_institution_admins(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let capabilities = crate::platform::capability::capabilities_for(&ctx.institution_code);
    if !capabilities.can_view_own_admins {
        return api_error(StatusCode::FORBIDDEN, 1003, "permission denied");
    }
    let binding = match repo::active_node_binding(&state.db) {
        Ok(Some(binding)) => binding,
        Ok(None) => return api_error(StatusCode::FORBIDDEN, 2002, "not an on-chain admin"),
        Err(err) => {
            let message = format!("query node binding failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };
    if binding.institution_code != ctx.institution_code {
        return api_error(StatusCode::FORBIDDEN, 1003, "permission denied");
    }
    let identity = match chain_runtime::identity_from_binding_parts(
        &binding.institution_code,
        Some(binding.institution_cid_number.as_str()),
        binding.frg_province_code.as_deref(),
    ) {
        Ok(identity) => identity,
        Err(err) => {
            let message = format!("node binding invalid: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };
    let chain_assignments =
        match crate::institution::admins::chain_roles::fetch_active_assignments_onchain(&identity)
            .await
        {
            Ok(Some(assignments)) => assignments,
            Ok(None) => return api_error(StatusCode::FORBIDDEN, 2002, "not an on-chain admin"),
            Err(err) => {
                tracing::warn!(error = %err, "chain unreachable listing own institution admins");
                return api_error(StatusCode::BAD_GATEWAY, 5002, "chain unreachable");
            }
        };
    // 列表展示也做一次链上 active 复查,避免后台清退窗口内的失效管理员继续读取。
    if !chain_assignments
        .iter()
        .any(|assignment| same_account_id(assignment.account_id.as_str(), ctx.account_id.as_str()))
    {
        return api_error(StatusCode::FORBIDDEN, 2002, "not an on-chain admin");
    }
    let actor_account_id = ctx.account_id.clone();
    let institution_code = ctx.institution_code.clone();
    let cid_short_name = ctx.cid_short_name.clone();
    let balance_account_ids = chain_assignments
        .iter()
        .map(|assignment| assignment.account_id.clone())
        .collect::<Vec<_>>();
    let balance_by_account_id = match chain_runtime::fetch_account_balances_onchain(
        &balance_account_ids,
    )
    .await
    {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(error = %err, "chain balance unavailable listing own institution admins");
            BTreeMap::new()
        }
    };
    let result = {
        let mut rows = Vec::with_capacity(chain_assignments.len());
        for assignment in chain_assignments {
            let is_self =
                same_account_id(assignment.account_id.as_str(), actor_account_id.as_str());
            let balance = balance_fen(&balance_by_account_id, assignment.account_id.as_str());
            rows.push(OwnInstitutionAdminRow {
                account_id: assignment.account_id,
                family_name: assignment.family_name,
                given_name: assignment.given_name,
                role_code: assignment.role_code,
                role_name: assignment.role_name,
                term_required: assignment.term_required,
                term_start: assignment.term_start,
                term_end: assignment.term_end,
                assignment_source: assignment.assignment_source,
                assignment_source_label: assignment.assignment_source_label,
                assignment_source_ref: assignment.assignment_source_ref,
                balance_fen: balance,
                is_self,
            });
        }
        Ok::<_, String>(OwnInstitutionAdminListOutput {
            institution_code,
            cid_short_name,
            rows,
        })
    };
    let data = match result {
        Ok(v) => v,
        Err(err) => {
            let message = format!("query own institution admins failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data,
    })
    .into_response()
}

/// 当前登录机构详情。
///
/// 只读取本节点 active binding 对应的机构 CID,不允许前端传 cid_number,避免把“本机构信息”
/// 变成任意机构详情读取入口。管理员资格仍由登录守卫和链上 active admins 决定。
pub(crate) async fn get_own_institution(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let binding = match repo::active_node_binding(&state.db) {
        Ok(Some(binding)) => binding,
        Ok(None) => return api_error(StatusCode::FORBIDDEN, 2002, "not an on-chain admin"),
        Err(err) => {
            let message = format!("query node binding failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    };
    if binding.institution_code != ctx.institution_code {
        return api_error(StatusCode::FORBIDDEN, 1003, "permission denied");
    }
    let cid_number = binding.institution_cid_number.trim().to_string();
    if cid_number.is_empty() {
        return api_error(
            StatusCode::CONFLICT,
            1007,
            "binding institution_cid_number is required",
        );
    }
    let Some((inst, accounts)) = (match state.db.get_institution_with_accounts(cid_number.as_str())
    {
        Ok(v) => v,
        Err(err) => {
            let message = format!("query own institution failed: {err}");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, message.as_str());
        }
    }) else {
        return api_error(StatusCode::NOT_FOUND, 1004, "institution not found");
    };
    let (creator_family_name, creator_given_name, creator_institution_code) =
        resolve_created_by(&state, inst.creator_account_id.as_deref().unwrap_or(""));
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: InstitutionDetailOutput {
            institution: inst,
            accounts,
            creator_family_name,
            creator_given_name,
            creator_institution_code,
        },
    })
    .into_response()
}
