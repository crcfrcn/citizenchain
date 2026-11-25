//! 机构岗位、人员任职和法代治理；签名由人员CID当前绑定钱包完成。
use super::{encode_propose_institution_governance, roles, ProposeInstitutionGovernanceArgs};
use crate::{
    api_error, chain_submit, require_admin_any,
    transactions::session::{ChainSignSession, SESSION_TTL_SECS},
    ApiResponse, AppState,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::{Duration, Utc};
use codec::Encode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
pub(crate) const PURPOSE_INSTITUTION_GOVERNANCE: &str = "INSTITUTION_GOVERNANCE";
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstitutionRolePermissionInput {
    pub module_tag: String,
    pub action_code: u32,
    pub operation: String,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstitutionAssignmentTargetInput {
    pub person_cid: String,
    #[serde(default)]
    pub term_start: u32,
    #[serde(default)]
    pub term_end: u32,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstitutionAssignmentChangeInput {
    pub role_code: String,
    pub assignments: Vec<InstitutionAssignmentTargetInput>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstitutionRoleMutationInput {
    pub mutation: String,
    pub role_code: Option<String>,
    pub role_name: Option<String>,
    #[serde(default)]
    pub term_required: bool,
    pub seats: Option<u32>,
    /// Configure必须明确目标状态；其他动作不能夹带此字段。
    pub active: Option<bool>,
    #[serde(default)]
    pub permissions: Vec<InstitutionRolePermissionInput>,
    #[serde(default)]
    pub assignments: Vec<InstitutionAssignmentTargetInput>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PrepareInstitutionGovernanceInput {
    pub cid_number: String,
    pub proposer_role_code: String,
    #[serde(default)]
    pub role_mutations: Vec<InstitutionRoleMutationInput>,
    #[serde(default)]
    pub assignment_changes: Vec<InstitutionAssignmentChangeInput>,
    pub representative_cid: Option<String>,
    #[serde(default)]
    pub clear_legal_representative: bool,
    pub representative_family_name: Option<String>,
    pub representative_given_name: Option<String>,
    /// 政策与岗位人事是互斥治理动作，复用同一准备入口和认证流程。
    pub policy: Option<InstitutionPolicyInput>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct InstitutionPolicyInput {
    #[serde(default)]
    pub permission_changes: Vec<RolePolicyInput>,
    pub threshold: Option<u32>,
    pub expected_state_hash: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RolePolicyInput {
    pub role_code: String,
    pub permissions: Vec<InstitutionRolePermissionInput>,
}

pub(crate) fn parse_policy_action(
    snapshot: &roles::RegistrationSnapshot,
    input: &InstitutionPolicyInput,
) -> Result<entitybase::InstitutionPolicyAction, String> {
    if !entitybase::business_action::is_ordinary_institution(
        snapshot.institution.institution_code,
        &snapshot.cid_number,
    ) || input.threshold.is_some_and(|value| value < 2)
        || (input.threshold.is_none() && input.permission_changes.is_empty())
        || input.permission_changes.len()
            > entitybase::institution_governance::MAX_POLICY_ROLES as usize
    {
        return Err("机构政策或阈值无效".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut changes = Vec::new();
    for change in &input.permission_changes {
        if !change.role_code.starts_with("R_")
            || !snapshot.roles.contains_key(change.role_code.as_bytes())
            || !seen.insert(&change.role_code)
            || change.permissions.is_empty()
            || change.permissions.len()
                > entitybase::institution_governance::MAX_POLICY_PERMISSIONS as usize
        {
            return Err("只能替换本机构动态岗位的非空原生权限".into());
        }
        let mut permissions = Vec::new();
        let mut keys = std::collections::BTreeSet::new();
        for value in &change.permissions {
            let value = permission(value)?;
            if !keys.insert(value.encode()) {
                return Err("权限重复".into());
            }
            permissions.push(value);
        }
        changes.push(entitybase::RolePermissionChange {
            role_code: change.role_code.as_bytes().to_vec(),
            permissions,
        });
    }
    Ok(entitybase::InstitutionPolicyAction {
        permission_changes: changes,
        threshold: input.threshold,
    })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareInstitutionChainOutput {
    pub request_id: String,
    pub cid_number: String,
    pub chain_action: u16,
    pub call_data_hex: String,
    pub sign_request: String,
    pub expires_at: i64,
}
pub fn code_bytes(code: &str) -> Result<[u8; 4], axum::response::Response> {
    primitives::cid::code::institution_code_from_str(code)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, 1001, "机构码无效"))
}

fn permission(
    p: &InstitutionRolePermissionInput,
) -> Result<entitybase::RolePermissionSpec, String> {
    if p.module_tag.is_empty() || p.module_tag.len() > 32 {
        return Err("业务模块标签无效".into());
    }
    let operation = match p.operation.as_str() {
        "PROPOSE" => entitybase::RolePermissionOperation::Propose,
        "VOTE" => entitybase::RolePermissionOperation::Vote,
        _ => return Err("业务操作无效".into()),
    };
    Ok(entitybase::RolePermissionSpec {
        business_action_id: entitybase::BusinessActionId {
            module_tag: p.module_tag.as_bytes().to_vec(),
            action_code: p.action_code,
        },
        operation,
    })
}
fn targets(
    snapshot: &roles::RegistrationSnapshot,
    role: &[u8],
    values: &[InstitutionAssignmentTargetInput],
    registration: bool,
) -> Result<Vec<entitybase::InstitutionAssignmentTarget>, String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut result = Vec::new();
    for v in values {
        if !roles::person_allowed(v.person_cid.as_bytes(), snapshot.owner)
            || !seen.insert(v.person_cid.clone())
        {
            return Err("人员CID类别无效或同岗位人员重复".into());
        }
        if let Some(a) =
            snapshot.retained_assignment(role, v.person_cid.as_bytes(), v.term_start, v.term_end)
        {
            result.push(a);
            continue;
        }
        result.push(entitybase::InstitutionAssignmentTarget {
            person_cid: v.person_cid.as_bytes().to_vec(),
            term_start: v.term_start,
            term_end: v.term_end,
            assignment_source: if registration {
                entitybase::InstitutionAssignmentSource::Registry
            } else {
                entitybase::InstitutionAssignmentSource::InstitutionGovernance
            },
            assignment_source_ref: Vec::new(),
            assignment_status: entitybase::InstitutionAssignmentStatus::Active,
        });
    }
    Ok(result)
}
/// 任职载荷只接受人员CID；原任职的任期和来源在未变化时原样保留。
pub fn parse_role_action(
    snapshot: &roles::RegistrationSnapshot,
    mutations: &[InstitutionRoleMutationInput],
    changes: &[InstitutionAssignmentChangeInput],
    registration: bool,
) -> Result<
    (
        Vec<entitybase::InstitutionRoleMutation>,
        Vec<entitybase::InstitutionRoleAssignmentChange>,
    ),
    String,
> {
    let mut output = Vec::new();
    let mut used = std::collections::BTreeSet::new();
    for m in mutations {
        let code = m.role_code.as_deref().unwrap_or_default().as_bytes();
        match m.mutation.as_str() {
            "CREATE" => {
                if m.active.is_some()
                    || !code.is_empty()
                    || m.role_name
                        .as_deref()
                        .is_none_or(|n| n.trim().is_empty() || n.len() > 128)
                    || m.permissions.is_empty()
                    || !matches!(m.seats,Some(n) if n>0 && n<=snapshot.max_seats)
                {
                    return Err("新岗位须填写名称、正整数席位和权限，岗位码由链生成".into());
                }
                if m.assignments.len() > m.seats.unwrap_or(0) as usize {
                    return Err("任职人数超过岗位席位".into());
                }
                let mut keys = std::collections::BTreeSet::new();
                let mut perms = Vec::new();
                for p in &m.permissions {
                    let p = permission(p)?;
                    if !keys.insert(p.encode()) {
                        return Err("权限重复".into());
                    }
                    perms.push(p);
                }
                let rows = targets(snapshot, &[], &m.assignments, registration)?;
                validate_terms(m.term_required, &rows)?;
                output.push(entitybase::InstitutionRoleMutation::Create {
                    role_name: m.role_name.as_ref().unwrap().trim().as_bytes().to_vec(),
                    term_required: m.term_required,
                    seats: m.seats.unwrap(),
                    permissions: perms,
                    assignments: rows,
                });
            }
            "RENAME" | "DELETE" => {
                if !used.insert(code.to_vec())
                    || m.active.is_some()
                    || !m.permissions.is_empty()
                    || !m.assignments.is_empty()
                    || m.seats.is_some()
                    || m.term_required
                {
                    return Err("既有岗位操作夹带了其它变更".into());
                }
                let role = snapshot.roles.get(code).ok_or("岗位不存在")?;
                if role.role_status != entitybase::InstitutionRoleStatus::Active
                    || primitives::institution_constraints::is_legal_representative_role(code)
                    || primitives::governance_skeleton::fixed_role_assignment_bounds_by_identity(
                        snapshot.institution.institution_code,
                        &snapshot.cid_number,
                        code,
                    )
                    .is_some()
                {
                    return Err("固定或无效岗位不能改写".into());
                }
                if m.mutation == "DELETE" {
                    if m.role_name.is_some() {
                        return Err("删除不能夹带名称".into());
                    }
                    output.push(entitybase::InstitutionRoleMutation::Delete {
                        role_code: code.to_vec(),
                    });
                } else {
                    let name = m
                        .role_name
                        .as_deref()
                        .filter(|n| !n.trim().is_empty() && n.len() <= 128)
                        .ok_or("岗位名称无效")?;
                    output.push(entitybase::InstitutionRoleMutation::Rename {
                        role_code: code.to_vec(),
                        role_name: name.trim().as_bytes().to_vec(),
                    });
                }
            }
            "CONFIGURE" => {
                if registration
                    || !entitybase::business_action::is_ordinary_institution(
                        snapshot.institution.institution_code,
                        &snapshot.cid_number,
                    )
                    || !code.starts_with(b"R_")
                    || !used.insert(code.to_vec())
                    || m.role_name.is_some()
                    || !m.permissions.is_empty()
                    || !m.assignments.is_empty()
                    || m.active.is_none()
                    || !matches!(m.seats, Some(n) if n > 0 && n <= snapshot.max_seats)
                {
                    return Err("只能配置本机构动态岗位的席位、任期和状态".into());
                }
                snapshot.roles.get(code).ok_or("岗位不存在")?;
                output.push(entitybase::InstitutionRoleMutation::Configure {
                    role_code: code.to_vec(),
                    term_required: m.term_required,
                    seats: m.seats.unwrap(),
                    role_status: if m.active == Some(true) {
                        entitybase::InstitutionRoleStatus::Active
                    } else {
                        entitybase::InstitutionRoleStatus::Inactive
                    },
                });
            }
            _ => return Err("岗位操作无效".into()),
        }
    }
    let mut assignments = Vec::new();
    let mut keys = std::collections::BTreeSet::new();
    for c in changes {
        let mut role = snapshot
            .roles
            .get(c.role_code.as_bytes())
            .ok_or("任职岗位不存在")?
            .clone();
        for mutation in &output {
            if let entitybase::InstitutionRoleMutation::Configure {
                role_code,
                term_required,
                seats,
                role_status,
            } = mutation
            {
                if role_code == c.role_code.as_bytes() {
                    role.term_required = *term_required;
                    role.seats = *seats;
                    role.role_status = *role_status;
                }
            }
        }
        if !keys.insert(c.role_code.clone()) || c.assignments.len() > role.seats as usize {
            return Err("任职岗位重复或超过席位".into());
        }
        let configuring = output.iter().any(|mutation| matches!(mutation, entitybase::InstitutionRoleMutation::Configure { role_code, .. } if role_code == c.role_code.as_bytes()));
        if (role.role_status != entitybase::InstitutionRoleStatus::Active
            && (!configuring || !c.assignments.is_empty()))
            || used.contains(c.role_code.as_bytes())
                && mutations
                    .iter()
                    .any(|m| m.mutation == "DELETE" && m.role_code.as_deref() == Some(&c.role_code))
        {
            return Err("任职岗位无效或已删除".into());
        }
        let code = c.role_code.as_bytes();
        let fixed = primitives::governance_skeleton::fixed_role_assignment_bounds_by_identity(
            snapshot.institution.institution_code,
            &snapshot.cid_number,
            code,
        );
        let bounds = primitives::institution_constraints::member_composition_by_identity(
            snapshot.institution.institution_code,
            &snapshot.cid_number,
        )
        .filter(|spec| spec.role_code == code)
        .map(|spec| (spec.min_members, spec.max_members))
        .or(fixed);
        if bounds.is_some_and(|(min, max)| {
            c.assignments.len() < min as usize || c.assignments.len() > max as usize
        }) {
            return Err("任职人数不符合规则固定岗位席位".into());
        }
        let rows = targets(snapshot, code, &c.assignments, registration)?;
        validate_terms(role.term_required, &rows)?;
        let protected = primitives::institution_constraints::is_legal_representative_role(code)
            || fixed.is_some()
            || (primitives::cid::china::citizenchain::is_citizenchain_foundation_identity(
                snapshot.institution.institution_code,
                &snapshot.cid_number,
            ) && primitives::cid::china::citizenchain::fixed_role(code).is_some());
        if registration && protected {
            let previous = snapshot
                .assignments
                .iter()
                .find(|(r, _)| r == code)
                .map(|(_, a)| a.as_slice())
                .unwrap_or_default();
            if rows.len() != previous.len()
                || rows.iter().any(|a| {
                    snapshot
                        .retained_assignment(code, &a.person_cid, a.term_start, a.term_end)
                        .as_ref()
                        != Some(a)
                })
            {
                return Err("登记不能修改受保护岗位任职".into());
            }
        }
        assignments.push(entitybase::InstitutionRoleAssignmentChange {
            role_code: c.role_code.as_bytes().to_vec(),
            assignments: rows,
        });
    }
    // 未显式更换的任职也须满足目标属性；不能靠缩席或停用隐式丢弃人员。
    for mutation in &output {
        if let entitybase::InstitutionRoleMutation::Configure {
            role_code,
            term_required,
            seats,
            role_status,
        } = mutation
        {
            if assignments
                .iter()
                .any(|change| &change.role_code == role_code)
            {
                continue;
            }
            let existing = snapshot
                .assignments
                .iter()
                .find(|(code, _)| code == role_code)
                .map(|(_, rows)| rows.as_slice())
                .unwrap_or_default();
            if existing.len() > *seats as usize
                || (*role_status == entitybase::InstitutionRoleStatus::Inactive
                    && !existing.is_empty())
            {
                return Err("目标岗位属性要求同案调整任职".into());
            }
            let targets = existing
                .iter()
                .map(|a| entitybase::InstitutionAssignmentTarget {
                    person_cid: a.person_cid.clone(),
                    term_start: a.term_start,
                    term_end: a.term_end,
                    assignment_source: a.assignment_source,
                    assignment_source_ref: a.assignment_source_ref.clone(),
                    assignment_status: a.assignment_status,
                })
                .collect::<Vec<_>>();
            validate_terms(*term_required, &targets)?;
        }
    }
    Ok((output, assignments))
}
/// 法代资料与唯一LR任职必须在同一治理动作中一致，不能只更改其中一侧。
pub(crate) fn validate_legal_change(
    snapshot: &roles::RegistrationSnapshot,
    changes: &[entitybase::InstitutionRoleAssignmentChange],
    change: Option<&entitybase::InstitutionLegalRepresentativeChange>,
) -> Result<(), String> {
    let expected = match change {
        Some(entitybase::InstitutionLegalRepresentativeChange::Set { person_cid, .. }) => {
            Some(person_cid.as_slice())
        }
        Some(entitybase::InstitutionLegalRepresentativeChange::Clear) => None,
        None => snapshot
            .institution
            .legal_representative
            .as_ref()
            .map(|r| r.person_cid.as_slice()),
    };
    let persons = if let Some(change) = changes.iter().find(|c| c.role_code == b"LR") {
        change
            .assignments
            .iter()
            .map(|a| a.person_cid.as_slice())
            .collect::<Vec<_>>()
    } else {
        snapshot
            .assignments
            .iter()
            .find(|(r, _)| r == b"LR")
            .map(|(_, a)| {
                a.iter()
                    .map(|a| a.person_cid.as_slice())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    if match expected {
        Some(person) => persons.len() != 1 || persons[0] != person,
        None => !persons.is_empty(),
    } {
        return Err("法代资料和唯一LR任职不一致".into());
    }
    Ok(())
}
fn validate_terms(
    required: bool,
    rows: &[entitybase::InstitutionAssignmentTarget],
) -> Result<(), String> {
    if rows.iter().any(|a| {
        if required {
            a.term_start == 0 || a.term_end < a.term_start
        } else {
            a.term_start != 0 || a.term_end != 0
        }
    }) {
        return Err("任期与岗位规则不一致".into());
    }
    Ok(())
}
pub async fn validate_action_people(
    snapshot: &roles::RegistrationSnapshot,
    mutations: &[entitybase::InstitutionRoleMutation],
    changes: &[entitybase::InstitutionRoleAssignmentChange],
) -> Result<(), String> {
    let view =
        crate::chain_runtime::FinalizedChainView::connect_at(Some(snapshot.block_hash)).await?;
    let mut persons = std::collections::BTreeSet::new();
    for m in mutations {
        if let entitybase::InstitutionRoleMutation::Create { assignments, .. } = m {
            for a in assignments {
                persons.insert(a.person_cid.clone());
            }
        }
    }
    for c in changes {
        for a in &c.assignments {
            persons.insert(a.person_cid.clone());
        }
    }
    for p in persons {
        let p = std::str::from_utf8(&p).map_err(|_| "人员CID无效")?;
        if crate::chain_identity::read_active_cid_account_id_at(&view.storage(), p)
            .await?
            .is_none()
        {
            return Err("任职人员没有有效当前绑定".into());
        }
    }
    Ok(())
}
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
        .get("actor_cid_number")
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
) -> axum::response::Response {
    if input.policy.is_some() {
        return prepare_policy(&state, &headers, &input).await;
    }
    let result=async {
        let ctx=require_admin_any(&state,&headers)?;
        if input.cid_number!=ctx.institution_cid_number || input.proposer_role_code!=ctx.role_code {return Err(api_error(StatusCode::FORBIDDEN,1003,"只能使用当前所选机构和岗位"));}
        let snapshot=roles::fetch_registration_snapshot(&input.cid_number).await.map_err(read_error)?;
        if input.role_mutations.iter().any(|m| m.mutation == "CONFIGURE") && !roles::supports_policy(&snapshot).await.map_err(read_error)? {
            return Err(input_error("当前Runtime不支持岗位属性配置".into()));
        }
        let identity=crate::chain_runtime::identity_from_binding_parts(&ctx.institution_code,Some(&ctx.institution_cid_number)).map_err(read_error)?;
        let view=crate::chain_runtime::FinalizedChainView::connect_at(Some(snapshot.block_hash)).await.map_err(read_error)?;
        let active=roles::fetch_active_assignments_at(&identity,&view,Some(&ctx.account_id)).await.map_err(read_error)?;
        let state_view=snapshot.governance_state(&active).map_err(read_error)?;
        if !state_view.proposer_roles.iter().any(|r|r.role_code==ctx.role_code){return Err(api_error(StatusCode::FORBIDDEN,1003,"岗位没有治理发起权限"));}
        let (role_mutations,assignment_changes)=parse_role_action(&snapshot,&input.role_mutations,&input.assignment_changes,false).map_err(input_error)?;
        validate_action_people(&snapshot,&role_mutations,&assignment_changes).await.map_err(input_error)?;
        let legal_representative_change=if input.clear_legal_representative {
            if input.representative_cid.is_some()||input.representative_family_name.is_some()||input.representative_given_name.is_some(){return Err(input_error("法代任免内容冲突".into()));}Some(entitybase::InstitutionLegalRepresentativeChange::Clear)
        }else if let Some(person)=&input.representative_cid {
            if !roles::person_allowed(person.as_bytes(),snapshot.owner)||crate::chain_identity::read_active_cid_account_id_at(&view.storage(),person).await.map_err(read_error)?.is_none(){return Err(input_error("法代人员CID或绑定无效".into()));}
            let family=input.representative_family_name.as_deref().filter(|n|!n.trim().is_empty()).ok_or_else(||input_error("法代姓不能为空".into()))?;
            let given=input.representative_given_name.as_deref().filter(|n|!n.trim().is_empty()).ok_or_else(||input_error("法代名不能为空".into()))?;
            if family.len()>128||given.len()>128{return Err(input_error("法代姓名超过链上长度限制".into()));}
            if let Some(record)=state.db.find_governance_citizen_by_cid(person).map_err(|_|api_error(StatusCode::INTERNAL_SERVER_ERROR,5001,"人员资料查询失败"))? {
                if !record.family_name.is_empty()&&!record.given_name.is_empty()&&(record.family_name!=family||record.given_name!=given){return Err(input_error("法代姓名与人员登记资料不一致".into()));}
            }
            Some(entitybase::InstitutionLegalRepresentativeChange::Set{family_name:family.trim().as_bytes().to_vec(),given_name:given.trim().as_bytes().to_vec(),person_cid:person.as_bytes().to_vec()})
        }else{None};
        if role_mutations.is_empty()&&assignment_changes.is_empty()&&legal_representative_change.is_none(){return Err(input_error("治理内容不能为空".into()));}
        validate_legal_change(&snapshot,&assignment_changes,legal_representative_change.as_ref()).map_err(input_error)?;
        let action=entitybase::InstitutionGovernanceAction{role_mutations,assignment_changes,legal_representative_change};
        let chain=encode_propose_institution_governance(&ProposeInstitutionGovernanceArgs{is_private:snapshot.owner==crate::chain_runtime::EntityPallet::Private,cid_number:input.cid_number.as_bytes().to_vec(),governance_action:action.encode(),institution_code:snapshot.institution.institution_code,actor_cid_number:ctx.institution_cid_number.as_bytes().to_vec(),proposer_role_code:ctx.role_code.as_bytes().to_vec()});
        let passkey=crate::authentication::passkey::require_passkey_assertion(&state,&headers,&ctx.account_id)?;
        build_chain_sign_output(&state,InstitutionChainSignRequest{account_id:&ctx.account_id,cid_number:&input.cid_number,purpose:PURPOSE_INSTITUTION_GOVERNANCE,call_data:chain.call_data,chain_action:chain.action,context:serde_json::json!({"cid_number":input.cid_number,"actor_cid_number":ctx.institution_cid_number,"actor_role_code":ctx.role_code,"op":"governance"})},&passkey).await
    }.await;
    match result {
        Ok(data) => Json(ApiResponse {
            code: 0,
            message: "ok".into(),
            data,
        })
        .into_response(),
        Err(error) => error,
    }
}

/// 原生政策沿用治理准备入口；原状态指纹必须与审阅时一致，不自动刷新签名内容。
async fn prepare_policy(
    state: &AppState,
    headers: &HeaderMap,
    input: &PrepareInstitutionGovernanceInput,
) -> axum::response::Response {
    let result = async {
        let ctx = require_admin_any(state, headers)?;
        if input.cid_number != ctx.institution_cid_number || input.proposer_role_code != ctx.role_code {
            return Err(api_error(StatusCode::FORBIDDEN, 1003, "只能使用当前所选机构和岗位"));
        }
        if !input.role_mutations.is_empty() || !input.assignment_changes.is_empty() || input.clear_legal_representative
            || input.representative_cid.is_some() || input.representative_family_name.is_some() || input.representative_given_name.is_some()
        { return Err(input_error("政策不能夹带岗位人事变更".into())); }
        let policy = input.policy.as_ref().ok_or_else(|| input_error("政策内容缺失".into()))?;
        let snapshot = roles::fetch_registration_snapshot(&input.cid_number).await.map_err(read_error)?;
        if !roles::supports_policy(&snapshot).await.map_err(read_error)? {
            return Err(input_error("当前Runtime或机构不支持原生政策".into()));
        }
        let hash = policy.expected_state_hash.strip_prefix("0x").ok_or_else(|| input_error("状态指纹无效".into()))?;
        let expected: [u8; 32] = hex::decode(hash).map_err(|_| input_error("状态指纹无效".into()))?.try_into().map_err(|_| input_error("状态指纹无效".into()))?;
        if expected != snapshot.state_hash() {
            return Err(api_error(StatusCode::CONFLICT, 1001, "机构状态已变化，请重新审阅"));
        }
        let identity = crate::chain_runtime::identity_from_binding_parts(&ctx.institution_code, Some(&ctx.institution_cid_number)).map_err(read_error)?;
        let view = crate::chain_runtime::FinalizedChainView::connect_at(Some(snapshot.block_hash)).await.map_err(read_error)?;
        let active = roles::fetch_active_assignments_at(&identity, &view, Some(&ctx.account_id)).await.map_err(read_error)?;
        if !snapshot.governance_state(&active).map_err(read_error)?.proposer_roles.iter().any(|role| role.role_code == ctx.role_code) {
            return Err(api_error(StatusCode::FORBIDDEN, 1003, "岗位没有治理发起权限"));
        }
        let action = parse_policy_action(&snapshot, policy).map_err(input_error)?;
        let chain = super::encode_propose_institution_policy(&super::ProposeInstitutionPolicyArgs {
            is_private: snapshot.owner == crate::chain_runtime::EntityPallet::Private,
            cid_number: input.cid_number.as_bytes().to_vec(), action,
            proposer_role_code: ctx.role_code.as_bytes().to_vec(), expected_state_hash: expected,
        });
        let passkey = crate::authentication::passkey::require_passkey_assertion(state, headers, &ctx.account_id)?;
        build_chain_sign_output(state, InstitutionChainSignRequest {
            account_id: &ctx.account_id, cid_number: &input.cid_number, purpose: PURPOSE_INSTITUTION_GOVERNANCE,
            call_data: chain.call_data, chain_action: chain.action,
            context: serde_json::json!({"cid_number":input.cid_number,"actor_cid_number":ctx.institution_cid_number,"actor_role_code":ctx.role_code,"op":"policy"}),
        }, &passkey).await
    }.await;
    match result {
        Ok(data) => Json(ApiResponse {
            code: 0,
            message: "ok".into(),
            data,
        })
        .into_response(),
        Err(error) => error,
    }
}
fn input_error(error: String) -> axum::response::Response {
    api_error(StatusCode::BAD_REQUEST, 1001, &error)
}
fn read_error(error: String) -> axum::response::Response {
    tracing::warn!(%error,"read finalized institution roles failed");
    api_error(StatusCode::BAD_GATEWAY, 5002, "机构链状态读取失败")
}
pub(super) fn can_read_institution_governance(
    ctx: &crate::authentication::login::AdminAuthContext,
    code: &str,
    cid: &str,
    province: &str,
    city: &str,
    town: &str,
) -> bool {
    (ctx.institution_code == code && ctx.institution_cid_number == cid)
        || crate::authorization::rules::get_visible_scope(ctx)
            .includes_institution(cid, province, city, town)
}
pub(crate) async fn get_governance_state(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(cid): Path<String>,
) -> axum::response::Response {
    let result = async {
        let ctx = require_admin_any(&state, &headers)?;
        let inst = state
            .db
            .get_institution_with_accounts(&cid)
            .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, 5001, "机构查询失败"))?
            .ok_or_else(|| api_error(StatusCode::NOT_FOUND, 1004, "机构不存在"))?
            .0;
        if !can_read_institution_governance(
            &ctx,
            &inst.institution_code,
            &cid,
            &inst.province_name,
            &inst.city_name,
            &inst.town_name,
        ) {
            return Err(api_error(StatusCode::FORBIDDEN, 1003, "机构不在可见范围"));
        }
        let view = crate::chain_runtime::FinalizedChainView::connect()
            .await
            .map_err(read_error)?;
        let snapshot = roles::registration_snapshot_at(&cid, &view)
            .await
            .map_err(read_error)?;
        let identity =
            crate::chain_runtime::identity_from_binding_parts(&inst.institution_code, Some(&cid))
                .map_err(read_error)?;
        let active = if cid == ctx.institution_cid_number {
            roles::fetch_active_assignments_at(&identity, &view, Some(&ctx.account_id))
                .await
                .map_err(read_error)?
                .into_iter()
                .filter(|a| a.role_code == ctx.role_code)
                .collect()
        } else {
            Vec::new()
        };
        let mut data = snapshot.governance_state(&active).map_err(read_error)?;
        data.policy_available = roles::supports_policy(&snapshot)
            .await
            .map_err(read_error)?;
        if matches!(ctx.institution_code.as_str(), "FRG" | "CREG") {
            let actor = crate::chain_runtime::identity_from_binding_parts(
                &ctx.institution_code,
                Some(&ctx.institution_cid_number),
            )
            .map_err(read_error)?;
            data.registration_roles = roles::fetch_registration_roles_at(
                &actor,
                &ctx.account_id,
                &cid,
                view.block_hash.0,
            )
            .await
            .map_err(read_error)?
            .into_iter()
            .filter(|r| r.role_code == ctx.role_code)
            .collect();
        }
        Ok(data)
    }
    .await;
    match result {
        Ok(data) => Json(ApiResponse {
            code: 0,
            message: "ok".into(),
            data,
        })
        .into_response(),
        Err(error) => error,
    }
}
