//! 管理机构管理员链下私密资料，解析链上管理员与岗位权限输入。

#![allow(clippy::result_large_err)]

use super::chain_roles;

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
    authentication::{login::parse_account_id_bytes, repo as auth_repo},
    chain_submit,
    institutions::{
        encode_propose_institution_governance, InstitutionAdminsPayload,
        ProposeInstitutionGovernanceArgs,
    },
    require_admin_any,
    transactions::session::{ChainSignSession, SESSION_TTL_SECS},
    ApiResponse, AppState,
};

pub(crate) const PURPOSE_INSTITUTION_GOVERNANCE: &str = "INSTITUTION_GOVERNANCE";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionAdminInput {
    pub account_id: String,
    #[serde(default)]
    pub cid_number: Option<String>,
    #[serde(default)]
    pub family_name: Option<String>,
    #[serde(default)]
    pub given_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionRolePermissionInput {
    pub(crate) module_tag: String,
    pub(crate) action_code: u32,
    pub(crate) operation: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionRoleMutationInput {
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
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionAssignmentTargetInput {
    pub(crate) account_id: String,
    #[serde(default)]
    pub(crate) term_start: u32,
    #[serde(default)]
    pub(crate) term_end: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionAssignmentChangeInput {
    pub(crate) role_code: String,
    #[serde(default)]
    pub(crate) assignments: Vec<InstitutionAssignmentTargetInput>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
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
    pub(crate) representative_cid: Option<String>,
    #[serde(default)]
    pub(crate) clear_legal_representative: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareInstitutionChainOutput {
    pub request_id: String,
    pub cid_number: String,
    pub chain_action: u16,
    pub call_data_hex: String,
    pub sign_request: String,
    pub expires_at: i64,
}

pub fn code_bytes(institution_code: &str) -> Result<[u8; 4], axum::response::Response> {
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

pub fn parse_admin_inputs(
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
            .find_governance_citizen_by_account_id(admin.account_id.trim())
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
        let family_name: adminbase::FamilyName = family_name
            .into_bytes()
            .try_into()
            .map_err(|_| api_error(StatusCode::BAD_REQUEST, 1001, "管理员姓过长"))?;
        let given_name: adminbase::GivenName = given_name
            .into_bytes()
            .try_into()
            .map_err(|_| api_error(StatusCode::BAD_REQUEST, 1001, "管理员名过长"))?;
        if is_public {
            let cid_number: adminbase::AdminCidNumber = citizen_cid
                .unwrap_or_default()
                .into_bytes()
                .try_into()
                .map_err(|_| api_error(StatusCode::BAD_REQUEST, 1001, "公民 CID 过长"))?;
            public_admins.push(adminbase::Admin {
                account_id,
                cid_number,
                family_name,
                given_name,
            });
        } else {
            private_admins.push(adminbase::Admin {
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

/// 既有成员缺省字段取原链记录，显式字段仍逐项校验；不从本地资料重建名册锚点。
pub(crate) fn retained_registration_admin(
    input: &InstitutionAdminInput,
    stored: &adminbase::Admin<[u8; 32]>,
) -> Result<adminbase::Admin<[u8; 32]>, axum::response::Response> {
    let mut result = stored.clone();
    if let Some(cid) = &input.cid_number {
        result.cid_number = cid
            .trim()
            .as_bytes()
            .to_vec()
            .try_into()
            .map_err(|_| api_error(StatusCode::BAD_REQUEST, 1001, "公民CID过长"))?;
    }
    if let Some(name) = &input.family_name {
        result.family_name = name
            .trim()
            .as_bytes()
            .to_vec()
            .try_into()
            .map_err(|_| api_error(StatusCode::BAD_REQUEST, 1001, "管理员姓过长"))?;
    }
    if let Some(name) = &input.given_name {
        result.given_name = name
            .trim()
            .as_bytes()
            .to_vec()
            .try_into()
            .map_err(|_| api_error(StatusCode::BAD_REQUEST, 1001, "管理员名过长"))?;
    }
    Ok(result)
}

pub fn parse_registration_admin_inputs(
    state: &AppState,
    inputs: &[InstitutionAdminInput],
    snapshot: &chain_roles::RegistrationSnapshot,
) -> Result<InstitutionAdminsPayload, axum::response::Response> {
    if inputs.is_empty() || inputs.len() > 1989 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "管理员人数超出链上范围",
        ));
    }
    let is_public =
        !primitives::cid::code::is_private_legal_code(&snapshot.institution.institution_code);
    let previous = snapshot
        .admins
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|admin| (admin.account_id, admin))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut seen = std::collections::BTreeSet::new();
    let mut admins = Vec::with_capacity(inputs.len());
    for input in inputs {
        let account = parse_account_id_bytes(input.account_id.trim())
            .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, 1001, "管理员账户无效"))?;
        if !seen.insert(account) {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                1001,
                "管理员账户不能重复",
            ));
        }
        if let Some(stored) = previous.get(&account) {
            admins.push(retained_registration_admin(input, stored)?);
        } else {
            let (InstitutionAdminsPayload::Public(mut parsed)
            | InstitutionAdminsPayload::Private(mut parsed)) =
                parse_admin_inputs(state, std::slice::from_ref(input), is_public)?;
            admins.append(&mut parsed);
        }
    }
    Ok(if is_public {
        InstitutionAdminsPayload::Public(admins)
    } else {
        InstitutionAdminsPayload::Private(admins)
    })
}

fn encode_governance_action<AdminRecord: Encode>(
    admins: Option<Vec<AdminRecord>>,
    role_mutations: Vec<entitybase::InstitutionRoleMutation<[u8; 32]>>,
    assignment_changes: Vec<entitybase::InstitutionRoleAssignmentChange<[u8; 32]>>,
    legal_representative_change: Option<entitybase::InstitutionLegalRepresentativeChange<[u8; 32]>>,
) -> Vec<u8> {
    match admins {
        None => entitybase::InstitutionGovernanceAction::<[u8; 32], AdminRecord>::MutateRolesAndAssignments {
            role_mutations,
            assignment_changes,
            legal_representative_change,
        },
        Some(admins)
            if role_mutations.is_empty()
                && assignment_changes.is_empty()
                && legal_representative_change.is_none() =>
        {
            entitybase::InstitutionGovernanceAction::ReplaceAdmins { admins }
        }
        Some(admins) => entitybase::InstitutionGovernanceAction::ReplaceAdminsAndMutateRoles {
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
) -> Result<entitybase::RolePermissionOperation, axum::response::Response> {
    match value.trim().to_ascii_uppercase().as_str() {
        "PROPOSE" => Ok(entitybase::RolePermissionOperation::Propose),
        "VOTE" => Ok(entitybase::RolePermissionOperation::Vote),
        _ => Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "岗位权限操作只能是 PROPOSE 或 VOTE",
        )),
    }
}

fn assignment_targets(
    input: &[InstitutionAssignmentTargetInput],
    registration: Option<&chain_roles::RegistrationSnapshot>,
    role_code: Option<&[u8]>,
) -> Result<Vec<entitybase::InstitutionAssignmentTarget<[u8; 32]>>, axum::response::Response> {
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
        if let Some(retained) = registration.and_then(|snapshot| {
            role_code.and_then(|role| {
                snapshot.retained_assignment(role, account_id, target.term_start, target.term_end)
            })
        }) {
            out.push(retained);
            continue;
        }
        out.push(entitybase::InstitutionAssignmentTarget {
            account_id,
            term_start: target.term_start,
            term_end: target.term_end,
            assignment_source: if registration.is_some() {
                entitybase::InstitutionAssignmentSource::Registry
            } else {
                entitybase::InstitutionAssignmentSource::InstitutionGovernance
            },
            assignment_source_ref: if registration.is_some() {
                Vec::new()
            } else {
                b"onchina-governance".to_vec()
            },
            assignment_status: entitybase::InstitutionAssignmentStatus::Active,
        });
    }
    Ok(out)
}

fn role_mutations(
    input: &[InstitutionRoleMutationInput],
    registration: Option<&chain_roles::RegistrationSnapshot>,
) -> Result<Vec<entitybase::InstitutionRoleMutation<[u8; 32]>>, axum::response::Response> {
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
                    permissions.push(entitybase::RolePermissionSpec {
                        business_action_id: entitybase::BusinessActionId {
                            module_tag: module_tag.as_bytes().to_vec(),
                            action_code: permission.action_code,
                        },
                        operation,
                    });
                }
                out.push(entitybase::InstitutionRoleMutation::Create {
                    role_name: role_name.as_bytes().to_vec(),
                    term_required: item.term_required,
                    permissions,
                    assignments: assignment_targets(&item.assignments, registration, None)?,
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
                out.push(entitybase::InstitutionRoleMutation::Rename {
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
                out.push(entitybase::InstitutionRoleMutation::Delete {
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
    registration: Option<&chain_roles::RegistrationSnapshot>,
) -> Result<Vec<entitybase::InstitutionRoleAssignmentChange<[u8; 32]>>, axum::response::Response> {
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
        let assignments =
            assignment_targets(&item.assignments, registration, Some(role_code.as_bytes()))?;
        out.push(entitybase::InstitutionRoleAssignmentChange {
            role_code: role_code.as_bytes().to_vec(),
            assignments,
        });
    }
    Ok(out)
}

/// 登记复用治理输入及校验；新任职由Runtime写入真实Registry引用，存量原记录逐字段保留。
/// 调用者不能提交来源、引用或法定代表人变更，也不能通过登记改名/删除岗位。
pub fn parse_registration_action(
    admins: InstitutionAdminsPayload,
    mutations: &[InstitutionRoleMutationInput],
    changes: &[InstitutionAssignmentChangeInput],
    snapshot: &chain_roles::RegistrationSnapshot,
    initial_threshold: Option<u32>,
) -> Result<entitybase::InstitutionGovernanceAction<[u8; 32]>, axum::response::Response> {
    if mutations
        .iter()
        .any(|m| !m.mutation.trim().eq_ignore_ascii_case("CREATE"))
        || (snapshot.threshold.is_some() && (!mutations.is_empty() || initial_threshold.is_some()))
        || (snapshot.threshold.is_none() && (mutations.is_empty() || initial_threshold.is_none()))
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            1001,
            "首次登记须提交岗位和阈值；已初始化机构只能登记名册和任职",
        ));
    }
    let role_mutations = role_mutations(mutations, Some(snapshot))?;
    let assignment_changes = assignment_changes(changes, Some(snapshot))?;
    let (InstitutionAdminsPayload::Public(admins) | InstitutionAdminsPayload::Private(admins)) =
        admins;
    if role_mutations.is_empty() && assignment_changes.is_empty() {
        Ok(entitybase::InstitutionGovernanceAction::ReplaceAdmins { admins })
    } else {
        Ok(
            entitybase::InstitutionGovernanceAction::ReplaceAdminsAndMutateRoles {
                admins,
                role_mutations,
                assignment_changes,
                legal_representative_change: None,
            },
        )
    }
}

fn legal_representative_change(
    state: &AppState,
    cid_number: Option<&str>,
    clear_legal_representative: bool,
) -> Result<
    Option<entitybase::InstitutionLegalRepresentativeChange<[u8; 32]>>,
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
            entitybase::InstitutionLegalRepresentativeChange::Clear,
        ));
    }
    let Some(cid_number) = cid_number.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let record = match state.db.find_governance_citizen_by_cid(cid_number) {
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
        entitybase::InstitutionLegalRepresentativeChange::Set {
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
pub struct InstitutionChainSignRequest<'a> {
    /// 发起管理员的钱包账户 account_id。
    pub account_id: &'a str,
    /// 目标机构 CID。
    pub cid_number: &'a str,
    /// 会话用途常量,落库后决定 submit 阶段的投影分支。
    pub purpose: &'static str,
    /// 待冷签的 extrinsic call_data。
    pub call_data: Vec<u8>,
    /// QR 链交易动作码 `(pallet_index << 8) | call_index`。
    pub chain_action: u16,
    /// 落库的业务上下文,供 submit 阶段还原操作语义。
    pub context: serde_json::Value,
}

pub async fn build_chain_sign_output(
    state: &AppState,
    request: InstitutionChainSignRequest<'_>,
    passkey: &crate::authentication::passkey::PasskeyProof,
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
    let sign_request = crate::qr::build_sign_request_bytes(
        request_id.as_str(),
        issued_at.timestamp(),
        expires_at.timestamp(),
        account_id,
        &prepared.payload,
        chain_action,
    )?;
    let actor_cid_number = context
        .get("actorCidNumber")
        .and_then(|value| value.as_str())
        .unwrap_or(cid_number)
        .to_string();
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
        .insert_chain_sign_session(&session, passkey, &actor_cid_number)
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
    let passkey = match crate::authentication::passkey::require_passkey_assertion(
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
    let role_mutations = match role_mutations(&input.role_mutations, None) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let assignment_changes = match assignment_changes(&input.assignment_changes, None) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let legal = match legal_representative_change(
        &state,
        input.representative_cid.as_deref(),
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
        None if is_public => encode_governance_action::<adminbase::Admin<[u8; 32]>>(
            None,
            role_mutations,
            assignment_changes,
            legal,
        ),
        None => encode_governance_action::<adminbase::Admin<[u8; 32]>>(
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
    crate::audit_runtime::append_audit_log(
        &state,
        "INSTITUTION_GOVERNANCE_PREPARE",
        &ctx.account_id,
        Some(cid_number.to_string()),
        serde_json::json!({ "cidNumber": cid_number, "actorIp": actor_ip_from_headers(&headers) }),
    );
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: output,
    })
    .into_response()
}

/// 仅治理读取允许各省专员访问同一个 FRG；地域办理范围和写入授权保持独立。
pub(super) fn can_read_institution_governance(
    ctx: &crate::authentication::login::AdminAuthContext,
    institution_code: &str,
    cid_number: &str,
    province_name: &str,
    city_name: &str,
    town_name: &str,
) -> bool {
    let frg = primitives::governance_skeleton::federal_registry_institution();
    let same_frg = ctx.institution_code == "FRG"
        && institution_code == "FRG"
        && ctx.institution_cid_number == cid_number
        && cid_number == frg.cid_number;
    same_frg
        || crate::authorization::rules::get_visible_scope(ctx).includes_institution(
            cid_number,
            province_name,
            city_name,
            town_name,
        )
}

/// 治理数据只使用就绪登录会话和只读范围，不重复调用管理员资格守卫。
pub(crate) async fn get_governance_state(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(cid_number): axum::extract::Path<String>,
) -> impl IntoResponse {
    let ctx = match require_admin_any(&state, &headers) {
        Ok(ctx) => ctx,
        Err(response) => return response,
    };
    let inst = match state.db.get_institution_with_accounts(&cid_number) {
        Ok(Some((inst, _))) => inst,
        Ok(None) => return api_error(StatusCode::NOT_FOUND, 1004, "institution not found"),
        Err(error) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, &error),
    };
    if !can_read_institution_governance(
        &ctx,
        &inst.institution_code,
        &inst.cid_number,
        &inst.province_name,
        &inst.city_name,
        &inst.town_name,
    ) {
        return api_error(StatusCode::FORBIDDEN, 1003, "out of admin scope");
    }
    let identity = match crate::chain_runtime::identity_from_binding_parts(
        &inst.institution_code,
        Some(&inst.cid_number),
    ) {
        Ok(identity) => identity,
        Err(error) => return api_error(StatusCode::BAD_REQUEST, 1001, &error),
    };
    match chain_roles::fetch_governance_state(&identity).await {
        Ok(mut data) => {
            if matches!(ctx.institution_code.as_str(), "FRG" | "CREG") {
                let actor = match crate::chain_runtime::identity_from_binding_parts(
                    &ctx.institution_code,
                    Some(&ctx.institution_cid_number),
                ) {
                    Ok(actor) => actor,
                    Err(error) => return api_error(StatusCode::BAD_REQUEST, 1001, &error),
                };
                let block = match hex::decode(data.block_hash.trim_start_matches("0x"))
                    .ok()
                    .and_then(|v| <[u8; 32]>::try_from(v).ok())
                {
                    Some(block) => block,
                    None => return api_error(StatusCode::BAD_GATEWAY, 5002, "链状态区块无效"),
                };
                data.registration_roles = match chain_roles::fetch_registration_roles_at(
                    &actor,
                    &ctx.account_id,
                    &cid_number,
                    block,
                )
                .await
                {
                    Ok(roles) => roles,
                    Err(error) => {
                        tracing::error!(%error, "read registration roles failed");
                        return api_error(
                            StatusCode::BAD_GATEWAY,
                            5002,
                            "办理岗位读取失败，请重试",
                        );
                    }
                };
            }
            Json(ApiResponse {
                code: 0,
                message: "ok".into(),
                data,
            })
            .into_response()
        }
        Err(error) => {
            tracing::error!(%error, "read governance state failed");
            api_error(StatusCode::BAD_GATEWAY, 5002, "治理链上数据读取失败")
        }
    }
}

/// Institutional proof parameters are frozen; the personal CID binding is never written here.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct InstitutionWalletInput {
    cid_number: String,
    admin_account: String,
    new_wallet: String,
    mode: String,
    #[serde(default)]
    proposer_role_code: String,
    #[serde(default)]
    expires_at: Option<u64>,
    #[serde(default)]
    new_wallet_response: Option<String>,
}
pub(crate) const PURPOSE_INSTITUTION_WALLET: &str = "INSTITUTION_WALLET";

async fn wallet_call(
    state: &AppState,
    ctx: &crate::authentication::login::AdminAuthContext,
    input: &InstitutionWalletInput,
) -> Result<
    (
        adminbase::InstitutionWalletAcceptance<[u8; 32], [u8; 32]>,
        String,
        Option<super::chain_call::ChainCall>,
    ),
    axum::response::Response,
> {
    let bad = |message: &str| api_error(StatusCode::BAD_REQUEST, 1001, message);
    let binding = auth_repo::active_node_binding(&state.db)
        .map_err(|_| bad("节点绑定读取失败"))?
        .ok_or_else(|| bad("节点尚未绑定机构"))?;
    if input.cid_number != binding.institution_cid_number
        || input.cid_number != ctx.institution_cid_number
        || binding.institution_code != ctx.institution_code
    {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "只能办理本机构管理员钱包",
        ));
    }
    let identity = crate::chain_runtime::identity_from_binding_parts(
        &binding.institution_code,
        Some(&binding.institution_cid_number),
    )
    .map_err(|_| bad("机构绑定无效"))?;
    let view = chain_roles::fetch_governance_state(&identity)
        .await
        .map_err(|_| bad("治理链状态读取失败"))?;
    let member = view
        .members
        .iter()
        .find(|m| m.account_id == input.admin_account)
        .ok_or_else(|| bad("目标不是本机构在册管理员"))?;
    let actor = view
        .members
        .iter()
        .find(|m| m.signer_id == ctx.account_id)
        .ok_or_else(|| bad("当前钱包已无本机构授权"))?;
    let mode = match input.mode.as_str() {
        "SELF" if actor.account_id == member.account_id && input.proposer_role_code.is_empty() => {
            adminbase::InstitutionWalletChangeMode::SelfSigned
        }
        "GOVERNANCE" => {
            let role = view
                .roles
                .iter()
                .find(|r| {
                    r.active
                        && r.role_code == input.proposer_role_code
                        && r.assignments
                            .iter()
                            .any(|a| a.account_id == actor.account_id && a.effective)
                })
                .ok_or_else(|| bad("发起人无有效岗位"))?;
            if !role.permissions.iter().any(|p| {
                p.module_tag == view.module_tag
                    && p.action_code == entitybase::business_action::ACTION_INSTITUTION_GOVERNANCE
                    && p.operation == "PROPOSE"
            }) {
                return Err(bad("岗位无机构治理提案权限"));
            }
            if binding.institution_code == "FRG"
                && !role
                    .assignments
                    .iter()
                    .any(|a| a.effective && a.account_id == member.account_id)
            {
                return Err(bad("只能更换本省专员岗位内的钱包"));
            }
            adminbase::InstitutionWalletChangeMode::Governance
        }
        _ => return Err(bad("更换方式或本人目标不符")),
    };
    let parse = |raw: &str| parse_account_id_bytes(raw).ok_or_else(|| bad("钱包账户编码无效"));
    let new_wallet = parse(&input.new_wallet)?;
    if view.members.iter().any(|m| m.signer_id == input.new_wallet) {
        return Err(bad("新钱包已是本机构管理员钱包"));
    }
    let now = Utc::now().timestamp() as u64;
    let limit = if matches!(mode, adminbase::InstitutionWalletChangeMode::SelfSigned) {
        180
    } else {
        adminbase::INSTITUTION_WALLET_PROOF_MAX_LIFETIME_SECS
    };
    let expires_at = input.expires_at.unwrap_or(now + limit);
    if expires_at <= now
        || expires_at > now + limit
        || (input.new_wallet_response.is_some() && input.expires_at.is_none())
    {
        return Err(bad("接收证明已过期或缺少冻结期限"));
    }
    let finalized = crate::chain_runtime::FinalizedChainView::connect()
        .await
        .map_err(|_| bad("链不可用"))?;
    let acceptance = adminbase::InstitutionWalletAcceptance {
        genesis_hash: finalized.client.genesis_hash().0,
        cid_number: input
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .map_err(|_| bad("机构CID过长"))?,
        change: adminbase::InstitutionAdminWalletChange {
            admin_account: parse(&member.account_id)?,
            expected_wallet: parse(&member.signer_id)?,
            new_wallet,
            expected_revision: member.wallet_revision,
        },
        mode,
        expires_at,
    };
    let payload = acceptance.encode();
    let request_id = format!(
        "institution-wallet-{}",
        hex::encode(sp_crypto_hashing::blake2_256(&payload))
    );
    let Some(response) = &input.new_wallet_response else {
        return Ok((acceptance, request_id, None));
    };
    let response =
        crate::qr::parse_sign_response(response).map_err(|_| bad("新钱包签名响应无效"))?;
    if response.id.as_deref() != Some(request_id.as_str())
        || response
            .expires_at
            .is_none_or(|e| e <= now as i64 || e > expires_at as i64)
        || response.body.account_id != input.new_wallet
    {
        return Err(bad("新钱包签名响应不匹配冻结证明"));
    }
    let proof = hex::decode(response.body.signature.trim_start_matches("0x"))
        .map_err(|_| bad("签名编码无效"))?;
    let message =
        primitives::sign::signing_message(primitives::sign::OP_SIGN_INSTITUTION_WALLET, &payload);
    let valid = match chain_signing::mldsa::signature_parts(&proof) {
        Ok((Some(public), signature)) => {
            use codec::Decode;
            let public = chain_signing::mldsa::PublicKey::decode(&mut &public[..])
                .map_err(|_| bad("新钱包公钥无效"))?;
            let signature = chain_signing::mldsa::Signature::decode(&mut &signature[..])
                .map_err(|_| bad("新钱包签名无效"))?;
            chain_signing::mldsa::verify_account_signature(
                &new_wallet,
                &public,
                &message,
                &signature,
            )
        }
        Ok((None, _)) => {
            !crate::chain_runtime::operation_at(&finalized.storage())
                .await
                .map_err(|_| bad("链阶段不可读取"))?
                && crate::authentication::login::verify_admin_signature_bytes(
                    &input.new_wallet,
                    &message,
                    &response.body.signature,
                )
        }
        Err(_) => false,
    };
    if !valid {
        return Err(bad("新钱包接收证明验签失败"));
    }
    let proof: adminbase::InstitutionWalletProof =
        proof.try_into().map_err(|_| bad("接收证明过长"))?;
    // Actual chain owner is authoritative for UNIN; never infer its pallet from its code.
    let pallet = match view.module_tag.as_str() {
        "pub-mgmt" => 30,
        "pri-mgmt" => 31,
        _ => return Err(bad("机构归属无效")),
    };
    let call = if matches!(
        acceptance.mode,
        adminbase::InstitutionWalletChangeMode::SelfSigned
    ) {
        let mut bytes = vec![pallet, 10];
        bytes.extend(acceptance.cid_number.encode());
        bytes.extend(acceptance.change.encode());
        bytes.extend(expires_at.encode());
        bytes.extend(proof.encode());
        super::chain_call::ChainCall {
            action: super::chain_call::chain_action_code(pallet, 10),
            call_data: bytes,
        }
    } else {
        let action = entitybase::InstitutionGovernanceAction::<[u8; 32]>::RotateAdminWallet {
            change: acceptance.change.clone(),
            expires_at,
            new_wallet_proof: proof,
        };
        let mut bytes = vec![pallet, 8];
        bytes.extend(acceptance.cid_number.encode());
        bytes.extend(action.encode());
        bytes.extend(acceptance.cid_number.encode());
        bytes.extend(input.proposer_role_code.as_bytes().to_vec().encode());
        super::chain_call::ChainCall {
            action: super::chain_call::chain_action_code(pallet, 8),
            call_data: bytes,
        }
    };
    Ok((acceptance, request_id, Some(call)))
}

/// First call returns the new-wallet proof QR; second call freezes that exact proof in the old-wallet/proposer transaction.
pub(crate) async fn prepare_institution_wallet(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<InstitutionWalletInput>,
) -> axum::response::Response {
    let result = async {
        let ctx = require_admin_any(&state, &headers)?;
        let (acceptance, id, call) = wallet_call(&state, &ctx, &input).await?;
        if let Some(call) = call {
            let passkey = crate::authentication::passkey::require_passkey_assertion(&state, &headers, &ctx.account_id)?;
            let output = build_chain_sign_output(&state, InstitutionChainSignRequest { account_id: &ctx.account_id, cid_number: &input.cid_number, purpose: PURPOSE_INSTITUTION_WALLET, call_data: call.call_data, chain_action: call.action, context: serde_json::json!({"cid_number": input.cid_number, "walletInput": input, "expectedWallet": format!("0x{}",hex::encode(acceptance.change.expected_wallet)), "mode": input.mode}) }, &passkey).await?;
            Ok(serde_json::to_value(output).map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, "签名会话编码失败"))?)
        } else {
            let action = qr_protocol::action_by_key("institution_wallet_accept").map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, "签名协议缺失"))?.action_code;
            let qr = crate::qr::build_sign_request_bytes(&id, Utc::now().timestamp(), (Utc::now().timestamp() + 180).min(acceptance.expires_at as i64), &input.new_wallet, &acceptance.encode(), action)?;
            Ok::<_, axum::response::Response>(serde_json::json!({"requestId":id,"expiresAt":acceptance.expires_at,"acceptancePayloadHex":format!("0x{}",hex::encode(acceptance.encode())),"signRequest":qr}))
        }
    }.await;
    match result {
        Ok(data) => Json(ApiResponse {
            code: 0,
            message: "ok".into(),
            data,
        })
        .into_response(),
        Err(response) => response,
    }
}

pub(crate) async fn recheck_institution_wallet(
    state: &AppState,
    ctx: &crate::authentication::login::AdminAuthContext,
    session: &ChainSignSession,
) -> Result<(), axum::response::Response> {
    let input = serde_json::from_value(session.context["walletInput"].clone())
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, 1001, "冻结更换上下文缺失"))?;
    let (_, _, call) = wallet_call(state, ctx, &input).await?;
    if session.account_id != ctx.account_id || call.is_none_or(|c| c.call_data != session.call_data)
    {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            1003,
            "冻结钱包更换已失效，请重新审阅签名",
        ));
    }
    Ok(())
}
