//! 同一最终块内读取机构岗位、人员任职和权限；不维护机构人员名单。
use crate::chain_runtime::{EntityPallet, FinalizedChainView, NodeInstitutionIdentity};
use codec::{DecodeAll, Encode};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use subxt::dynamic;

type Role = entitybase::InstitutionRole<Vec<u8>, Vec<u8>, Vec<u8>>;
type Assignment = entitybase::InstitutionAssignment<Vec<u8>, Vec<u8>, Vec<u8>>;
type Permission = entitybase::RoleBusinessPermission<Vec<u8>, Vec<u8>, Vec<u8>>;
type Institution = entitybase::InstitutionInfo<u32, Vec<u8>, Vec<u8>>;

pub(crate) fn person_allowed(person: &[u8], owner: EntityPallet) -> bool {
    let Ok(text) = std::str::from_utf8(person) else {
        return false;
    };
    if text.trim() != text {
        return false;
    }
    let Ok(parts) = primitives::cid::number::parse_cid_number_parts_bytes(person) else {
        return false;
    };
    parts.institution == *b"CTZN" || owner == EntityPallet::Private && parts.institution == *b"NATP"
}
fn effective(role: &Role, assignment: &Assignment, day: u32) -> bool {
    role.role_status == entitybase::InstitutionRoleStatus::Active
        && assignment.assignment_status == entitybase::InstitutionAssignmentStatus::Active
        && if role.term_required {
            assignment.term_start > 0 && assignment.term_start <= day && day <= assignment.term_end
        } else {
            assignment.term_start == 0 && assignment.term_end == 0
        }
}
fn source_label(source: entitybase::InstitutionAssignmentSource) -> &'static str {
    match source {
        entitybase::InstitutionAssignmentSource::Genesis => "创世",
        entitybase::InstitutionAssignmentSource::Registry => "注册局",
        _ => "链上任职结果",
    }
}
pub(crate) async fn value<T: DecodeAll>(
    view: &FinalizedChainView,
    pallet: &str,
    item: &str,
    keys: &[&[u8]],
) -> Result<Option<T>, String> {
    view.storage()
        .fetch(&dynamic::storage(
            pallet,
            item,
            keys.iter()
                .map(|key| dynamic::Value::from_bytes(*key))
                .collect::<Vec<_>>(),
        ))
        .await
        .map_err(|e| e.to_string())?
        .map(|v| {
            T::decode_all(&mut v.encoded()).map_err(|e| format!("decode {pallet}.{item}: {e}"))
        })
        .transpose()
}

#[derive(Debug)]
pub struct RegistrationSnapshot {
    pub owner: EntityPallet,
    pub block_hash: [u8; 32],
    pub block_number: u32,
    pub current_day: u32,
    pub cid_number: Vec<u8>,
    pub institution: Institution,
    pub(crate) roles: BTreeMap<Vec<u8>, Role>,
    pub(crate) permissions: Vec<(Vec<u8>, Vec<Permission>)>,
    pub(crate) assignments: Vec<(Vec<u8>, Vec<Assignment>)>,
    nonce: u64,
    pub threshold: Option<u32>,
    pub max_seats: u32,
}
impl RegistrationSnapshot {
    pub fn module_tag(&self) -> &'static [u8] {
        self.owner.module_tag()
    }
    /// 字段顺序和排序直接对应所属entity的registration_state_hash；不含钱包或世代。
    pub fn state_hash(&self) -> [u8; 32] {
        entitybase::institution_governance::registration_state_hash(
            self.module_tag(),
            &(
                &self.cid_number,
                &self.institution,
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
        person: &[u8],
        start: u32,
        end: u32,
    ) -> Option<entitybase::InstitutionAssignmentTarget> {
        let a = self
            .assignments
            .iter()
            .find(|(r, _)| r == role)?
            .1
            .iter()
            .find(|a| {
                a.person_cid == person
                    && a.term_start == start
                    && a.term_end == end
                    && a.assignment_status == entitybase::InstitutionAssignmentStatus::Active
            })?;
        Some(entitybase::InstitutionAssignmentTarget {
            person_cid: a.person_cid.clone(),
            term_start: start,
            term_end: end,
            assignment_source: a.assignment_source,
            assignment_source_ref: a.assignment_source_ref.clone(),
            assignment_status: a.assignment_status,
        })
    }
}
pub async fn fetch_registration_snapshot(cid: &str) -> Result<RegistrationSnapshot, String> {
    registration_snapshot_at(cid, &FinalizedChainView::connect().await?).await
}
pub(crate) async fn registration_snapshot_at(
    cid: &str,
    view: &FinalizedChainView,
) -> Result<RegistrationSnapshot, String> {
    let parts = primitives::cid::number::parse_cid_number_parts_bytes(cid.as_bytes())
        .map_err(|_| "invalid institution CID")?;
    let mut found = None;
    for owner in [EntityPallet::Public, EntityPallet::Private] {
        if let Some(info) =
            value::<Institution>(view, owner.pallet_name(), "Institutions", &[cid.as_bytes()])
                .await?
        {
            if found.is_some() || info.institution_code != parts.institution {
                return Err("institution identity is not unique".into());
            }
            found = Some((owner, info));
        }
    }
    let (owner, institution) = found.ok_or("institution missing")?;
    if !primitives::cid::code::is_unincorporated_code(&parts.institution)
        && (owner == EntityPallet::Private)
            != primitives::cid::code::is_private_legal_code(&parts.institution)
    {
        return Err("institution namespace mismatch".into());
    }
    let pallet = owner.pallet_name();
    let mut roles = BTreeMap::new();
    let mut iter = view
        .storage()
        .iter(dynamic::storage(
            pallet,
            "InstitutionRoles",
            vec![dynamic::Value::from_bytes(cid.as_bytes())],
        ))
        .await
        .map_err(|e| e.to_string())?;
    while let Some(row) = iter.next().await {
        let row = row.map_err(|e| e.to_string())?;
        let role = Role::decode_all(&mut row.value.encoded()).map_err(|e| e.to_string())?;
        if role.cid_number != cid.as_bytes()
            || role.seats == 0
            || roles.insert(role.role_code.clone(), role).is_some()
        {
            return Err("invalid institution role".into());
        }
    }
    let mut permissions = Vec::new();
    let mut assignments = Vec::new();
    for role in roles.keys() {
        let perms = value::<Vec<Permission>>(
            view,
            pallet,
            "InstitutionRolePermissions",
            &[cid.as_bytes(), role],
        )
        .await?
        .unwrap_or_default();
        if perms.iter().any(|p| {
            p.role_subject.cid_number != cid.as_bytes() || p.role_subject.role_code != *role
        }) {
            return Err("role permission subject mismatch".into());
        }
        let rows = value::<Vec<Assignment>>(
            view,
            pallet,
            "InstitutionRoleAssignments",
            &[cid.as_bytes(), role],
        )
        .await?
        .unwrap_or_default();
        let mut seen = BTreeSet::new();
        if rows.iter().any(|a| {
            a.cid_number != cid.as_bytes()
                || a.role_code != *role
                || !person_allowed(&a.person_cid, owner)
                || !seen.insert(a.person_cid.clone())
        }) {
            return Err("invalid or duplicate person assignment".into());
        }
        permissions.push((role.clone(), perms));
        assignments.push((role.clone(), rows));
    }
    let constant = view
        .client
        .constants()
        .at(&dynamic::constant(pallet, "MaxRoleEntries"))
        .map_err(|e| e.to_string())?;
    let max_seats = u32::decode_all(&mut constant.encoded()).map_err(|e| e.to_string())?;
    Ok(RegistrationSnapshot {
        owner,
        block_hash: view.block_hash.0,
        block_number: view.block_number,
        current_day: view.current_day().await?,
        cid_number: cid.as_bytes().to_vec(),
        institution,
        roles,
        permissions,
        assignments,
        nonce: value(view, pallet, "InstitutionRoleNonce", &[cid.as_bytes()])
            .await?
            .unwrap_or(0),
        threshold: value(
            view,
            pallet,
            "InstitutionGovernanceThresholds",
            &[cid.as_bytes()],
        )
        .await?,
        max_seats,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionAssignmentView {
    pub person_cid: String,
    pub account_id: String,
    pub binding_revision: String,
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
pub(crate) async fn fetch_active_assignments_at(
    identity: &NodeInstitutionIdentity,
    view: &FinalizedChainView,
    signer: Option<&str>,
) -> Result<Vec<InstitutionAssignmentView>, String> {
    let snapshot = registration_snapshot_at(&identity.cid_number, view).await?;
    let mut result = Vec::new();
    let selected_person = if let Some(signer) = signer {
        let account =
            crate::authentication::login::parse_account_id_bytes(signer).ok_or("invalid signer")?;
        crate::chain_identity::read_active_cid_number_by_account_id_at(&view.storage(), account)
            .await?
    } else {
        None
    };
    if signer.is_some() && selected_person.is_none() {
        return Ok(result);
    }

    for (code, rows) in &snapshot.assignments {
        let role = &snapshot.roles[code];
        for a in rows
            .iter()
            .filter(|a| effective(role, a, snapshot.current_day))
        {
            if selected_person.as_ref().is_some_and(|p| p != &a.person_cid) {
                continue;
            }
            let person =
                String::from_utf8(a.person_cid.clone()).map_err(|_| "person CID is not UTF-8")?;
            let Some(account) =
                crate::chain_identity::read_active_cid_account_id_at(&view.storage(), &person)
                    .await?
            else {
                continue;
            };
            let account_id = format!("0x{}", hex::encode(account));
            if signer.is_some_and(|s| !crate::crypto::pubkey::same_account_id(s, &account_id)) {
                continue;
            }
            let revision = value::<u64>(
                view,
                "Identity",
                "BindingRevisionByCid",
                &[person.as_bytes()],
            )
            .await?
            .ok_or("person binding revision missing")?;
            // 姓名只作展示；没有投票/参选资料的合法人员仍可任职。
            let detail =
                crate::chain_identity::read_citizen_identity_at(&person, Some(view.block_hash.0))
                    .await?;
            let (family_name, given_name) = detail
                .candidate
                .map(|c| {
                    (
                        String::from_utf8_lossy(&c.family_name).into_owned(),
                        String::from_utf8_lossy(&c.given_name).into_owned(),
                    )
                })
                .unwrap_or_default();
            result.push(InstitutionAssignmentView {
                person_cid: person,
                account_id,
                binding_revision: revision.to_string(),
                family_name,
                given_name,
                role_code: String::from_utf8(code.clone()).map_err(|_| "role code is not UTF-8")?,
                role_name: String::from_utf8(role.role_name.clone())
                    .map_err(|_| "role name is not UTF-8")?,
                term_required: role.term_required,
                term_start: a.term_start,
                term_end: a.term_end,
                assignment_source: a.assignment_source.encode()[0],
                assignment_source_label: source_label(a.assignment_source).into(),
                assignment_source_ref: format!("0x{}", hex::encode(&a.assignment_source_ref)),
            });
        }
    }
    result.sort_by(|a, b| {
        a.role_code
            .cmp(&b.role_code)
            .then(a.person_cid.cmp(&b.person_cid))
    });
    Ok(result)
}
pub async fn fetch_active_assignments_onchain(
    identity: &NodeInstitutionIdentity,
) -> Result<Option<Vec<InstitutionAssignmentView>>, String> {
    Ok(Some(
        fetch_active_assignments_at(identity, &FinalizedChainView::connect().await?, None).await?,
    ))
}
pub async fn fetch_authorized_roles_at(
    identity: &NodeInstitutionIdentity,
    signer: &str,
    action: &entitybase::BusinessActionId<Vec<u8>>,
    block: [u8; 32],
) -> Result<Vec<InstitutionAssignmentView>, String> {
    let view = FinalizedChainView::connect_at(Some(block)).await?;
    let snapshot = registration_snapshot_at(&identity.cid_number, &view).await?;
    let assignments = fetch_active_assignments_at(identity, &view, Some(signer)).await?;
    Ok(assignments
        .into_iter()
        .filter(|a| {
            snapshot.permissions.iter().any(|(code, perms)| {
                code == a.role_code.as_bytes()
                    && perms.iter().any(|p| {
                        p.business_action_id == *action
                            && p.operation == entitybase::RolePermissionOperation::Propose
                    })
            })
        })
        .collect())
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrationRoleChoice {
    pub role_code: String,
    pub role_name: String,
}
pub async fn fetch_registration_roles_at(
    actor: &NodeInstitutionIdentity,
    signer: &str,
    target: &str,
    block: [u8; 32],
) -> Result<Vec<RegistrationRoleChoice>, String> {
    let code = primitives::cid::number::parse_cid_number_parts_bytes(target.as_bytes())
        .map_err(|_| "invalid target CID")?
        .institution;
    let action = entitybase::BusinessActionId {
        module_tag: entitybase::business_action::MODULE_INSTITUTION_REGISTRATION.to_vec(),
        action_code: entitybase::business_action::ACTION_REGISTER_INSTITUTION,
    };
    Ok(fetch_authorized_roles_at(actor, signer, &action, block)
        .await?
        .into_iter()
        .filter(|r| {
            primitives::institution_constraints::registry_role_in_scope(
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
    match selected {
        Some(code) if roles.iter().any(|r| r.role_code == code) => Ok(code.into()),
        _ => Err("请选择当前有效的办理岗位"),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernancePermission {
    pub module_tag: String,
    pub action_code: u32,
    pub operation: &'static str,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceAssignment {
    pub person_cid: String,
    pub term_start: u32,
    pub term_end: u32,
    pub active: bool,
    pub effective: bool,
    pub source_label: String,
    pub source_ref: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceRole {
    pub role_code: String,
    pub role_name: String,
    pub term_required: bool,
    pub seats: u32,
    pub active: bool,
    pub fixed: bool,
    pub assignment_locked: bool,
    pub min_assignments: u32,
    pub max_assignments: u32,
    pub permissions: Vec<GovernancePermission>,
    pub assignments: Vec<GovernanceAssignment>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceRepresentative {
    pub family_name: String,
    pub given_name: String,
    pub person_cid: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceState {
    /// 真实目标Runtime是否提供普通机构政策；未升级时不展示可执行入口。
    pub policy_available: bool,
    pub cid_number: String,
    pub block_hash: String,
    pub block_number: u32,
    pub current_day: u32,
    pub module_tag: String,
    pub legal_role_code: String,
    pub max_seats: u32,
    pub roles: Vec<GovernanceRole>,
    pub representative: Option<GovernanceRepresentative>,
    pub registration_state_hash: String,
    pub governance_threshold: Option<u32>,
    pub proposer_roles: Vec<RegistrationRoleChoice>,
    pub registration_roles: Vec<RegistrationRoleChoice>,
}
impl RegistrationSnapshot {
    pub fn governance_state(
        &self,
        assignments: &[InstitutionAssignmentView],
    ) -> Result<GovernanceState, String> {
        let mut roles = Vec::new();
        let code = self.institution.institution_code;
        for (key, role) in &self.roles {
            let bounds = primitives::institution_constraints::member_composition_by_identity(
                code,
                &self.cid_number,
            )
            .filter(|spec| spec.role_code == key.as_slice())
            .map(|spec| (spec.min_members, spec.max_members))
            .or_else(|| {
                primitives::governance_skeleton::fixed_role_assignment_bounds_by_identity(
                    code,
                    &self.cid_number,
                    key,
                )
            });
            let legal = primitives::institution_constraints::is_legal_representative_role(key);
            let perms = self
                .permissions
                .iter()
                .find(|(r, _)| r == key)
                .map(|(_, p)| p.as_slice())
                .unwrap_or_default();
            roles.push(GovernanceRole {
                role_code: String::from_utf8_lossy(key).into_owned(),
                role_name: String::from_utf8_lossy(&role.role_name).into_owned(),
                term_required: role.term_required,
                seats: role.seats,
                active: role.role_status == entitybase::InstitutionRoleStatus::Active,
                fixed: legal || bounds.is_some(),
                assignment_locked: legal,
                min_assignments: bounds.map(|b| b.0).unwrap_or(0),
                max_assignments: role.seats,
                permissions: perms
                    .iter()
                    .map(|p| GovernancePermission {
                        module_tag: String::from_utf8_lossy(&p.business_action_id.module_tag)
                            .into_owned(),
                        action_code: p.business_action_id.action_code,
                        operation: match p.operation {
                            entitybase::RolePermissionOperation::Propose => "PROPOSE",
                            entitybase::RolePermissionOperation::Vote => "VOTE",
                        },
                    })
                    .collect(),
                assignments: self
                    .assignments
                    .iter()
                    .find(|(r, _)| r == key)
                    .map(|(_, a)| {
                        a.iter()
                            .map(|a| GovernanceAssignment {
                                person_cid: String::from_utf8_lossy(&a.person_cid).into_owned(),
                                term_start: a.term_start,
                                term_end: a.term_end,
                                active: a.assignment_status
                                    == entitybase::InstitutionAssignmentStatus::Active,
                                effective: effective(role, a, self.current_day),
                                source_label: source_label(a.assignment_source).into(),
                                source_ref: format!("0x{}", hex::encode(&a.assignment_source_ref)),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            });
        }
        let proposer_roles = assignments
            .iter()
            .filter(|a| {
                roles.iter().any(|r| {
                    r.role_code == a.role_code
                        && r.permissions.iter().any(|p| {
                            p.module_tag.as_bytes() == self.module_tag()
                                && p.action_code == 3
                                && p.operation == "PROPOSE"
                        })
                })
            })
            .map(|a| RegistrationRoleChoice {
                role_code: a.role_code.clone(),
                role_name: a.role_name.clone(),
            })
            .collect();
        Ok(GovernanceState {
            policy_available: false,
            cid_number: String::from_utf8_lossy(&self.cid_number).into_owned(),
            block_hash: format!("0x{}", hex::encode(self.block_hash)),
            block_number: self.block_number,
            current_day: self.current_day,
            module_tag: String::from_utf8_lossy(self.module_tag()).into_owned(),
            legal_role_code: "LR".into(),
            max_seats: self.max_seats,
            roles,
            representative: self.institution.legal_representative.as_ref().map(|r| {
                GovernanceRepresentative {
                    family_name: String::from_utf8_lossy(&r.family_name).into_owned(),
                    given_name: String::from_utf8_lossy(&r.given_name).into_owned(),
                    person_cid: String::from_utf8_lossy(&r.person_cid).into_owned(),
                }
            }),
            registration_state_hash: format!("0x{}", hex::encode(self.state_hash())),
            governance_threshold: self.threshold,
            proposer_roles,
            registration_roles: Vec::new(),
        })
    }
}
pub async fn fetch_governance_state(
    identity: &NodeInstitutionIdentity,
) -> Result<GovernanceState, String> {
    let view = FinalizedChainView::connect().await?;
    let snapshot = registration_snapshot_at(&identity.cid_number, &view).await?;
    let mut state = snapshot.governance_state(&[])?;
    state.policy_available = supports_policy(&snapshot).await?;
    Ok(state)
}

/// 交易能力取该最终块上的Runtime元数据；状态读取仍使用原有父Runtime规则。
pub(crate) async fn supports_policy(snapshot: &RegistrationSnapshot) -> Result<bool, String> {
    if !entitybase::business_action::is_ordinary_institution(
        snapshot.institution.institution_code,
        &snapshot.cid_number,
    ) {
        return Ok(false);
    }
    let url = crate::chain_url::chain_ws_url()?;
    let client = crate::chain_url::rpc_client(&url)
        .await
        .map_err(|e| e.to_string())?;
    let rpc = subxt::backend::legacy::LegacyRpcMethods::<subxt::PolkadotConfig>::new(client);
    let bytes = rpc
        .state_get_metadata(Some(subxt::utils::H256(snapshot.block_hash)))
        .await
        .map_err(|e| e.to_string())?
        .into_raw();
    let metadata = subxt::Metadata::decode_all(&mut bytes.as_slice()).map_err(|e| e.to_string())?;
    let index = if snapshot.owner == EntityPallet::Private {
        31
    } else {
        30
    };
    Ok(metadata
        .pallet_by_index(index)
        .and_then(|p| p.call_variant_by_index(11))
        .is_some_and(|call| call.name == "propose_institution_policy"))
}
#[cfg(test)]
#[path = "../tests/institutions_roles_scale_contract_tests.rs"]
mod scale_contract_tests;
