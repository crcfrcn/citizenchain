//! 机构岗位与管理员任职的链上只读适配层。
//!
//! `admins` pallet 保存管理员名册；有管理员 CID 时，实际签名账户由
//! `CitizenIdentity::AccountIdByCid` 当前绑定决定。岗位定义和任职关系只从
//! `PublicManage` / `PrivateManage` 读取。本模块在同一个 finalized 区块合并三类真源，
//! 不参与投票或任免业务。

use std::collections::{BTreeMap, HashMap, HashSet};

use codec::{Decode, DecodeAll, Encode};
use subxt::dynamic;

use crate::chain_runtime::{
    fetch_active_admins_onchain_at, AdminPallet, FinalizedChainView, NodeInstitutionIdentity,
};

const ROLE_STATUS_ACTIVE: u8 = 0;
const ASSIGNMENT_STATUS_ACTIVE: u8 = 0;

#[derive(Debug, Clone, Decode, Encode)]
struct RawInstitutionRole {
    cid_number: Vec<u8>,
    role_code: Vec<u8>,
    role_name: Vec<u8>,
    term_required: bool,
    role_status: u8,
}

#[derive(Debug, Clone, Decode, Encode)]
struct RawInstitutionAssignment {
    cid_number: Vec<u8>,
    account_id: [u8; 32],
    role_code: Vec<u8>,
    term_start: u32,
    term_end: u32,
    assignment_source: u8,
    assignment_source_ref: Vec<u8>,
    assignment_status: u8,
}

#[derive(Debug, Clone)]
struct ActiveAdminRoleProjection {
    account_id: String,
    family_name: String,
    given_name: String,
}

/// 管理员人员记录和一条可选有效任职的联合投影。
#[derive(Debug, Clone)]
pub struct InstitutionAssignmentView {
    pub account_id: String,
    pub family_name: String,
    pub given_name: String,
    pub role_code: String,
    pub role_name: String,
    pub term_required: bool,
    pub term_start: u32,
    pub term_end: u32,
    pub assignment_source: u8,
    pub assignment_source_label: String,
    pub assignment_source_ref: String,
}

/// 联邦注册局单省专员岗位的任职投影。
#[derive(Debug, Clone)]
pub struct FederalRegistryProvinceAssignments {
    pub province_code: [u8; 2],
    pub province_name: String,
    pub assignments: Vec<InstitutionAssignmentView>,
}

fn assignment_source_label(source: u8) -> &'static str {
    match source {
        0 => "创世",
        1 => "注册局",
        2 => "普选",
        3 => "互选",
        4 => "提名任免",
        5 => "机构内部治理",
        _ => "",
    }
}

fn assignment_is_effective(
    role: &RawInstitutionRole,
    assignment: &RawInstitutionAssignment,
    current_day: u32,
) -> bool {
    if assignment.assignment_status != ASSIGNMENT_STATUS_ACTIVE {
        return false;
    }
    if !role.term_required {
        return assignment.term_start == 0 && assignment.term_end == 0;
    }
    assignment.term_start > 0
        && assignment.term_start <= current_day
        && current_day <= assignment.term_end
}

fn entity_pallet_name(admin_pallet: AdminPallet) -> &'static str {
    match admin_pallet {
        AdminPallet::PublicAdmins => "PublicManage",
        AdminPallet::PrivateAdmins => "PrivateManage",
    }
}

fn role_code_from_storage_key(
    raw: &[u8],
    pallet: &str,
    entry: &str,
    cid: &[u8],
) -> Result<Vec<u8>, String> {
    let encoded = cid.to_vec().encode();
    let mut prefix = sp_crypto_hashing::twox_128(pallet.as_bytes()).to_vec();
    prefix.extend_from_slice(&sp_crypto_hashing::twox_128(entry.as_bytes()));
    if !raw.starts_with(&prefix) {
        return Err("institution role key namespace mismatch".into());
    }
    let end = 48 + encoded.len();
    if raw.len() < end + 17
        || raw[32..48] != sp_crypto_hashing::blake2_128(&encoded)
        || raw[48..end] != encoded
    {
        return Err("institution role key CID mismatch".into());
    }
    let role = Vec::<u8>::decode_all(&mut &raw[end + 16..]).map_err(|e| e.to_string())?;
    if role.is_empty()
        || role.len() > 64
        || role.encode() != raw[end + 16..]
        || raw[end..end + 16] != sp_crypto_hashing::blake2_128(&role.encode())
    {
        return Err("institution role key is not canonical".into());
    }
    Ok(role)
}
fn decode_stored_role(
    mut raw: &[u8],
    cid: &[u8],
    code: Vec<u8>,
) -> Result<RawInstitutionRole, String> {
    let v = entitybase::StoredInstitutionRole::<Vec<u8>>::decode_all(&mut raw)
        .map_err(|e| e.to_string())?;
    Ok(RawInstitutionRole {
        cid_number: cid.to_vec(),
        role_code: code,
        role_name: v.role_name,
        term_required: v.term_required,
        role_status: v.role_status as u8,
    })
}
fn restore_assignment(
    v: entitybase::StoredInstitutionAdminAssignment<[u8; 32], Vec<u8>>,
    cid: &[u8],
    code: &[u8],
) -> RawInstitutionAssignment {
    RawInstitutionAssignment {
        cid_number: cid.to_vec(),
        account_id: v.account_id,
        role_code: code.to_vec(),
        term_start: v.term_start,
        term_end: v.term_end,
        assignment_source: v.assignment_source as u8,
        assignment_source_ref: v.assignment_source_ref,
        assignment_status: v.assignment_status as u8,
    }
}

async fn read_roles_and_assignments(
    finalized: &FinalizedChainView,
    cid_number: &[u8],
    admin_pallet: AdminPallet,
) -> Result<(Vec<RawInstitutionRole>, Vec<RawInstitutionAssignment>), String> {
    let storage = finalized.storage();
    let pallet = entity_pallet_name(admin_pallet);

    // 从已核验的Concat存储键恢复完整CID和岗位，不在值中重复保存。
    let mut roles = Vec::new();
    let mut role_iter = storage
        .iter(dynamic::storage(
            pallet,
            "InstitutionRoles",
            vec![dynamic::Value::from_bytes(cid_number)],
        ))
        .await
        .map_err(|e| format!("iterate {pallet} InstitutionRoles failed: {e}"))?;
    while let Some(item) = role_iter.next().await {
        let item = item.map_err(|e| format!("read {pallet} InstitutionRoles failed: {e}"))?;
        let code = role_code_from_storage_key(&item.key_bytes,pallet,"InstitutionRoles",cid_number)?;
        let role = decode_stored_role(item.value.encoded(), cid_number, code)?;
        if role.cid_number == cid_number {
            roles.push(role);
        }
    }

    let mut assignments = Vec::new();
    let mut assignment_iter = storage
        .iter(dynamic::storage(
            pallet,
            "InstitutionRoleAssignments",
            vec![dynamic::Value::from_bytes(cid_number)],
        ))
        .await
        .map_err(|e| format!("iterate {pallet} InstitutionRoleAssignments failed: {e}"))?;
    while let Some(item) = assignment_iter.next().await {
        let item =
            item.map_err(|e| format!("read {pallet} InstitutionRoleAssignments failed: {e}"))?;
        let mut encoded = item.value.encoded();
        let code = role_code_from_storage_key(&item.key_bytes,pallet,"InstitutionRoleAssignments",cid_number)?;
        let role_assignments = Vec::<entitybase::StoredInstitutionAdminAssignment<[u8;32],Vec<u8>>>::decode_all(&mut encoded)
            .map_err(|e| format!("decode {pallet} InstitutionRoleAssignments failed: {e}"))?
            .into_iter().map(|v| restore_assignment(v,cid_number,&code)).collect::<Vec<_>>();
        assignments.extend(
            role_assignments
                .into_iter()
                .filter(|assignment| assignment.cid_number == cid_number),
        );
    }
    Ok((roles, assignments))
}

fn merge_active_assignments(
    roles: Vec<RawInstitutionRole>,
    assignments: Vec<RawInstitutionAssignment>,
    active_admins: &HashMap<[u8; 32], ActiveAdminRoleProjection>,
    current_day: u32,
) -> Result<Vec<InstitutionAssignmentView>, String> {
    let active_roles: HashMap<Vec<u8>, RawInstitutionRole> = roles
        .into_iter()
        .filter(|role| role.role_status == ROLE_STATUS_ACTIVE)
        .map(|role| (role.role_code.clone(), role))
        .collect();
    let mut views = Vec::new();
    for assignment in assignments {
        if !active_admins.contains_key(&assignment.account_id) {
            continue;
        }
        let Some(role) = active_roles.get(&assignment.role_code) else {
            return Err(
                "active institution assignment references a missing active role".to_string(),
            );
        };
        if !assignment_is_effective(role, &assignment, current_day) {
            continue;
        }
        let admin = &active_admins[&assignment.account_id];
        views.push(InstitutionAssignmentView {
            account_id: admin.account_id.clone(),
            family_name: admin.family_name.clone(),
            given_name: admin.given_name.clone(),
            role_code: String::from_utf8_lossy(&assignment.role_code).to_string(),
            role_name: String::from_utf8_lossy(&role.role_name).to_string(),
            term_required: role.term_required,
            term_start: assignment.term_start,
            term_end: assignment.term_end,
            assignment_source: assignment.assignment_source,
            assignment_source_label: assignment_source_label(assignment.assignment_source)
                .to_string(),
            assignment_source_ref: String::from_utf8_lossy(&assignment.assignment_source_ref)
                .to_string(),
        });
    }
    views.sort_by(|left, right| {
        left.role_code
            .cmp(&right.role_code)
            .then(left.account_id.cmp(&right.account_id))
    });
    Ok(views)
}

/// 合并本机构管理员人员集合与有效任职；没有岗位的管理员仍返回一条空任职投影。
pub async fn fetch_active_assignments_onchain(
    identity: &NodeInstitutionIdentity,
) -> Result<Option<Vec<InstitutionAssignmentView>>, String> {
    let finalized = FinalizedChainView::connect().await?;
    fetch_active_assignments_at(identity, &finalized).await
}

async fn fetch_active_assignments_at(
    identity: &NodeInstitutionIdentity,
    finalized: &FinalizedChainView,
) -> Result<Option<Vec<InstitutionAssignmentView>>, String> {
    let cid_number = identity.cid_number.as_str();
    let Some(admins) = fetch_active_admins_onchain_at(identity, finalized).await? else {
        return Ok(None);
    };
    let active_admins: HashMap<[u8; 32], ActiveAdminRoleProjection> = admins
        .iter()
        .map(|admin| {
            Ok((
                admin.role_account_id,
                ActiveAdminRoleProjection {
                    account_id: admin.account_id.clone(),
                    family_name: admin.family_name.clone(),
                    given_name: admin.given_name.clone(),
                },
            ))
        })
        .collect::<Result<_, String>>()?;
    let current_day = finalized.current_day().await?;

    for pallet in &identity.admin_pallets {
        let (roles, assignments) =
            read_roles_and_assignments(finalized, cid_number.as_bytes(), *pallet).await?;
        if roles.is_empty() {
            continue;
        }
        let mut views = merge_active_assignments(roles, assignments, &active_admins, current_day)?;
        let covered = views
            .iter()
            .map(|view| view.account_id.clone())
            .collect::<HashSet<_>>();
        for admin in active_admins.values() {
            if covered.contains(&admin.account_id) {
                continue;
            }
            views.push(InstitutionAssignmentView {
                account_id: admin.account_id.clone(),
                family_name: admin.family_name.clone(),
                given_name: admin.given_name.clone(),
                role_code: String::new(),
                role_name: String::new(),
                term_required: false,
                term_start: 0,
                term_end: 0,
                assignment_source: 0,
                assignment_source_label: String::new(),
                assignment_source_ref: String::new(),
            });
        }
        views.sort_by(|left, right| {
            left.account_id
                .cmp(&right.account_id)
                .then(left.role_code.cmp(&right.role_code))
        });
        return Ok(Some(views));
    }
    Err("institution roles not found in matching entity pallet".to_string())
}

// 权限候选只看有效岗位；已停用/删除岗位的历史任职不阻塞其它可用岗位。
fn merge_authorization_assignments(
    roles: Vec<RawInstitutionRole>,
    assignments: Vec<RawInstitutionAssignment>,
    active_admins: &HashMap<[u8; 32], ActiveAdminRoleProjection>,
    current_day: u32,
) -> Result<Vec<InstitutionAssignmentView>, String> {
    let active_roles: HashSet<_> = roles
        .iter()
        .filter(|role| role.role_status == ROLE_STATUS_ACTIVE)
        .map(|role| role.role_code.clone())
        .collect();
    let assignments = assignments
        .into_iter()
        .filter(|assignment| active_roles.contains(&assignment.role_code))
        .collect();
    merge_active_assignments(roles, assignments, active_admins, current_day)
}

type BusinessPermission = entitybase::RoleBusinessPermission<Vec<u8>, Vec<u8>, Vec<u8>>;

fn permission_allows_proposal(
    permissions: &[BusinessPermission],
    cid_number: &[u8],
    role_code: &[u8],
    action: &entitybase::BusinessActionId<Vec<u8>>,
) -> bool {
    permissions.iter().any(|permission| {
        permission.role_subject.cid_number == cid_number
            && permission.role_subject.role_code == role_code
            && permission.business_action_id == *action
            && permission.operation == entitybase::RolePermissionOperation::Propose
    })
}

/// 与 Runtime 共用固定机构和注册局能力目录；不以机构类别代替岗位授权。
fn public_role_capability_allows(
    cid: &[u8],
    action: &entitybase::BusinessActionId<Vec<u8>>,
) -> Result<bool, String> {
    let parts = primitives::cid::number::parse_cid_number_parts_bytes(cid)
        .map_err(|_| "invalid institution CID for role authorization".to_string())?;
    Ok(entitybase::fixed_institution_capability_allows(
        parts.institution,
        cid,
        action.module_tag.as_slice(),
        action.action_code,
        entitybase::RolePermissionOperation::Propose,
    ) || entitybase::business_action::registry_institution_capability_allows(
        parts.institution,
        cid,
        action.module_tag.as_slice(),
        action.action_code,
        entitybase::RolePermissionOperation::Propose,
    ))
}

/// 在调用方已确定的 finalized 区块中读取当前签名者可发起指定业务的有效岗位。
/// 不使用登录快照授予业务权限，也不将无岗位管理员的空投影当作可用岗位。
pub async fn fetch_authorized_public_roles_at(
    identity: &NodeInstitutionIdentity,
    signer: &str,
    action: &entitybase::BusinessActionId<Vec<u8>>,
    block_hash: [u8; 32],
) -> Result<Vec<InstitutionAssignmentView>, String> {
    let finalized = FinalizedChainView::connect_at(Some(block_hash)).await?;
    let cid = identity.cid_number.as_bytes();
    // 能力目录补齐后仍逐项读取权限和任职。RPC/解码错误继续返回 Err，
    // 交由前端显示加载失败及重试，不能把查询失败伪装为空候选。
    if !public_role_capability_allows(cid, action)? {
        return Ok(Vec::new());
    }
    let storage = finalized.storage();
    let keys = || vec![dynamic::Value::from_bytes(cid)];
    let public = storage
        .fetch(&dynamic::storage("PublicManage", "Institutions", keys()))
        .await
        .map_err(|e| format!("read public institution failed: {e}"))?;
    let private = storage
        .fetch(&dynamic::storage("PrivateManage", "Institutions", keys()))
        .await
        .map_err(|e| format!("read private institution failed: {e}"))?;
    if public.is_none() || private.is_some() {
        return Err("institution does not have a unique public entity".to_string());
    }
    let admins = fetch_active_admins_onchain_at(identity, &finalized)
        .await?
        .ok_or_else(|| "institution admin roster missing".to_string())?;
    let active_admins: HashMap<_, _> = admins
        .into_iter()
        .filter(|admin| crate::crypto::pubkey::same_account_id(&admin.account_id, signer))
        .map(|admin| {
            (
                admin.role_account_id,
                ActiveAdminRoleProjection {
                    account_id: admin.account_id,
                    family_name: admin.family_name,
                    given_name: admin.given_name,
                },
            )
        })
        .collect();
    let (roles, assignments) =
        read_roles_and_assignments(&finalized, cid, AdminPallet::PublicAdmins).await?;
    let assignments = merge_authorization_assignments(
        roles,
        assignments,
        &active_admins,
        finalized.current_day().await?,
    )?;
    let mut authorized = Vec::new();
    for assignment in assignments {
        if assignment.role_code.is_empty()
            || !crate::crypto::pubkey::same_account_id(&assignment.account_id, signer)
        {
            continue;
        }
        let value = storage
            .fetch(&dynamic::storage(
                "PublicManage",
                "InstitutionRolePermissions",
                vec![
                    dynamic::Value::from_bytes(cid),
                    dynamic::Value::from_bytes(assignment.role_code.as_bytes()),
                ],
            ))
            .await
            .map_err(|e| format!("read institution role permissions failed: {e}"))?;
        let Some(value) = value else { continue };
        let mut encoded = value.encoded();
        let permissions = Vec::<entitybase::StoredRoleBusinessPermission<Vec<u8>>>::decode(&mut encoded)
            .map_err(|e| format!("decode institution role permissions failed: {e}"))?
            .into_iter().map(|v| v.with_subject(cid.to_vec(),assignment.role_code.as_bytes().to_vec())).collect::<Vec<_>>();
        if !encoded.is_empty() {
            return Err("role permissions have trailing bytes".into());
        }
        if permission_allows_proposal(&permissions, cid, assignment.role_code.as_bytes(), action) {
            if assignment.role_name.trim().is_empty() {
                return Err("authorized role has no display name".into());
            }
            authorized.push(assignment);
        }
    }
    authorized.sort_by(|left, right| left.role_code.cmp(&right.role_code));
    authorized.dedup_by(|left, right| left.role_code == right.role_code);
    Ok(authorized)
}

/// 查找某个联邦注册局管理员账户当前担任专员的省码。
pub(crate) async fn fetch_frg_province_codes_for_admin(
    finalized: &FinalizedChainView,
    cid_number: &[u8],
    account_id: [u8; 32],
) -> Result<Vec<[u8; 2]>, String> {
    let (roles, assignments) =
        read_roles_and_assignments(finalized, cid_number, AdminPallet::PublicAdmins).await?;
    let active_roles: HashMap<Vec<u8>, RawInstitutionRole> = roles
        .into_iter()
        .filter(|role| role.role_status == ROLE_STATUS_ACTIVE)
        .map(|role| (role.role_code.clone(), role))
        .collect();
    let current_day = finalized.current_day().await?;
    let assigned_codes: HashSet<Vec<u8>> = assignments
        .into_iter()
        .filter(|assignment| {
            assignment.account_id == account_id
                && active_roles
                    .get(&assignment.role_code)
                    .is_some_and(|role| assignment_is_effective(role, assignment, current_day))
        })
        .map(|assignment| assignment.role_code)
        .collect();
    Ok(primitives::cid::code::PROVINCE_CODE_INFOS
        .iter()
        .filter_map(|info| {
            assigned_codes
                .contains(
                    &primitives::governance_skeleton::province_commissioner_role_code(
                        info.province_code,
                    ),
                )
                .then_some(info.province_code)
        })
        .collect())
}

/// 按 43 个省专员岗位输出联邦注册局全部有效任职。
pub async fn fetch_all_federal_registry_assignments(
    identity: &NodeInstitutionIdentity,
) -> Result<Vec<FederalRegistryProvinceAssignments>, String> {
    let assignments = fetch_active_assignments_onchain(identity)
        .await?
        .ok_or_else(|| "federal registry admin account not found".to_string())?;
    Ok(primitives::cid::code::PROVINCE_CODE_INFOS
        .iter()
        .map(|info| {
            let role_code = primitives::governance_skeleton::province_commissioner_role_code(
                info.province_code,
            );
            FederalRegistryProvinceAssignments {
                province_code: info.province_code,
                province_name: info.province_name.to_string(),
                assignments: assignments
                    .iter()
                    .filter(|assignment| assignment.role_code.as_bytes() == role_code.as_slice())
                    .cloned()
                    .collect(),
            }
        })
        .collect())
}

/// 岗位定义与任职使用同一链上快照；account_id是名册锚点，signer_id是当前钱包，不互相覆盖。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceMember {
    pub account_id: String,
    pub signer_id: String,
    pub cid_number: String,
    pub family_name: String,
    pub given_name: String,
}
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernancePermission {
    pub module_tag: String,
    pub action_code: u32,
    pub operation: &'static str,
}
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceAssignment {
    pub account_id: String,
    pub term_start: u32,
    pub term_end: u32,
    pub active: bool,
    pub effective: bool,
    pub source_label: String,
    pub source_ref: String,
}
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceRole {
    pub role_code: String,
    pub role_name: String,
    pub term_required: bool,
    pub active: bool,
    pub fixed: bool,
    pub assignment_locked: bool,
    pub min_assignments: u32,
    pub max_assignments: Option<u32>,
    pub permissions: Vec<GovernancePermission>,
    pub assignments: Vec<GovernanceAssignment>,
}
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceRepresentative {
    pub family_name: String,
    pub given_name: String,
    pub cid_number: String,
    pub account_id: String,
}
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceState {
    pub cid_number: String,
    pub block_hash: String,
    pub block_number: u32,
    pub current_day: u32,
    pub module_tag: String,
    pub legal_role_code: String,
    pub members: Vec<GovernanceMember>,
    pub roles: Vec<GovernanceRole>,
    pub representative: Option<GovernanceRepresentative>,
    pub registration_state_hash: String,
    pub governance_threshold: Option<u32>,
    pub registration_roles: Vec<RegistrationRoleChoice>,
}

fn governance_roles(
    code: [u8; 4],
    cid: &str,
    roles: Vec<RawInstitutionRole>,
    assignments: Vec<RawInstitutionAssignment>,
    permissions: HashMap<Vec<u8>, Vec<BusinessPermission>>,
    current_day: u32,
) -> Result<Vec<GovernanceRole>, String> {
    use primitives::{governance_skeleton as skeleton, institution_constraints as constraints};
    let mut output = Vec::with_capacity(roles.len());
    let composition = constraints::member_composition_by_identity(code, cid.as_bytes());
    for role in roles {
        if role.cid_number != cid.as_bytes() {
            return Err("governance role CID mismatch".into());
        }
        let legal = constraints::is_legal_representative_role(&role.role_code);
        let seats = skeleton::fixed_role_seats_by_identity(code, cid.as_bytes(), &role.role_code);
        let composition_role = composition
            .as_ref()
            .is_some_and(|spec| spec.role_code == role.role_code.as_slice());
        let bounds = composition
            .as_ref()
            .filter(|spec| spec.role_code == role.role_code.as_slice())
            .map(|spec| (spec.min_members, spec.max_members))
            .or_else(|| {
                skeleton::fixed_role_assignment_bounds_by_identity(
                    code,
                    cid.as_bytes(),
                    &role.role_code,
                )
            });
        let mut row_permissions = Vec::new();
        if let Some(values) = permissions.get(&role.role_code) {
            for value in values {
                if value.role_subject.cid_number != cid.as_bytes()
                    || value.role_subject.role_code != role.role_code
                {
                    return Err("governance permission subject mismatch".into());
                }
                row_permissions.push(GovernancePermission {
                    module_tag: String::from_utf8(value.business_action_id.module_tag.clone())
                        .map_err(|_| "invalid permission module tag")?,
                    action_code: value.business_action_id.action_code,
                    operation: match value.operation {
                        entitybase::RolePermissionOperation::Propose => "PROPOSE",
                        entitybase::RolePermissionOperation::Vote => "VOTE",
                    },
                });
            }
        }
        let row_assignments = assignments
            .iter()
            .filter(|a| a.role_code == role.role_code)
            .map(|a| GovernanceAssignment {
                account_id: format!("0x{}", hex::encode(a.account_id)),
                term_start: a.term_start,
                term_end: a.term_end,
                active: a.assignment_status == ASSIGNMENT_STATUS_ACTIVE,
                effective: role.role_status == ROLE_STATUS_ACTIVE
                    && assignment_is_effective(&role, a, current_day),
                source_label: assignment_source_label(a.assignment_source).into(),
                source_ref: format!("0x{}", hex::encode(&a.assignment_source_ref)),
            })
            .collect();
        output.push(GovernanceRole {
            role_code: String::from_utf8(role.role_code)
                .map_err(|_| "invalid governance role code")?,
            role_name: String::from_utf8(role.role_name)
                .map_err(|_| "invalid governance role name")?,
            term_required: role.term_required,
            active: role.role_status == ROLE_STATUS_ACTIVE,
            fixed: legal || seats.is_some() || composition_role,
            assignment_locked: legal || (code == primitives::cid::code::FRG && seats.is_some()),
            min_assignments: bounds.map(|v| v.0).unwrap_or(0),
            max_assignments: bounds.map(|v| v.1),
            permissions: row_permissions,
            assignments: row_assignments,
        });
    }
    output.sort_by(|a, b| a.role_code.cmp(&b.role_code));
    Ok(output)
}

/// 登记使用原始账户锚点和二进制来源；展示用的当前钱包、中文标签不得参与指纹。
#[derive(Debug)]
pub struct RegistrationSnapshot {
    pub block_hash: [u8; 32],
    pub block_number: u32,
    pub current_day: u32,
    pub cid_number: Vec<u8>,
    pub institution: entitybase::InstitutionInfo<u32, Vec<u8>, Vec<u8>, [u8; 32]>,
    pub admins: Option<Vec<adminbase::Admin<[u8; 32]>>>,
    roles: BTreeMap<Vec<u8>, RawInstitutionRole>,
    permissions: Vec<(Vec<u8>, Vec<BusinessPermission>)>,
    assignments: Vec<(Vec<u8>, Vec<RawInstitutionAssignment>)>,
    nonce: u64,
    pub threshold: Option<u32>,
}

impl RegistrationSnapshot {
    pub fn module_tag(&self) -> &'static [u8] {
        if primitives::cid::code::is_private_legal_code(&self.institution.institution_code) {
            b"pri-mgmt"
        } else {
            b"pub-mgmt"
        }
    }

    /// 与entity call 9逐字段同序；Option名册、岗位排序、任职原始顺序都不能改写。
    pub fn state_hash(&self) -> [u8; 32] {
        let admins = self
            .admins
            .as_ref()
            .map(|admins| admins.iter().map(|a| a.account_id).collect::<Vec<_>>());
        entitybase::institution_governance::registration_state_hash(
            self.module_tag(),
            &(
                &self.cid_number,
                &self.institution,
                admins,
                &self.roles,
                &self.permissions,
                &self.assignments,
                self.nonce,
                self.threshold,
            ),
        )
    }

    pub fn retained_assignment(
        &self,
        role: &[u8],
        account: [u8; 32],
        start: u32,
        end: u32,
    ) -> Option<entitybase::InstitutionAssignmentTarget<[u8; 32]>> {
        let stored = self
            .assignments
            .iter()
            .find(|(code, _)| code == role)?
            .1
            .iter()
            .find(|a| {
                a.account_id == account
                    && a.term_start == start
                    && a.term_end == end
                    && a.assignment_status == 0
            })?;
        // Decode共享枚举而非根据中文标签反推来源，未知值拒绝构造。
        Some(entitybase::InstitutionAssignmentTarget {
            account_id: account,
            term_start: start,
            term_end: end,
            assignment_source: entitybase::InstitutionAssignmentSource::decode_all(
                &mut &[stored.assignment_source][..],
            )
            .ok()?,
            assignment_source_ref: stored.assignment_source_ref.clone(),
            assignment_status: entitybase::InstitutionAssignmentStatus::Active,
        })
    }
}

async fn registration_value<V: DecodeAll>(
    finalized: &FinalizedChainView,
    pallet: &str,
    item: &str,
    keys: &[&[u8]],
) -> Result<Option<V>, String> {
    finalized
        .storage()
        .fetch(&dynamic::storage(
            pallet,
            item,
            keys.iter()
                .map(|key| dynamic::Value::from_bytes(*key))
                .collect::<Vec<_>>(),
        ))
        .await
        .map_err(|e| format!("read {pallet}.{item} failed: {e}"))?
        .map(|value| {
            V::decode_all(&mut value.encoded())
                .map_err(|e| format!("decode {pallet}.{item} failed: {e}"))
        })
        .transpose()
}

pub async fn fetch_registration_snapshot(cid: &str) -> Result<RegistrationSnapshot, String> {
    let finalized = FinalizedChainView::connect().await?;
    registration_snapshot_at(cid, &finalized).await
}

async fn registration_snapshot_at(
    cid: &str,
    finalized: &FinalizedChainView,
) -> Result<RegistrationSnapshot, String> {
    let expected = primitives::cid::number::parse_cid_number_parts_bytes(cid.as_bytes())
        .map_err(|_| "invalid registration CID")?
        .institution;
    let mut found = None;
    for pallet in [AdminPallet::PublicAdmins, AdminPallet::PrivateAdmins] {
        let info: Option<entitybase::InstitutionInfo<u32, Vec<u8>, Vec<u8>, [u8; 32]>> =
            registration_value(
                finalized,
                entity_pallet_name(pallet),
                "Institutions",
                &[cid.as_bytes()],
            )
            .await?;
        if let Some(info) = info {
            if found.is_some() || info.institution_code != expected {
                return Err("registration institution identity is not unique".into());
            }
            found = Some((pallet, info));
        }
    }
    let (pallet, institution) = found.ok_or("registration institution missing")?;
    if (pallet == AdminPallet::PrivateAdmins)
        != primitives::cid::code::is_private_legal_code(&expected)
    {
        return Err("registration institution namespace mismatch".into());
    }
    let name = entity_pallet_name(pallet);
    let mut roles = BTreeMap::new();
    let mut rows = finalized
        .storage()
        .iter(dynamic::storage(
            name,
            "InstitutionRoles",
            vec![dynamic::Value::from_bytes(cid.as_bytes())],
        ))
        .await
        .map_err(|e| e.to_string())?;
    while let Some(row) = rows.next().await {
        let row = row.map_err(|e| e.to_string())?;
        let code = role_code_from_storage_key(&row.key_bytes,name,"InstitutionRoles",cid.as_bytes())?;
        let role = decode_stored_role(row.value.encoded(),cid.as_bytes(),code)?;
        if role.cid_number != cid.as_bytes()
            || role.role_status > 1
            || roles.insert(role.role_code.clone(), role).is_some()
            || roles.len() > 1989
        {
            return Err("invalid registration roles".into());
        }
    }
    let mut permissions = Vec::with_capacity(roles.len());
    let mut assignments = Vec::with_capacity(roles.len());
    for code in roles.keys() {
        let keys = [cid.as_bytes(), code.as_slice()];
        let stored: Vec<entitybase::StoredRoleBusinessPermission<Vec<u8>>> =
            registration_value(finalized, name, "InstitutionRolePermissions", &keys).await?.unwrap_or_default();
        let values: Vec<BusinessPermission> = stored.into_iter().map(|v| v.with_subject(cid.as_bytes().to_vec(),code.clone())).collect();
        if values.iter().any(|p| {
            p.role_subject.cid_number != cid.as_bytes() || p.role_subject.role_code != *code
        }) {
            return Err("registration permission subject mismatch".into());
        }
        permissions.push((code.clone(), values));
        let stored: Vec<entitybase::StoredInstitutionAdminAssignment<[u8;32],Vec<u8>>> =
            registration_value(finalized, name, "InstitutionRoleAssignments", &keys).await?.unwrap_or_default();
        let values: Vec<RawInstitutionAssignment> = stored.into_iter().map(|v| restore_assignment(v,cid.as_bytes(),code)).collect();
        if values.iter().any(|a| {
            a.cid_number != cid.as_bytes()
                || a.role_code != *code
                || a.assignment_source > 5
                || a.assignment_status > 1
        }) {
            return Err("registration assignment subject mismatch".into());
        }
        assignments.push((code.clone(), values));
    }
    let roster: Option<adminbase::InstitutionAdmins<Vec<adminbase::Admin<[u8; 32]>>>> =
        registration_value(
            finalized,
            pallet.pallet_name(),
            "AdminAccounts",
            &[cid.as_bytes()],
        )
        .await?;
    if roster
        .as_ref()
        .is_some_and(|r| r.institution_code != expected)
    {
        return Err("registration roster identity mismatch".into());
    }
    Ok(RegistrationSnapshot {
        block_hash: finalized.block_hash.0,
        block_number: finalized.block_number,
        current_day: finalized.current_day().await?,
        cid_number: cid.as_bytes().to_vec(),
        institution,
        admins: roster.map(|r| r.admins),
        roles,
        permissions,
        assignments,
        nonce: registration_value(finalized, name, "InstitutionRoleNonce", &[cid.as_bytes()])
            .await?
            .unwrap_or(0),
        threshold: registration_value(
            finalized,
            name,
            "InstitutionGovernanceThresholds",
            &[cid.as_bytes()],
        )
        .await?,
    })
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrationRoleChoice {
    pub role_code: String,
    pub role_name: String,
}

fn registration_role_in_scope(actor: &[u8], role: &[u8], target: &[u8], code: [u8; 4]) -> bool {
    use primitives::{
        cid::{code as codes, number},
        institution_constraints as constraints,
    };
    if codes::is_fixed_governance_code(&code) || constraints::is_permanent_singleton_code(&code) {
        return false;
    }
    let Ok(parts) = number::parse_cid_number_parts_bytes(actor) else {
        return false;
    };
    let Ok((province, city)) = number::cid_scope_codes(target) else {
        return false;
    };
    if parts.institution == codes::FRG {
        return role == primitives::governance_skeleton::province_commissioner_role_code(province);
    }
    parts.institution == *b"CREG"
        && code != *b"CREG"
        && number::cid_scope_codes(actor).is_ok_and(|scope| scope == (province, city))
}

/// 使用同一finalized区块的有效名册、任职、权限和既有登记辖区规则。
pub async fn fetch_registration_roles_at(
    actor: &NodeInstitutionIdentity,
    signer: &str,
    target: &str,
    block: [u8; 32],
) -> Result<Vec<RegistrationRoleChoice>, String> {
    let code = primitives::cid::number::parse_cid_number_parts_bytes(target.as_bytes())
        .map_err(|_| "invalid target CID")?
        .institution;
    Ok(fetch_authorized_public_roles_at(
        actor,
        signer,
        &entitybase::BusinessActionId {
            module_tag: entitybase::business_action::MODULE_INSTITUTION_REGISTRATION.to_vec(),
            action_code: entitybase::business_action::ACTION_REGISTER_INSTITUTION,
        },
        block,
    )
    .await?
    .into_iter()
    .filter(|r| {
        registration_role_in_scope(
            actor.cid_number.as_bytes(),
            r.role_code.as_bytes(),
            target.as_bytes(),
            code,
        )
    })
    .map(|r| RegistrationRoleChoice {
        role_code: r.role_code,
        role_name: r.role_name,
    })
    .collect())
}

pub fn select_registration_role(
    roles: &[RegistrationRoleChoice],
    selected: Option<&str>,
) -> Result<String, &'static str> {
    let selected = selected.map(str::trim).filter(|s| !s.is_empty());
    match (roles, selected) {
        ([], _) => Err("当前管理员没有可办理此业务的有效岗位"),
        ([role], None) => Ok(role.role_code.clone()),
        (_, Some(code)) if roles.iter().any(|r| r.role_code == code) => Ok(code.into()),
        (_, None) => Err("请选择办理岗位名称"),
        _ => Err("办理岗位已失效，请刷新后重新选择"),
    }
}

/// 纯数据读取：不判断调用人是否仍在名册、是否拥有岗位；写权限由原写入口和链端裁决。
pub async fn fetch_governance_state(
    identity: &NodeInstitutionIdentity,
) -> Result<GovernanceState, String> {
    let finalized = FinalizedChainView::connect().await?;
    let snapshot = registration_snapshot_at(&identity.cid_number, &finalized).await?;
    let pallet = if snapshot.module_tag() == b"pri-mgmt" {
        AdminPallet::PrivateAdmins
    } else {
        AdminPallet::PublicAdmins
    };
    let info = snapshot.institution.clone();
    let roles = snapshot.roles.values().cloned().collect();
    let assignments = snapshot
        .assignments
        .iter()
        .flat_map(|(_, values)| values.iter().cloned())
        .collect();
    let permissions = snapshot.permissions.iter().cloned().collect();
    let members = fetch_active_admins_onchain_at(identity, &finalized)
        .await?
        .unwrap_or_default()
        .into_iter()
        .map(|a| GovernanceMember {
            account_id: format!("0x{}", hex::encode(a.role_account_id)),
            signer_id: a.account_id,
            cid_number: a.cid_number,
            family_name: a.family_name,
            given_name: a.given_name,
        })
        .collect();
    let representative = info
        .legal_representative
        .map(|r| -> Result<_, String> {
            Ok(GovernanceRepresentative {
                family_name: String::from_utf8(r.family_name)
                    .map_err(|_| "invalid representative family name")?,
                given_name: String::from_utf8(r.given_name)
                    .map_err(|_| "invalid representative given name")?,
                cid_number: String::from_utf8(r.cid_number)
                    .map_err(|_| "invalid representative CID")?,
                account_id: format!("0x{}", hex::encode(r.account_id)),
            })
        })
        .transpose()?;
    let current_day = finalized.current_day().await?;
    Ok(GovernanceState {
        registration_state_hash: format!("0x{}", hex::encode(snapshot.state_hash())),
        governance_threshold: snapshot.threshold,
        registration_roles: Vec::new(),
        cid_number: identity.cid_number.clone(),
        block_hash: format!("0x{}", hex::encode(finalized.block_hash.0)),
        block_number: finalized.block_number,
        current_day,
        module_tag: match pallet {
            AdminPallet::PublicAdmins => "pub-mgmt",
            AdminPallet::PrivateAdmins => "pri-mgmt",
        }
        .into(),
        legal_role_code: String::from_utf8_lossy(
            primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE,
        )
        .into_owned(),
        roles: governance_roles(
            info.institution_code,
            &identity.cid_number,
            roles,
            assignments,
            permissions,
            current_day,
        )?,
        members,
        representative,
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "../tests/institutions_admins_chain_roles_scale_contract_tests.rs"]
mod scale_contract_tests;
