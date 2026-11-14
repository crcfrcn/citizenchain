//! 当前市注册局详情的链上名册；人员无任职也保留，钱包与岗位锚点分开表达。
use crate::*;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use onchina::institutions::admins::chain_roles::{fetch_governance_state, GovernanceState};

pub(crate) fn chain_registry_rows(
    cid: &str,
    city: &str,
    snapshot: &GovernanceState,
) -> Result<Vec<CityRegistryAdminRow>, String> {
    if snapshot.cid_number != cid {
        return Err("链上名册目标不匹配".into());
    }
    Ok(snapshot
        .members
        .iter()
        .map(|member| {
            let names = snapshot
                .roles
                .iter()
                .filter(|role| {
                    role.active
                        && role
                            .assignments
                            .iter()
                            .any(|a| a.account_id == member.account_id && a.effective)
                })
                .map(|role| role.role_name.as_str())
                .collect::<Vec<_>>()
                .join("、");
            CityRegistryAdminRow {
                id: format!("{cid}:{}", member.account_id),
                institution_cid_number: cid.into(),
                roster_account_id: member.account_id.clone(),
                citizen_cid_number: member.cid_number.clone(),
                role_name: names,
                account_id: member.signer_id.clone(),
                family_name: member.family_name.clone(),
                given_name: member.given_name.clone(),
                balance_fen: None,
                institution_code: "CREG".into(),
                creator_account_id: String::new(),
                creator_family_name: String::new(),
                creator_given_name: String::new(),
                created_at: None,
                city_name: city.into(),
            }
        })
        .collect())
}

pub(crate) async fn list_city_registry_admins(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(ctx) => ctx,
        Err(response) => return response,
    };
    if let Err(response) = onchina::authorization::rules::ensure_registry_admin(&ctx) {
        return response;
    }
    let cid = query.cid_number.trim();
    let (_, city) = match super::repo::registry_target_scope(&ctx, cid) {
        Ok(scope) => scope,
        Err(error) => return api_error(StatusCode::FORBIDDEN, 1003, &error),
    };
    let identity = match onchina::chain_runtime::identity_from_binding_parts("CREG", Some(cid)) {
        Ok(identity) => identity,
        Err(error) => return api_error(StatusCode::BAD_REQUEST, 1001, &error),
    };
    // 真源读取失败明确报错，不回退本地表或把失败伪装为零人。
    let snapshot = match fetch_governance_state(&identity).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            tracing::error!(%error, "read registry chain roster failed");
            return api_error(StatusCode::BAD_GATEWAY, 5002, "链上管理员读取失败，请重试");
        }
    };
    let rows = match chain_registry_rows(cid, &city, &snapshot) {
        Ok(rows) => rows,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, 5002, &error),
    };
    let total = rows.len();
    let limit = query.limit.unwrap_or(1989).clamp(1, 1989);
    let offset = query.offset.unwrap_or(0);
    let mut rows = rows
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let accounts = rows
        .iter()
        .map(|r| r.account_id.clone())
        .collect::<Vec<_>>();
    // 余额只作展示，失败不改变已经确认的名册和岗位。
    if let Ok(balances) = onchina::chain_runtime::fetch_account_balances_onchain(&accounts).await {
        for row in &mut rows {
            row.balance_fen = balances.get(&row.account_id).cloned().flatten();
        }
    }
    Json(ApiResponse {
        code: 0,
        message: "ok".into(),
        data: CityRegistryAdminListOutput {
            total,
            limit,
            offset,
            rows,
            cid_number: cid.into(),
            finalized_block_hash: snapshot.block_hash,
            finalized_block_number: snapshot.block_number,
        },
    })
    .into_response()
}
