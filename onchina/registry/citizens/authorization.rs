//! 公民业务自动授权：按当前机构、管理员、有效任职、业务权限和辖区判断。
//! 岗位只在后端确定并固定；公民回签和冷签提交只能复核原岗位，不能自动换岗。
#![allow(clippy::result_large_err)]

use super::{
    chain_identity::CitizenOnchainIdentityLevel,
    chain_identity::{ensure_record_in_admin_scope, IdentityOperations},
};
use axum::{http::StatusCode, response::Response};
use entitybase::{business_action, BusinessActionId};
use onchina::{
    api_error,
    authentication::login::AdminAuthContext,
    chain_identity::{
        read_finalized_citizen_identity, FinalizedCidStatus, FinalizedCitizenIdentity,
    },
    chain_runtime::identity_from_binding_parts,
    institutions::fetch_authorized_public_roles_at,
    AppState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CitizenOperation {
    OccupyCid,
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
                Self::OccupyCid => business_action::ACTION_OCCUPY_CID,
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

/// 所有候选均先通过同一机构、名册、有效任职、业务权限与辖区筛选。
/// 此处的frozen只允许来自后端操作记录或冷签会话，不接受客户端指定。
pub(crate) async fn resolve_operation_role_at(
    ctx: &AdminAuthContext,
    operation: CitizenOperation,
    block_hash: [u8; 32],
    scope: Option<(&[u8], &[u8])>,
    frozen: Option<&str>,
) -> Result<String, Response> {
    onchina::authorization::rules::ensure_registry_admin(ctx)?;
    let identity =
        identity_from_binding_parts(&ctx.institution_code, Some(&ctx.institution_cid_number))
            .map_err(chain_read_error)?;
    let roles = fetch_authorized_public_roles_at(
        &identity,
        &ctx.account_id,
        &operation.action(),
        block_hash,
    )
    .await
    .map_err(chain_read_error)?
    .into_iter()
    .filter(|assignment| {
        role_in_scope(
            identity.cid_number.as_bytes(),
            assignment.role_code.as_bytes(),
            scope,
        )
    })
    .map(|assignment| assignment.role_code)
    .collect::<Vec<_>>();
    choose_role(&roles, frozen)
}

fn chain_read_error(error: String) -> Response {
    tracing::error!(%error, "check citizen business authorization failed");
    api_error(StatusCode::BAD_GATEWAY, 2004, "业务权限核验失败，请重试")
}

fn choose_role(roles: &[String], frozen: Option<&str>) -> Result<String, Response> {
    if let Some(role) = frozen {
        return roles
            .iter()
            .find(|candidate| !candidate.is_empty() && candidate.as_str() == role)
            .cloned()
            .ok_or_else(|| {
                api_error(
                    StatusCode::FORBIDDEN,
                    1003,
                    "本次业务授权已失效，请重新发起",
                )
            });
    }
    // 多个岗位均满足完整授权时按稳定岗位码自动确定，不能依赖链读取返回顺序。
    roles
        .iter()
        .filter(|role| !role.is_empty())
        .min()
        .cloned()
        .ok_or_else(|| api_error(StatusCode::FORBIDDEN, 1003, "当前管理员无此业务办理权限"))
}

/// 身份等级只允许首次登记或投票升级竞选；其它业务不受本控件等级锁定影响。
fn ensure_identity_transition(
    operation: CitizenOperation,
    voting: bool,
    candidate: bool,
) -> Result<(), Response> {
    match operation {
        CitizenOperation::RegisterVotingIdentity if voting || candidate => Err(api_error(
            StatusCode::CONFLICT,
            1005,
            "链上身份已存在，不能重复登记投票身份",
        )),
        CitizenOperation::UpgradeCandidateIdentity if candidate => Err(api_error(
            StatusCode::CONFLICT,
            1005,
            "已是竞选身份，不能再次更换或签名",
        )),
        _ => Ok(()),
    }
}

pub(crate) async fn resolve_operation_role(
    state: &AppState,
    ctx: &AdminAuthContext,
    cid: &str,
    operation: CitizenOperation,
    frozen: Option<&str>,
) -> Result<(String, FinalizedCitizenIdentity), Response> {
    onchina::authorization::rules::ensure_registry_admin(ctx)?;
    onchina::codes::validate_cid_number_format(cid)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, 1001, "公民CID号格式错误"))?;
    let snapshot = read_finalized_citizen_identity(cid)
        .await
        .map_err(chain_read_error)?;
    if operation == CitizenOperation::OccupyCid {
        if !snapshot.is_unoccupied() {
            return Err(api_error(
                StatusCode::CONFLICT,
                1005,
                "CID链上状态已变化，请重新发起占号",
            ));
        }
    } else if snapshot.cid_status != FinalizedCidStatus::Active
        || snapshot.active_binding().is_none()
    {
        return Err(api_error(
            StatusCode::CONFLICT,
            1005,
            "公民CID号未有效绑定或已吊销，不能办理",
        ));
    }
    // prepare、complete和首次实际提交均复用该快照，不增加额外RPC；finalized恢复沿用原路径。
    ensure_identity_transition(
        operation,
        snapshot.voting.is_some(),
        snapshot.candidate.is_some(),
    )?;
    let scope = match operation {
        CitizenOperation::RegisterVotingIdentity | CitizenOperation::UpgradeCandidateIdentity => {
            let record = state
                .db
                .find_citizen_by_cid(cid)
                .map_err(|error| {
                    tracing::error!(%error, "read citizen authorization scope failed");
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
        CitizenOperation::OccupyCid => None,
    };
    let role = resolve_operation_role_at(
        ctx,
        operation,
        snapshot.finalized_block_hash,
        scope
            .as_ref()
            .map(|(province, city)| (province.as_slice(), city.as_slice())),
        frozen,
    )
    .await?;
    Ok((role, snapshot))
}

/// 只复查尚未提交的业务，已 finalized 的投影恢复不依赖当前岗位权限。
pub(crate) async fn recheck_submit(
    state: &AppState,
    ctx: &AdminAuthContext,
    session: &onchina::transactions::session::ChainSignSession,
) -> Result<(), Response> {
    use super::occupy::{
        PURPOSE_CITIZEN_ADMIN_REBIND, PURPOSE_CITIZEN_IDENTITY_PUSH, PURPOSE_CITIZEN_OCCUPY,
        PURPOSE_CITIZEN_REVOKE,
    };
    let operation = match session.purpose.as_str() {
        PURPOSE_CITIZEN_OCCUPY => CitizenOperation::OccupyCid,
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
            "签名会话授权信息缺失",
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
