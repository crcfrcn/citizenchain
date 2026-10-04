//! 机构管理员链下私密资料子模块。
//!
//! 管理员链上人员记录使用账户、姓、名；授权只比较账户，岗位、任期和来源由 entity 表达。
//! 本子模块只承接链下私密档案(部门/联系方式/证件照/passkey 绑定)与链投影,
//! 落库到 `institution_admins` 省级分区表。控制台登录元数据走独立的 `admins` 表,与此无关。

// 机构管理员校验辅助函数直接返回统一 Axum Response，错误路径不另造可分叉的包装类型。
#![allow(clippy::result_large_err)]

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::{Duration, Utc};
use codec::Encode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    actor_ip_from_headers, api_error,
    auth::{login::parse_account_id_bytes, repo as auth_repo},
    core::{
        chain_submit,
        institution_call::{
            encode_propose_institution_governance, encode_register_institution_admins,
            InstitutionAdminsPayload, ProposeInstitutionGovernanceArgs,
            RegisterInstitutionAdminsArgs,
        },
    },
    domains::citizens::{
        chain_identity::ensure_registry_admin,
        occupy::{ChainSignSession, SESSION_TTL_SECS},
    },
    require_admin_any, ApiResponse, AppState,
};

pub(crate) mod chain_roles;

pub(crate) const PURPOSE_INSTITUTION_GOVERNANCE: &str = "INSTITUTION_GOVERNANCE";
pub(crate) const PURPOSE_INSTITUTION_REGISTER_ADMINS: &str = "INSTITUTION_REGISTER_ADMINS";

#[derive(Debug, Deserialize)]
pub(crate) struct InstitutionAdminInput {
    pub(crate) account_id: String,
    #[serde(default)]
    pub(crate) cid_number: Option<String>,
    #[serde(default)]
    pub(crate) family_name: Option<String>,
    #[serde(default)]
    pub(crate) given_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct InstitutionRolePermissionInput {
    pub(crate) module_tag: String,
    pub(crate) action_code: u32,
    pub(crate) operation: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct InstitutionRoleMutationInput {
    pub(crate) mutation: String,
    #[serde(default)]
    pub(crate) role_code: Option<String>,
    #[serde(default)]
    pub(crate) role_name: Option<String>,
    #[serde(default)]
    pub(crate) term_required: bool,
    #[serde(default)]
    pub(crate) permissions: Vec<InstitutionRolePermissionInput>,
    #[serde(default)]
    pub(crate) assignments: Vec<InstitutionAssignmentTargetInput>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct InstitutionAssignmentTargetInput {
    pub(crate) account_id: String,
    #[serde(default)]
    pub(crate) term_start: u32,
    #[serde(default)]
    pub(crate) term_end: u32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct InstitutionAssignmentChangeInput {
    pub(crate) role_code: String,
    #[serde(default)]
    pub(crate) assignments: Vec<InstitutionAssignmentTargetInput>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PrepareInstitutionGovernanceInput {
    pub(crate) cid_number: String,
    pub(crate) proposer_role_code: String,
    #[serde(default)]
    pub(crate) admins: Vec<InstitutionAdminInput>,
    #[serde(default)]
    pub(crate) role_mutations: Vec<InstitutionRoleMutationInput>,
    #[serde(default)]
    pub(crate) assignment_changes: Vec<InstitutionAssignmentChangeInput>,
    #[serde(default)]
    pub(crate) legal_representative_cid_number: Option<String>,
    #[serde(default)]
    pub(crate) clear_legal_representative: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PrepareRegisterInstitutionAdminsInput {
    pub(crate) cid_number: String,
    pub(crate) admins: Vec<InstitutionAdminInput>,
}

#[derive(Debug, Serialize)]
pub(crate) struct PrepareInstitutionChainOutput {
    pub(crate) request_id: String,
    pub(crate) cid_number: String,
    pub(crate) chain_action: u16,
    pub(crate) call_data_hex: String,
    pub(crate) sign_request: String,
    pub(crate) expires_at: i64,
}

pub(crate) fn code_bytes(institution_code: &str) -> Result<[u8; 4], axum::response::Response> {
    let raw = institution_code.trim().as_bytes();
    if raw.is_empty() || raw.len() > 4 {
        return Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            5001,
            "institution_code invalid",
        ));
    }
    let mut out = [0u8; 4];
    out[..raw.len()].copy_from_slice(raw);
    Ok(out)
}

fn parse_admin_inputs(
    state: &AppState,
    admins: &[InstitutionAdminInput],
    is_public: bool,
) -> Result<InstitutionAdminsPayload, axum::response::Response> {
    let mut public_admins = Vec::with_capacity(admins.len());
    let mut private_admins = Vec::with_capacity(admins.len());
    let mut seen = std::collections::BTreeSet::new();
    for admin in admins {
        let account_id = parse_account_id_bytes(admin.account_id.trim()).ok_or_else(|| {
            api_error(
                StatusCode::BAD_REQUEST,
                1001,
                "account_id 必须是小写 0x 加 64 位十六进制",
            )
        })?;
        if !seen.insert(account_id) {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                1001,
                "管理员账户不能重复",
            ));
        }
        let citizen = state
            .db
            .find_citizen_by_account_id(admin.account_id.trim())
            .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, &err))?;
        let supplied_cid = admin
            .cid_number
            .as_deref()
            .map(str::trim)
            .filter(|cid| !cid.is_empty());
        let citizen_cid = supplied_cid
            .map(str::to_string)
            .or_else(|| {
                citizen
                    .as_ref()
                    .map(|record| record.cid_number.trim().to_string())
            })
            .filter(|cid| !cid.is_empty());
        let family_name = admin
            .family_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .or_else(|| {
                citizen
                    .as_ref()
                    .map(|record| record.family_name.trim())
                    .filter(|name| !name.is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| {
                if is_public {
                    String::new()
                } else {
                    "管理".to_string()
                }
            });
        let given_name = admin
            .given_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .or_else(|| {
                citizen
                    .as_ref()
                    .map(|record| record.given_name.trim())
                    .filter(|name| !name.is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| {
                if is_public {
                    String::new()
                } else {
                    "员".to_string()
                }
            });
        let family_name: admin_primitives::FamilyName = family_name
            .into_bytes()
            .try_into()
            .map_err(|_| api_error(StatusCode::BAD_REQUEST, 1001, "管理员姓过长"))?;
        let given_name: admin_primitives::GivenName = given_name
            .into_bytes()
            .try_into()
            .map_err(|_| api_error(StatusCode::BAD_REQUEST, 1001, "管理员名过长"))?;
        if is_public {
            let cid_number: admin_primitives::AdminCidNumber = citizen_cid
                .unwrap_or_default()
                .into_bytes()
                .try_into()
                .map_err(|_| api_error(StatusCode::BAD_REQUEST, 1001, "公民 CID 过长"))?;
            public_admins.push(admin_primitives::Admin {
                account_id,
                cid_number,
                family_name,
                given_name,
            });
        } else {
            private_admins.push(admin_primitives::Admin {
                account_id,
                cid_number: Default::default(),
                family_name,
                given_name,
            });
        }
    }
    if admins.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "机构管理员至少需要 1 人",
        ));
    }
    Ok(if is_public {
        InstitutionAdminsPayload::Public(public_admins)
    } else {
        InstitutionAdminsPayload::Private(private_admins)
    })
}

fn encode_governance_action<AdminRecord: Encode>(
    admins: Option<Vec<AdminRecord>>,
    role_mutations: Vec<entity_primitives::InstitutionRoleMutation<[u8; 32]>>,
    assignment_changes: Vec<entity_primitives::InstitutionRoleAssignmentChange<[u8; 32]>>,
    legal_representative_change: Option<
        entity_primitives::InstitutionLegalRepresentativeChange<[u8; 32]>,
    >,
) -> Vec<u8> {
    match admins {
        None => entity_primitives::InstitutionGovernanceAction::<[u8; 32], AdminRecord>::MutateRolesAndAssignments {
            role_mutations,
            assignment_changes,
            legal_representative_change,
        },
        Some(admins)
            if role_mutations.is_empty()
                && assignment_changes.is_empty()
                && legal_representative_change.is_none() =>
        {
            entity_primitives::InstitutionGovernanceAction::ReplaceAdmins { admins }
        }
        Some(admins) => entity_primitives::InstitutionGovernanceAction::ReplaceAdminsAndMutateRoles {
            admins,
            role_mutations,
            assignment_changes,
            legal_representative_change,
        },
    }
    .encode()
}

fn parse_permission_operation(
    value: &str,
) -> Result<entity_primitives::RolePermissionOperation, axum::response::Response> {
    match value.trim().to_ascii_uppercase().as_str() {
        "PROPOSE" => Ok(entity_primitives::RolePermissionOperation::Propose),
        "VOTE" => Ok(entity_primitives::RolePermissionOperation::Vote),
        _ => Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "岗位权限操作只能是 PROPOSE 或 VOTE",
        )),
    }
}

fn assignment_targets(
    input: &[InstitutionAssignmentTargetInput],
) -> Result<Vec<entity_primitives::InstitutionAssignmentTarget<[u8; 32]>>, axum::response::Response>
{
    let mut seen_accounts = std::collections::BTreeSet::new();
    let mut out = Vec::with_capacity(input.len());
    for target in input {
        let account_id = parse_account_id_bytes(target.account_id.trim()).ok_or_else(|| {
            api_error(
                StatusCode::BAD_REQUEST,
                1001,
                "任职 account_id 必须是小写 0x 加 64 位十六进制",
            )
        })?;
        if !seen_accounts.insert(account_id) {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                1001,
                "同一岗位任职账户不能重复",
            ));
        }
        out.push(entity_primitives::InstitutionAssignmentTarget {
            account_id,
            term_start: target.term_start,
            term_end: target.term_end,
            assignment_source:
                entity_primitives::InstitutionAssignmentSource::InstitutionGovernance,
            assignment_source_ref: b"onchina-governance".to_vec(),
            assignment_status: entity_primitives::InstitutionAssignmentStatus::Active,
        });
    }
    Ok(out)
}

fn role_mutations(
    input: &[InstitutionRoleMutationInput],
) -> Result<Vec<entity_primitives::InstitutionRoleMutation<[u8; 32]>>, axum::response::Response> {
    let mut out = Vec::with_capacity(input.len());
    for item in input {
        match item.mutation.trim().to_ascii_uppercase().as_str() {
            "CREATE" => {
                if item
                    .role_code
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
                {
                    return Err(api_error(
                        StatusCode::BAD_REQUEST,
                        1001,
                        "创建岗位不得提交岗位码",
                    ));
                }
                let role_name = item.role_name.as_deref().unwrap_or_default().trim();
                if role_name.is_empty() || item.permissions.is_empty() {
                    return Err(api_error(
                        StatusCode::BAD_REQUEST,
                        1001,
                        "创建岗位必须提交岗位名和权限",
                    ));
                }
                if role_name.len() > 128 || item.permissions.len() > 256 {
                    return Err(api_error(
                        StatusCode::BAD_REQUEST,
                        1001,
                        "岗位名或岗位权限数量超过链上上限",
                    ));
                }
                for assignment in &item.assignments {
                    let valid_term = if item.term_required {
                        assignment.term_start > 0 && assignment.term_end >= assignment.term_start
                    } else {
                        assignment.term_start == 0 && assignment.term_end == 0
                    };
                    if !valid_term {
                        return Err(api_error(
                            StatusCode::BAD_REQUEST,
                            1001,
                            "初始任职任期与岗位任期规则不一致",
                        ));
                    }
                }
                let mut seen_permissions = std::collections::BTreeSet::new();
                let mut permissions = Vec::with_capacity(item.permissions.len());
                for permission in &item.permissions {
                    let module_tag = permission.module_tag.trim();
                    let operation = parse_permission_operation(&permission.operation)?;
                    if module_tag.is_empty()
                        || module_tag.len() > 32
                        || !seen_permissions.insert((
                            module_tag.to_string(),
                            permission.action_code,
                            operation as u8,
                        ))
                    {
                        return Err(api_error(
                            StatusCode::BAD_REQUEST,
                            1001,
                            "岗位权限不能为空或重复",
                        ));
                    }
                    permissions.push(entity_primitives::RolePermissionSpec {
                        business_action_id: entity_primitives::BusinessActionId {
                            module_tag: module_tag.as_bytes().to_vec(),
                            action_code: permission.action_code,
                        },
                        operation,
                    });
                }
                out.push(entity_primitives::InstitutionRoleMutation::Create {
                    role_name: role_name.as_bytes().to_vec(),
                    term_required: item.term_required,
                    permissions,
                    assignments: assignment_targets(&item.assignments)?,
                });
            }
            "RENAME" => {
                let role_code = item.role_code.as_deref().unwrap_or_default().trim();
                let role_name = item.role_name.as_deref().unwrap_or_default().trim();
                if role_code.is_empty()
                    || role_code.len() > 64
                    || role_name.is_empty()
                    || role_name.len() > 128
                    || item.term_required
                    || !item.permissions.is_empty()
                    || !item.assignments.is_empty()
                {
                    return Err(api_error(
                        StatusCode::BAD_REQUEST,
                        1001,
                        "改名只能提交岗位码和新岗位名",
                    ));
                }
                out.push(entity_primitives::InstitutionRoleMutation::Rename {
                    role_code: role_code.as_bytes().to_vec(),
                    role_name: role_name.as_bytes().to_vec(),
                });
            }
            "DELETE" => {
                let role_code = item.role_code.as_deref().unwrap_or_default().trim();
                if role_code.is_empty()
                    || role_code.len() > 64
                    || item
                        .role_name
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty())
                    || item.term_required
                    || !item.permissions.is_empty()
                    || !item.assignments.is_empty()
                {
                    return Err(api_error(
                        StatusCode::BAD_REQUEST,
                        1001,
                        "删除只能提交岗位码",
                    ));
                }
                out.push(entity_primitives::InstitutionRoleMutation::Delete {
                    role_code: role_code.as_bytes().to_vec(),
                });
            }
            _ => {
                return Err(api_error(
                    StatusCode::BAD_REQUEST,
                    1001,
                    "岗位操作只能是 CREATE、RENAME 或 DELETE",
                ))
            }
        }
    }
    Ok(out)
}

fn assignment_changes(
    input: &[InstitutionAssignmentChangeInput],
) -> Result<
    Vec<entity_primitives::InstitutionRoleAssignmentChange<[u8; 32]>>,
    axum::response::Response,
> {
    let mut seen_roles = std::collections::BTreeSet::new();
    let mut out = Vec::with_capacity(input.len());
    for item in input {
        let role_code = item.role_code.trim();
        if role_code.is_empty() {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                1001,
                "任职岗位码不能为空",
            ));
        }
        if !seen_roles.insert(role_code.to_string()) {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                1001,
                "任职变更岗位码不能重复",
            ));
        }
        let assignments = assignment_targets(&item.assignments)?;
        out.push(entity_primitives::InstitutionRoleAssignmentChange {
            role_code: role_code.as_bytes().to_vec(),
            assignments,
        });
    }
    Ok(out)
}

fn legal_representative_change(
    state: &AppState,
    cid_number: Option<&str>,
    clear_legal_representative: bool,
) -> Result<
    Option<entity_primitives::InstitutionLegalRepresentativeChange<[u8; 32]>>,
    axum::response::Response,
> {
    if clear_legal_representative {
        if cid_number
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .is_some()
        {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                1001,
                "解除法定代表人时不能同时填写法定代表人公民CID",
            ));
        }
        return Ok(Some(
            entity_primitives::InstitutionLegalRepresentativeChange::Clear,
        ));
    }
    let Some(cid_number) = cid_number.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let record = match state.db.find_citizen_by_cid(cid_number) {
        Ok(Some(record)) => record,
        Ok(None) => {
            return Err(api_error(
                StatusCode::NOT_FOUND,
                1004,
                "法定代表人公民档案不存在",
            ))
        }
        Err(err) => {
            tracing::error!(error = %err, "query legal representative citizen failed");
            return Err(api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "法定代表人档案查询失败",
            ));
        }
    };
    let Some(account_id) = record.account_id.as_deref() else {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "法定代表人公民未绑定链账户",
        ));
    };
    let account_id_bytes = parse_account_id_bytes(account_id).ok_or_else(|| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            1004,
            "法定代表人 account_id 格式错误",
        )
    })?;
    let family_name = record.family_name.trim();
    let given_name = record.given_name.trim();
    if family_name.is_empty() || given_name.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "法定代表人姓或名为空",
        ));
    }
    Ok(Some(
        entity_primitives::InstitutionLegalRepresentativeChange::Set {
            family_name: family_name.as_bytes().to_vec(),
            given_name: given_name.as_bytes().to_vec(),
            cid_number: cid_number.as_bytes().to_vec(),
            account_id: account_id_bytes,
        },
    ))
}

/// 一次机构链签名会话的完整描述。
///
/// 把「发起管理员 / 机构 CID / 用途 / 待签 call / 动作码 / 业务上下文」收成一体,
/// 避免 `build_chain_sign_output` 变成长参数列表——那样既容易在调用点串位,
/// 也会触发 `clippy::too_many_arguments`(手写函数不走 FRAME ABI 豁免)。
pub(crate) struct InstitutionChainSignRequest<'a> {
    /// 发起管理员的钱包账户 account_id。
    pub(crate) account_id: &'a str,
    /// 目标机构 CID。
    pub(crate) cid_number: &'a str,
    /// 会话用途常量,落库后决定 submit 阶段的投影分支。
    pub(crate) purpose: &'static str,
    /// 待冷签的 extrinsic call_data。
    pub(crate) call_data: Vec<u8>,
    /// QR 链交易动作码 `(pallet_index << 8) | call_index`。
    pub(crate) chain_action: u16,
    /// 落库的业务上下文,供 submit 阶段还原操作语义。
    pub(crate) context: serde_json::Value,
}

pub(crate) async fn build_chain_sign_output(
    state: &AppState,
    request: InstitutionChainSignRequest<'_>,
    passkey: &crate::auth::passkey::PasskeyProof,
) -> Result<PrepareInstitutionChainOutput, axum::response::Response> {
    let InstitutionChainSignRequest {
        account_id,
        cid_number,
        purpose,
        call_data,
        chain_action,
        context,
    } = request;
    let prepared = chain_submit::prepare_signing(&call_data, account_id)
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "prepare institution governance signing failed");
            api_error(
                StatusCode::BAD_GATEWAY,
                1004,
                "链签名载荷准备失败(链不可用)",
            )
        })?;
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::seconds(SESSION_TTL_SECS);
    let request_id = format!("institution-governance-{}", Uuid::new_v4());
    let sign_request = crate::core::qr::build_sign_request_bytes(
        request_id.as_str(),
        issued_at.timestamp(),
        expires_at.timestamp(),
        account_id,
        &prepared.payload,
        chain_action,
    )?;
    let session = ChainSignSession {
        request_id: request_id.clone(),
        purpose: purpose.to_string(),
        account_id: account_id.to_string(),
        call_data: call_data.clone(),
        nonce: prepared.nonce,
        signing_hash: prepared.signing_hash_hex,
        context,
        expires_at,
        consumed_at: None,
    };
    state
        .db
        .insert_chain_sign_session(&session, passkey)
        .map_err(|err| {
            tracing::error!(error = %err, "insert institution governance session failed");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                1004,
                "机构治理冷签会话落库失败",
            )
        })?;
    Ok(PrepareInstitutionChainOutput {
        request_id,
        cid_number: cid_number.to_string(),
        chain_action,
        call_data_hex: format!("0x{}", hex::encode(call_data)),
        sign_request,
        expires_at: expires_at.timestamp(),
    })
}

pub(crate) async fn prepare_institution_governance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PrepareInstitutionGovernanceInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    // 链上写(PasskeyColdSign 档):会话 + passkey + 钱包冷签,三者缺一不可。
    // 凭证向下传给冷签会话创建入口,由类型保证这一步不会被跳过。
    let passkey = match crate::auth::passkey::require_passkey_assertion(
        &state,
        &headers,
        ctx.account_id.as_str(),
    ) {
        Ok(proof) => proof,
        Err(resp) => return resp,
    };
    let binding = match auth_repo::active_node_binding(&state.db) {
        Ok(Some(binding)) => binding,
        Ok(None) => return api_error(StatusCode::FORBIDDEN, 2002, "not an on-chain admin"),
        Err(err) => {
            tracing::error!(error = %err, "query node binding failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                5001,
                "node binding query failed",
            );
        }
    };
    let cid_number = input.cid_number.trim();
    let proposer_role_code = input.proposer_role_code.trim();
    if proposer_role_code.is_empty() || proposer_role_code.len() > 64 {
        return api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "proposer_role_code 长度必须为 1 到 64 字节",
        );
    }
    if cid_number != binding.institution_cid_number
        || binding.institution_code != ctx.institution_code
    {
        return api_error(StatusCode::FORBIDDEN, 1003, "只能发起本机构治理");
    }
    let Some((inst, _)) = (match state.db.get_institution_with_accounts(cid_number) {
        Ok(value) => value,
        Err(err) => {
            tracing::error!(error = %err, "query institution failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, "机构查询失败");
        }
    }) else {
        return api_error(StatusCode::NOT_FOUND, 1004, "机构不存在");
    };
    let role_mutations = match role_mutations(&input.role_mutations) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let assignment_changes = match assignment_changes(&input.assignment_changes) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let legal = match legal_representative_change(
        &state,
        input.legal_representative_cid_number.as_deref(),
        input.clear_legal_representative,
    ) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let code = match code_bytes(&inst.institution_code) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let is_public = !primitives::cid::code::is_private_legal_code(&code);
    let parsed_admins = if input.admins.is_empty() {
        if role_mutations.is_empty() && assignment_changes.is_empty() && legal.is_none() {
            return api_error(StatusCode::BAD_REQUEST, 1001, "机构治理内容不能为空");
        }
        None
    } else {
        let admins = match parse_admin_inputs(&state, &input.admins, is_public) {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        Some(admins)
    };
    let action_payload = match parsed_admins {
        Some(InstitutionAdminsPayload::Public(admins)) => {
            encode_governance_action(Some(admins), role_mutations, assignment_changes, legal)
        }
        Some(InstitutionAdminsPayload::Private(admins)) => {
            encode_governance_action(Some(admins), role_mutations, assignment_changes, legal)
        }
        None if is_public => encode_governance_action::<admin_primitives::Admin<[u8; 32]>>(
            None,
            role_mutations,
            assignment_changes,
            legal,
        ),
        None => encode_governance_action::<admin_primitives::Admin<[u8; 32]>>(
            None,
            role_mutations,
            assignment_changes,
            legal,
        ),
    };
    // 机构治理 = 发起管理员使用签名钱包直接冷签这笔 extrinsic,
    // 授权由 runtime 在 origin 处以 `is_institution_admin`(本机构管理员)+ 岗位码校验。
    let chain = encode_propose_institution_governance(&ProposeInstitutionGovernanceArgs {
        cid_number: cid_number.as_bytes().to_vec(),
        governance_action: action_payload,
        institution_code: code,
        actor_cid_number: cid_number.as_bytes().to_vec(),
        proposer_role_code: proposer_role_code.as_bytes().to_vec(),
    });
    let output = match build_chain_sign_output(
        &state,
        InstitutionChainSignRequest {
            account_id: ctx.account_id.as_str(),
            cid_number,
            purpose: PURPOSE_INSTITUTION_GOVERNANCE,
            call_data: chain.call_data,
            chain_action: chain.action,
            context: serde_json::json!({ "cid_number": cid_number, "op": "governance" }),
        },
        &passkey,
    )
    .await
    {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    crate::core::runtime_ops::append_audit_log(
        &state,
        "INSTITUTION_GOVERNANCE_PREPARE",
        &ctx.account_id,
        Some(cid_number.to_string()),
        serde_json::json!({ "cid_number": cid_number, "actor_ip": actor_ip_from_headers(&headers) }),
    );
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: output,
    })
    .into_response()
}

pub(crate) async fn prepare_register_institution_admins(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PrepareRegisterInstitutionAdminsInput>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    // 链上写(PasskeyColdSign 档):会话 + passkey + 钱包冷签,三者缺一不可。
    // 凭证向下传给冷签会话创建入口,由类型保证这一步不会被跳过。
    let passkey = match crate::auth::passkey::require_passkey_assertion(
        &state,
        &headers,
        ctx.account_id.as_str(),
    ) {
        Ok(proof) => proof,
        Err(resp) => return resp,
    };
    if let Err(resp) = ensure_registry_admin(&ctx) {
        return resp;
    }
    let binding = match auth_repo::active_node_binding(&state.db) {
        Ok(Some(binding)) => binding,
        Ok(None) => return api_error(StatusCode::FORBIDDEN, 2002, "not an on-chain admin"),
        Err(err) => {
            tracing::error!(error = %err, "query node binding failed");
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                5001,
                "node binding query failed",
            );
        }
    };
    if binding.institution_code != ctx.institution_code {
        return api_error(StatusCode::FORBIDDEN, 1003, "permission denied");
    }
    let cid_number = input.cid_number.trim();
    let Some((inst, _)) = (match state.db.get_institution_with_accounts(cid_number) {
        Ok(value) => value,
        Err(err) => {
            tracing::error!(error = %err, "query institution failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, "机构查询失败");
        }
    }) else {
        return api_error(StatusCode::NOT_FOUND, 1004, "机构不存在");
    };
    let code = match code_bytes(&inst.institution_code) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let is_public = !primitives::cid::code::is_private_legal_code(&code);
    let admins = match parse_admin_inputs(&state, &input.admins, is_public) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    // 管理员登记 = 注册局管理员使用签名钱包直接冷签这笔 extrinsic,
    // 授权由 runtime 在 origin 处以 `can_register_institution_origin`(注册局在册管理员 +
    // 对目标机构有登记权)校验。
    let chain = encode_register_institution_admins(&RegisterInstitutionAdminsArgs {
        cid_number: cid_number.as_bytes().to_vec(),
        admins,
        institution_code: code,
        actor_cid_number: binding.institution_cid_number.as_bytes().to_vec(),
    });
    let output = match build_chain_sign_output(
        &state,
        InstitutionChainSignRequest {
            account_id: ctx.account_id.as_str(),
            cid_number,
            purpose: PURPOSE_INSTITUTION_REGISTER_ADMINS,
            call_data: chain.call_data,
            chain_action: chain.action,
            context: serde_json::json!({
                "cid_number": cid_number,
                "actor_cid_number": binding.institution_cid_number,
                "op": "register_admins"
            }),
        },
        &passkey,
    )
    .await
    {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    crate::core::runtime_ops::append_audit_log(
        &state,
        "INSTITUTION_REGISTER_ADMINS_PREPARE",
        &ctx.account_id,
        Some(cid_number.to_string()),
        serde_json::json!({ "cid_number": cid_number, "actor_ip": actor_ip_from_headers(&headers) }),
    );
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: output,
    })
    .into_response()
}
