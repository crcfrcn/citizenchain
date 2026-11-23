//! 只读解码机构岗位、任职和管理员CID的当前链上绑定。

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

async fn read_roles_and_assignments(
    finalized: &FinalizedChainView,
    cid_number: &[u8],
    admin_pallet: AdminPallet,
) -> Result<(Vec<RawInstitutionRole>, Vec<RawInstitutionAssignment>), String> {
    let storage = finalized.storage();
    let pallet = entity_pallet_name(admin_pallet);

    // 按完整 CID 前缀读取本机构条目，并核对值内 CID；不解析 storage key 的哈希后缀。
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
        let mut encoded = item.value.encoded();
        let role = RawInstitutionRole::decode(&mut encoded)
            .map_err(|e| format!("decode {pallet} InstitutionRoles failed: {e}"))?;
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
        let role_assignments = Vec::<RawInstitutionAssignment>::decode(&mut encoded)
            .map_err(|e| format!("decode {pallet} InstitutionRoleAssignments failed: {e}"))?;
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
    let Some(admins) = fetch_active_admins_onchain_at(identity, finalized, None).await? else {
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
    let admins = fetch_active_admins_onchain_at(identity, &finalized, None)
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
        let permissions = Vec::<BusinessPermission>::decode(&mut encoded)
            .map_err(|e| format!("decode institution role permissions failed: {e}"))?;
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

/// 岗位定义与任职使用同一链上快照；account_id与signer_id均为实际管理员钱包；authorization_account_id只关联既有岗位授权。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceMember {
    pub account_id: String,
    /// 已批准岗位资料引用；不作为管理员或钱包变更目标。
    pub authorization_account_id: String,
    pub signer_id: String,
    pub cid_number: String,
    pub family_name: String,
    pub given_name: String,
    pub wallet_revision: u64,
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

/// 登记指纹使用实际管理员账户、原始任职账户引用和二进制来源；中文标签不得参与指纹。
#[derive(Debug)]
pub struct RegistrationSnapshot {
    owner: AdminPallet,
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
    pub wallet_states: BTreeMap<[u8; 32], adminbase::InstitutionAdminWalletState<[u8; 32]>>,
    pub role_authorizations: BTreeMap<[u8; 32], [u8; 32]>,
}

impl RegistrationSnapshot {
    pub fn module_tag(&self) -> &'static [u8] {
        if matches!(self.owner, AdminPallet::PrivateAdmins) {
            b"pri-mgmt"
        } else {
            b"pub-mgmt"
        }
    }

    pub fn validate_wallet_action(
        &self,
        action: &entitybase::InstitutionGovernanceAction<[u8; 32]>,
        actor_role: &[u8],
    ) -> Result<(), String> {
        let entitybase::InstitutionGovernanceAction::RegisterAdminWalletChanges {
            added,
            removed,
            replaced,
        } = action
        else {
            return Err("not a wallet registration action".into());
        };
        let current = self.admins.as_deref().unwrap_or_default();
        if added.len() + removed.len() + replaced.len() == 0
            || added.len() + removed.len() + replaced.len() > 1989
        {
            return Err("empty or oversized wallet operation".into());
        }
        if primitives::governance_skeleton::fixed_institution_by_identity(
            self.institution.institution_code,
            &self.cid_number,
        )
        .is_some()
            && (!added.is_empty() || !removed.is_empty())
        {
            return Err("fixed institution wallet count cannot change".into());
        }
        if self.module_tag() == b"pub-mgmt" {
            let mut cids = std::collections::BTreeSet::new();
            if current
                .iter()
                .filter(|a| !removed.iter().any(|(w, _)| w == &a.account_id))
                .chain(added.iter())
                .filter(|a| !a.cid_number.is_empty())
                .any(|a| !cids.insert(a.cid_number.clone()))
            {
                return Err("duplicate public citizen metadata".into());
            }
        }
        let mut sources = std::collections::BTreeSet::new();
        let mut targets = std::collections::BTreeSet::new();
        for (wallet, revision) in removed.iter().map(|(w, r)| (w, r)).chain(
            replaced
                .iter()
                .map(|c| (&c.admin_account, &c.expected_revision)),
        ) {
            if !sources.insert(*wallet)
                || !current.iter().any(|a| &a.account_id == wallet)
                || !self
                    .wallet_states
                    .get(wallet)
                    .is_some_and(|s| s.wallet == Some(*wallet) && s.revision == *revision)
            {
                return Err("wallet membership or version changed".into());
            }
        }
        for wallet in added
            .iter()
            .map(|a| &a.account_id)
            .chain(replaced.iter().map(|c| &c.new_wallet))
        {
            if !targets.insert(*wallet)
                || current.iter().any(|a| &a.account_id == wallet)
                || sources.contains(wallet)
            {
                return Err("duplicate wallet target".into());
            }
        }
        for change in replaced {
            if change.admin_account != change.expected_wallet
                || change.expected_wallet == change.new_wallet
            {
                return Err("old wallet mismatch".into());
            }
            if self.institution.institution_code == primitives::cid::code::FRG {
                let reference = self
                    .role_authorizations
                    .get(&change.admin_account)
                    .ok_or("role authorization missing")?;
                if !self.assignments.iter().any(|(role, rows)| {
                    role == actor_role
                        && rows.iter().any(|a| {
                            &a.account_id == reference
                                && self.roles.get(role).is_some_and(|r| {
                                    r.role_status == ROLE_STATUS_ACTIVE
                                        && assignment_is_effective(r, a, self.current_day)
                                })
                        })
                }) {
                    return Err("target wallet is outside registrar province group".into());
                }
            }
        }
        if current
            .len()
            .saturating_sub(removed.len())
            .saturating_add(added.len())
            > 1989
        {
            return Err("too many wallets".into());
        }
        Ok(())
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
    if !primitives::cid::code::is_unincorporated_code(&expected)
        && (pallet == AdminPallet::PrivateAdmins)
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
        let role =
            RawInstitutionRole::decode_all(&mut row.value.encoded()).map_err(|e| e.to_string())?;
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
        let values: Vec<BusinessPermission> =
            registration_value(finalized, name, "InstitutionRolePermissions", &keys)
                .await?
                .unwrap_or_default();
        if values.iter().any(|p| {
            p.role_subject.cid_number != cid.as_bytes() || p.role_subject.role_code != *code
        }) {
            return Err("registration permission subject mismatch".into());
        }
        permissions.push((code.clone(), values));
        let values: Vec<RawInstitutionAssignment> =
            registration_value(finalized, name, "InstitutionRoleAssignments", &keys)
                .await?
                .unwrap_or_default();
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
    let mut wallet_states = BTreeMap::new();
    let mut role_authorizations = BTreeMap::new();
    for admin in roster
        .as_ref()
        .map(|r| r.admins.as_slice())
        .unwrap_or_default()
    {
        let state = crate::chain_runtime::institution_wallet_state_at(
            &finalized.storage(),
            pallet,
            cid.as_bytes(),
            &admin.account_id,
        )
        .await?
        .ok_or("institution wallet state missing")?;
        if state.wallet != Some(admin.account_id) {
            return Err("institution wallet mismatch".into());
        }
        wallet_states.insert(admin.account_id, state);
        role_authorizations.insert(
            admin.account_id,
            crate::chain_runtime::institution_role_authorization_at(
                &finalized.storage(),
                pallet,
                cid.as_bytes(),
                &admin.account_id,
            )
            .await?,
        );
    }
    Ok(RegistrationSnapshot {
        owner: pallet,
        wallet_states,
        role_authorizations,
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
    primitives::institution_constraints::registry_wallet_role_in_scope(actor, role, target, code)
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
    let mut members = Vec::new();
    for admin in fetch_active_admins_onchain_at(identity, &finalized, None)
        .await?
        .unwrap_or_default()
    {
        let state = crate::chain_runtime::institution_wallet_state_at(
            &finalized.storage(),
            pallet,
            identity.cid_number.as_bytes(),
            &crate::authentication::login::parse_account_id_bytes(&admin.account_id)
                .ok_or("invalid member wallet")?,
        )
        .await?;
        let state = state.ok_or("institution wallet state missing")?;
        let wallet = crate::authentication::login::parse_account_id_bytes(&admin.account_id)
            .ok_or("invalid member wallet")?;
        if state.wallet != Some(wallet) {
            return Err("institution wallet state inactive or mismatched".into());
        }
        let revision = state.revision;
        members.push(GovernanceMember {
            account_id: admin.account_id.clone(),
            authorization_account_id: format!("0x{}", hex::encode(admin.role_account_id)),
            signer_id: admin.account_id,
            cid_number: admin.cid_number,
            family_name: admin.family_name,
            given_name: admin.given_name,
            wallet_revision: revision,
        });
    }
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
