//! 公民详情业务的岗位候选、自动确定与签名前复查；岗位码只在系统内部传递。
#![allow(clippy::result_large_err)]

use super::{
    chain_identity::CitizenOnchainIdentityLevel,
    chain_identity::{ensure_record_in_admin_scope, IdentityOperations},
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use entitybase::{business_action, BusinessActionId};
use onchina::{
    api_error,
    authentication::login::AdminAuthContext,
    chain_identity::{
        read_finalized_citizen_identity, FinalizedCidStatus, FinalizedCitizenIdentity,
    },
    chain_runtime::identity_from_binding_parts,
    institutions::admins::chain_roles::fetch_authorized_public_roles_at,
    ApiResponse, AppState,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CitizenOperation {
    RegisterVotingIdentity,
    UpgradeCandidateIdentity,
    RebindWallet,
    RevokeIdentity,
}

impl CitizenOperation {
    pub(crate) fn for_level(level: CitizenOnchainIdentityLevel) -> Self {
        match level {
            CitizenOnchainIdentityLevel::Voting => Self::RegisterVotingIdentity,
            CitizenOnchainIdentityLevel::Candidate => Self::UpgradeCandidateIdentity,
        }
    }

    fn action(self) -> BusinessActionId<Vec<u8>> {
        BusinessActionId {
            module_tag: business_action::MODULE_CITIZEN_IDENTITY.to_vec(),
            action_code: match self {
                Self::RegisterVotingIdentity => business_action::ACTION_REGISTER_VOTING_IDENTITY,
                Self::UpgradeCandidateIdentity => {
                    business_action::ACTION_UPGRADE_CANDIDATE_IDENTITY
                }
                Self::RebindWallet => business_action::ACTION_ADMIN_REBIND_CID_ACCOUNT_ID,
                Self::RevokeIdentity => business_action::ACTION_REVOKE_CID,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OperationRole {
    pub(crate) role_code: String,
    pub(crate) role_name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OperationRolesOutput {
    pub(crate) roles: Vec<OperationRole>,
    finalized_block_hash: String,
    finalized_block_number: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OperationRolesQuery {
    operation: CitizenOperation,
}

fn role_in_scope(actor_cid: &[u8], role_code: &[u8], scope: Option<(&[u8], &[u8])>) -> bool {
    let Some((province, city)) = scope else {
        return true;
    };
    if province.is_empty() || city.is_empty() {
        return false;
    }
    let Ok(parts) = primitives::cid::number::parse_cid_number_parts_bytes(actor_cid) else {
        return false;
    };
    if parts.institution == primitives::cid::code::FRG {
        let Ok(province) = <[u8; 2]>::try_from(province) else {
            return false;
        };
        return role_code
            == primitives::governance_skeleton::province_commissioner_role_code(province);
    }
    if parts.institution != *b"CREG" {
        return false;
    }
    primitives::cid::number::cid_scope_codes(actor_cid).is_ok_and(|(actor_province, actor_city)| {
        actor_province.as_ref() == province && actor_city.as_ref() == city
    })
}

async fn load_operation_roles(
    state: &AppState,
    ctx: &AdminAuthContext,
    cid: &str,
    operation: CitizenOperation,
) -> Result<(OperationRolesOutput, FinalizedCitizenIdentity), Response> {
    onchina::authorization::rules::ensure_registry_admin(ctx)?;
    onchina::codes::validate_cid_number_format(cid)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, 1001, "公民CID号格式错误"))?;
    let snapshot = read_finalized_citizen_identity(cid)
        .await
        .map_err(chain_read_error)?;
    if snapshot.cid_status != FinalizedCidStatus::Active || snapshot.active_binding().is_none() {
        return Err(api_error(
            StatusCode::CONFLICT,
            1005,
            "公民CID号未有效绑定或已吊销，不能办理",
        ));
    }
    let scope = match operation {
        CitizenOperation::RegisterVotingIdentity | CitizenOperation::UpgradeCandidateIdentity => {
            let record = state
                .db
                .find_citizen_by_cid(cid)
                .map_err(|error| {
                    tracing::error!(%error, "read citizen role scope failed");
                    api_error(StatusCode::INTERNAL_SERVER_ERROR, 1004, "公民档案读取失败")
                })?
                .ok_or_else(|| api_error(StatusCode::NOT_FOUND, 1004, "公民档案不存在"))?;
            ensure_record_in_admin_scope(ctx, &record)?;
            Some((
                record.province_code.into_bytes(),
                record.city_code.into_bytes(),
            ))
        }
        CitizenOperation::RebindWallet | CitizenOperation::RevokeIdentity => {
            snapshot.voting.as_ref().map(|identity| {
                (
                    identity.residence_province_code.clone(),
                    identity.residence_city_code.clone(),
                )
            })
        }
    };
    let identity =
        identity_from_binding_parts(&ctx.institution_code, Some(&ctx.institution_cid_number))
            .map_err(chain_read_error)?;
    let assignments = fetch_authorized_public_roles_at(
        &identity,
        &ctx.account_id,
        &operation.action(),
        snapshot.finalized_block_hash,
    )
    .await
    .map_err(chain_read_error)?;
    let roles = assignments
        .into_iter()
        .filter(|assignment| {
            role_in_scope(
                identity.cid_number.as_bytes(),
                assignment.role_code.as_bytes(),
                scope
                    .as_ref()
                    .map(|(province, city)| (province.as_slice(), city.as_slice())),
            )
        })
        .map(|assignment| OperationRole {
            role_code: assignment.role_code,
            role_name: assignment.role_name,
        })
        .collect();
    Ok((
        OperationRolesOutput {
            roles,
            finalized_block_hash: format!("0x{}", hex::encode(snapshot.finalized_block_hash)),
            finalized_block_number: snapshot.finalized_block_number,
        },
        snapshot,
    ))
}

fn chain_read_error(error: String) -> Response {
    tracing::error!(%error, "read citizen operation roles failed");
    api_error(StatusCode::BAD_GATEWAY, 2004, "办理岗位加载失败，请重试")
}

fn choose_role(roles: &[OperationRole], selected: Option<&str>) -> Result<String, Response> {
    if roles.is_empty() {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "当前管理员没有办理此业务的有效岗位权限",
        ));
    }
    if let Some(selected) = selected {
        return roles
            .iter()
            .find(|role| role.role_code == selected)
            .map(|role| role.role_code.clone())
            .ok_or_else(|| {
                api_error(
                    StatusCode::FORBIDDEN,
                    1003,
                    "所选办理岗位已失效或无此业务权限，请刷新后重试",
                )
            });
    }
    if roles.len() == 1 {
        return Ok(roles[0].role_code.clone());
    }
    Err(api_error(StatusCode::BAD_REQUEST, 1001, "请选择办理岗位"))
}

pub(crate) async fn resolve_operation_role(
    state: &AppState,
    ctx: &AdminAuthContext,
    cid: &str,
    operation: CitizenOperation,
    selected: Option<&str>,
) -> Result<(String, FinalizedCitizenIdentity), Response> {
    let (options, snapshot) = load_operation_roles(state, ctx, cid, operation).await?;
    Ok((choose_role(&options.roles, selected)?, snapshot))
}

pub(crate) async fn operation_roles(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(cid): Path<String>,
    Query(query): Query<OperationRolesQuery>,
) -> Response {
    let ctx = match onchina::require_admin_any(&state, &headers) {
        Ok(ctx) => ctx,
        Err(response) => return response,
    };
    match load_operation_roles(&state, &ctx, &cid, query.operation).await {
        Ok((options, _)) => Json(ApiResponse {
            code: 0,
            message: "ok".to_string(),
            data: options,
        })
        .into_response(),
        Err(response) => response,
    }
}

/// 只复查尚未提交的业务，已 finalized 的投影恢复不依赖当前岗位权限。
pub(crate) async fn recheck_submit(
    state: &AppState,
    ctx: &AdminAuthContext,
    session: &onchina::transactions::session::ChainSignSession,
) -> Result<(), Response> {
    use super::occupy::{
        PURPOSE_CITIZEN_ADMIN_REBIND, PURPOSE_CITIZEN_IDENTITY_PUSH, PURPOSE_CITIZEN_REVOKE,
    };
    let operation = match session.purpose.as_str() {
        PURPOSE_CITIZEN_ADMIN_REBIND => CitizenOperation::RebindWallet,
        PURPOSE_CITIZEN_REVOKE => CitizenOperation::RevokeIdentity,
        PURPOSE_CITIZEN_IDENTITY_PUSH => match session
            .context
            .get("identity_level")
            .and_then(|v| v.as_str())
        {
            Some("voting") => CitizenOperation::RegisterVotingIdentity,
            Some("candidate") => CitizenOperation::UpgradeCandidateIdentity,
            _ => {
                return Err(api_error(
                    StatusCode::CONFLICT,
                    1005,
                    "签名会话身份类型缺失",
                ))
            }
        },
        _ => return onchina::authorization::rules::ensure_registry_admin(ctx),
    };
    let cid = session.context.get("cid_number").and_then(|v| v.as_str());
    let role = session
        .context
        .get("actor_role_code")
        .and_then(|v| v.as_str());
    if session
        .context
        .get("origin_institution_cid")
        .and_then(|v| v.as_str())
        != Some(ctx.institution_cid_number.as_str())
    {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "签名会话不属于当前机构",
        ));
    }
    let (Some(cid), Some(role)) = (cid, role) else {
        return Err(api_error(
            StatusCode::CONFLICT,
            1005,
            "签名会话办理岗位缺失",
        ));
    };
    resolve_operation_role(state, ctx, cid, operation, Some(role))
        .await
        .map(|_| ())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/citizens_authorization.rs"]
mod tests;
