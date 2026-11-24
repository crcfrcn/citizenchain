//! 固定同一最终块读取实体岗位、任职及identity当前绑定，全部SCALE使用所属共享类型。
use crate::roles::*;
use codec::{DecodeAll, Encode};
use entitybase::{
    InstitutionAssignment, InstitutionAssignmentSource, InstitutionAssignmentStatus,
    InstitutionRole, InstitutionRoleStatus, RoleBusinessPermission, RolePermissionOperation,
};
use std::collections::{BTreeMap, BTreeSet};
type RawRole = InstitutionRole<Vec<u8>, Vec<u8>, Vec<u8>>;
type RawAssignment = InstitutionAssignment<Vec<u8>, Vec<u8>, Vec<u8>>;
type RawPermission = RoleBusinessPermission<Vec<u8>, Vec<u8>, Vec<u8>>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityPallet {
    Public,
    Private,
}
impl EntityPallet {
    pub fn name(self) -> &'static str {
        match self {
            Self::Public => "PublicEntity",
            Self::Private => "PrivateEntity",
        }
    }
    pub fn kind(self) -> u8 {
        match self {
            Self::Public => 0,
            Self::Private => 1,
        }
    }
}
pub fn person_allowed(cid: &[u8], owner: EntityPallet) -> bool {
    if !std::str::from_utf8(cid).is_ok_and(|s| s.trim() == s) {
        return false;
    }
    primitives::cid::number::parse_cid_number_parts_bytes(cid).is_ok_and(|p| {
        p.institution == *b"CTZN" || owner == EntityPallet::Private && p.institution == *b"NATP"
    })
}
fn decode_exact<T: DecodeAll>(raw: &str, label: &str) -> Result<T, String> {
    let bytes = nodeclient::codec::decode_hex_storage(raw)?;
    T::decode_all(&mut bytes.as_slice()).map_err(|e| format!("{label} SCALE损坏:{e}"))
}
fn value<T: DecodeAll>(
    pallet: &str,
    item: &str,
    keys: &[Vec<u8>],
    at: &str,
) -> Result<Option<T>, String> {
    let key = storage_key(pallet, item, keys);
    nodeclient::query::fetch_storage_at(&key, at)?
        .map(|v| decode_exact(&v, item))
        .transpose()
}
fn storage_key(pallet: &str, item: &str, keys: &[Vec<u8>]) -> String {
    let mut bytes = nodedesktop::storage::prefix(pallet.as_bytes(), item.as_bytes());
    for key in keys {
        bytes.extend(nodedesktop::storage::blake2_128_concat(key));
    }
    nodedesktop::storage::to_hex(&bytes)
}
/// 允许调用方注入已固定块的只读入口；解码/闭环规则仍只有此处实现。
pub fn person_binding(
    person: &str,
    read: &mut impl FnMut(&str) -> Result<Option<String>, String>,
) -> Result<Option<(String, String)>, String> {
    if person.trim() != person {
        return Err("人员CID不是规范原值".into());
    }
    let parts = primitives::cid::number::parse_cid_number_parts_bytes(person.as_bytes())
        .map_err(|_| "人员CID无效")?;
    if parts.institution != *b"CTZN" && parts.institution != *b"NATP" {
        return Err("非人员CID不能解析签名绑定".into());
    }
    let mut get =
        |item: &str, key: Vec<u8>| read(&nodeclient::query::map_key("Identity", item, &key));
    let record = get("CidRegistry", person.as_bytes().to_vec().encode())?
        .map(|v| decode_exact::<identity::CidRecord<u32>>(&v, "CidRegistry"))
        .transpose()?;
    let Some(record) = record else {
        return Ok(None);
    };
    if record.status != identity::CidRecordStatus::Active {
        return Ok(None);
    }
    let Some(wallet) = get("AccountIdByCid", person.as_bytes().to_vec().encode())?
        .map(|v| decode_exact::<[u8; 32]>(&v, "AccountIdByCid"))
        .transpose()?
    else {
        return Ok(None);
    };
    let revision = get("BindingRevisionByCid", person.as_bytes().to_vec().encode())?
        .map(|v| decode_exact::<u64>(&v, "BindingRevisionByCid"))
        .transpose()?
        .filter(|r| *r > 0)
        .ok_or("有效人员缺少真实绑定版本")?;
    let reverse = get("CidByAccountId", wallet.encode())?
        .map(|v| decode_exact::<Vec<u8>>(&v, "CidByAccountId"))
        .transpose()?;
    if reverse.as_deref() != Some(person.as_bytes()) {
        return Err("人员绑定双向索引不一致".into());
    }
    Ok(Some((
        format!("0x{}", hex::encode(wallet)),
        revision.to_string(),
    )))
}
pub fn person_binding_at(person: &str, at: &str) -> Result<Option<(String, String)>, String> {
    person_binding(person, &mut |key| {
        nodeclient::query::fetch_storage_at(key, at)
    })
}

/// 姓名优先读取人员档案；已有创世公民缺档时，仅使用同块链上法代公开资料。
fn person_name(
    person: &str,
    read: &mut impl FnMut(&str) -> Result<Option<String>, String>,
) -> Result<(String, String), String> {
    let key = nodeclient::query::map_key(
        "Identity",
        "CandidateIdentityByCid",
        &person.as_bytes().to_vec().encode(),
    );
    if let Some(raw) = read(&key)? {
        let profile: identity::CandidateIdentity<u32> =
            decode_exact(&raw, "CandidateIdentityByCid")?;
        return Ok((
            utf8(profile.family_name.to_vec())?,
            utf8(profile.given_name.to_vec())?,
        ));
    }
    use primitives::cid::china::{china_zf::CHINA_ZF, citizenchain};
    if person != citizenchain::LEGAL_REPRESENTATIVE_CITIZEN_CID_NUMBER {
        return Ok((String::new(), String::new()));
    }
    let security = CHINA_ZF
        .iter()
        .find(|entry| {
            primitives::cid::code::institution_code_from_cid_number(entry.cid_number)
                == Some(primitives::cid::code::FSC)
        })
        .ok_or("联邦安全局身份缺失")?;
    let mut name = None;
    for (owner, cid) in [
        (
            EntityPallet::Private,
            citizenchain::CITIZENCHAIN_FOUNDATION.cid_number,
        ),
        (EntityPallet::Public, security.cid_number),
    ] {
        let key = storage_key(
            owner.name(),
            "Institutions",
            &[cid.as_bytes().to_vec().encode()],
        );
        let Some(raw) = read(&key)? else { continue };
        let info: entitybase::InstitutionInfo<u32, Vec<u8>, Vec<u8>> =
            decode_exact(&raw, "Institutions")?;
        if primitives::cid::code::institution_code_from_cid_number(cid)
            != Some(info.institution_code)
        {
            return Err("姓名来源机构码不一致".into());
        }
        let Some(representative) = info.legal_representative else {
            continue;
        };
        if representative.person_cid != person.as_bytes() {
            continue;
        }
        if representative.family_name.is_empty() || representative.given_name.is_empty() {
            return Err("法定代表人姓名不完整".into());
        }
        let current = (
            utf8(representative.family_name)?,
            utf8(representative.given_name)?,
        );
        if name.as_ref().is_some_and(|previous| previous != &current) {
            return Err("同一人员的法定代表人姓名不一致".into());
        }
        name = Some(current);
    }
    Ok(name.unwrap_or_default())
}
fn assignment_is_effective(role: &RawRole, a: &RawAssignment, day: u32) -> bool {
    role.role_status == InstitutionRoleStatus::Active
        && a.assignment_status == InstitutionAssignmentStatus::Active
        && if role.term_required {
            a.term_start > 0 && a.term_start <= day && day <= a.term_end
        } else {
            a.term_start == 0 && a.term_end == 0
        }
}
fn source_label(s: InstitutionAssignmentSource) -> &'static str {
    match s {
        InstitutionAssignmentSource::Genesis => "创世",
        InstitutionAssignmentSource::Registry => "注册局",
        InstitutionAssignmentSource::PopularElection => "普选",
        InstitutionAssignmentSource::MutualElection => "互选",
        InstitutionAssignmentSource::NominationAppointment => "提名任免",
        InstitutionAssignmentSource::InstitutionGovernance => "机构内部治理",
    }
}
fn permission_operation_label(s: RolePermissionOperation) -> &'static str {
    match s {
        RolePermissionOperation::Propose => "发起提案",
        RolePermissionOperation::Vote => "参与投票",
    }
}
fn display_source_ref(v: Vec<u8>) -> String {
    format!("0x{}", hex::encode(v))
}
fn utf8(v: Vec<u8>) -> Result<String, String> {
    String::from_utf8(v).map_err(|_| "链上文本非UTF-8".into())
}
fn keys_at(prefix: &str, at: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut start = None;
    loop {
        let page = nodeclient::query::fetch_keys_paged_at(prefix, 256, start.as_deref(), at)?;
        if page.is_empty() {
            break;
        }
        let last = page.last().cloned();
        if last == start {
            return Err("链上分页未前进".into());
        }
        let n = page.len();
        start = last;
        out.extend(page);
        if n < 256 {
            break;
        }
    }
    Ok(out)
}
pub fn entity_at(
    cid: &str,
    at: &str,
) -> Result<
    Option<(
        EntityPallet,
        entitybase::InstitutionInfo<u32, Vec<u8>, Vec<u8>>,
    )>,
    String,
> {
    if cid.trim() != cid {
        return Err("机构CID不是规范原值".into());
    }
    let parts = primitives::cid::number::parse_cid_number_parts_bytes(cid.as_bytes())
        .map_err(|_| "机构CID无效")?;
    let mut found = None;
    for owner in [EntityPallet::Public, EntityPallet::Private] {
        if let Some(info) = value::<entitybase::InstitutionInfo<u32, Vec<u8>, Vec<u8>>>(
            owner.name(),
            "Institutions",
            &[cid.as_bytes().to_vec().encode()],
            at,
        )? {
            if found.is_some() || info.institution_code != parts.institution {
                return Err("机构CID命名空间或机构码不唯一".into());
            }
            if !primitives::cid::code::is_unincorporated_code(&parts.institution)
                && (owner == EntityPallet::Private)
                    != primitives::cid::code::is_private_legal_code(&parts.institution)
            {
                return Err("机构所属实体不匹配".into());
            }
            found = Some((owner, info));
        }
    }
    Ok(found)
}
pub fn fetch_roles(cid: &str) -> Result<Option<InstitutionRolesState>, String> {
    let at = nodeclient::query::fetch_finalized_head()?;
    fetch_roles_at(cid, &at)
}
pub fn fetch_roles_at(cid: &str, at: &str) -> Result<Option<InstitutionRolesState>, String> {
    let Some((owner, info)) = entity_at(cid, at)? else {
        return Ok(None);
    };
    let now = value::<u64>("Timestamp", "Now", &[], at)?.ok_or("最终块时间缺失")?;
    let day = u32::try_from(now / 86_400_000).map_err(|_| "链上日期超限")?;
    let prefix = storage_key(
        owner.name(),
        "InstitutionRoles",
        &[cid.as_bytes().to_vec().encode()],
    );
    let mut roles = BTreeMap::new();
    let mut people = BTreeMap::new();
    for key in keys_at(&prefix, at)? {
        let raw = nodeclient::query::fetch_storage_at(&key, at)?.ok_or("同块岗位值缺失")?;
        let role: RawRole = decode_exact(&raw, "InstitutionRoles")?;
        if role.cid_number != cid.as_bytes()
            || role.seats == 0
            || primitives::institution_constraints::is_legal_representative_role(&role.role_code)
                && role.seats != 1
        {
            return Err("岗位主体或席位无效".into());
        }
        let role_key = role.role_code.clone();
        let encoded = [cid.as_bytes().to_vec().encode(), role_key.encode()];
        let perms =
            value::<Vec<RawPermission>>(owner.name(), "InstitutionRolePermissions", &encoded, at)?
                .unwrap_or_default();
        if perms.iter().any(|p| {
            p.role_subject.cid_number != cid.as_bytes() || p.role_subject.role_code != role_key
        }) {
            return Err("岗位权限主体不一致".into());
        }
        let rows =
            value::<Vec<RawAssignment>>(owner.name(), "InstitutionRoleAssignments", &encoded, at)?
                .unwrap_or_default();
        if rows.len() > role.seats as usize {
            return Err("任职超过岗位席位容量".into());
        }
        let mut seen = BTreeSet::new();
        let mut assignments = Vec::new();
        for a in rows {
            if a.cid_number != cid.as_bytes()
                || a.role_code != role_key
                || !person_allowed(&a.person_cid, owner)
                || !seen.insert(a.person_cid.clone())
            {
                return Err("岗位任职身份无效或重复".into());
            }
            let effective = assignment_is_effective(&role, &a, day);
            let person = utf8(a.person_cid)?;
            if !people.contains_key(&person) {
                people.insert(person.clone(), person_binding_at(&person, at)?);
            }
            let binding = people[&person].clone();
            let (family_name, given_name) = person_name(&person, &mut |key| {
                nodeclient::query::fetch_storage_at(key, at)
            })?;
            assignments.push(InstitutionAssignmentInfo {
                person_cid: person,
                account_id: binding.as_ref().map(|b| b.0.clone()),
                binding_revision: binding.map(|b| b.1),
                family_name,
                given_name,
                term_start: a.term_start,
                term_end: a.term_end,
                assignment_source: a.assignment_source as u8,
                assignment_source_label: source_label(a.assignment_source).into(),
                assignment_source_ref: display_source_ref(a.assignment_source_ref),
                active: a.assignment_status == InstitutionAssignmentStatus::Active,
                effective,
            });
        }
        let item = InstitutionRoleInfo {
            role_code: utf8(role_key.clone())?,
            role_name: utf8(role.role_name)?,
            term_required: role.term_required,
            seats: role.seats,
            active: role.role_status == InstitutionRoleStatus::Active,
            permissions: perms
                .into_iter()
                .map(|p| {
                    Ok(InstitutionRolePermissionInfo {
                        module_tag: utf8(p.business_action_id.module_tag)?,
                        action_code: p.business_action_id.action_code,
                        operation: p.operation as u8,
                        operation_label: permission_operation_label(p.operation).into(),
                    })
                })
                .collect::<Result<_, String>>()?,
            assignments,
        };
        if roles.insert(role_key, item).is_some() {
            return Err("岗位码重复".into());
        }
    }
    Ok(Some(InstitutionRolesState {
        cid_number: cid.into(),
        institution_code: info.institution_code,
        kind: owner.kind(),
        block_hash: at.into(),
        roles: roles.into_values().collect(),
    }))
}
#[cfg(test)]
#[path = "../tests/storage.rs"]
mod tests;
