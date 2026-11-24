//! 核对受保护机构实体、岗位容量和人员CID任职结构。

use codec::Decode;
use codec::Encode;
use entitybase::InstitutionAssignment;
use entitybase::InstitutionAssignmentStatus;
use entitybase::InstitutionRole;
use entitybase::InstitutionRoleStatus;
use primitives::institution_constraints::member_composition_specs;
use primitives::institution_constraints::MemberCompositionSpec;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

const INTERNAL_VOTE_PALLET: &[u8] = b"InternalVote";

const VOTING_ENGINE_PALLET: &[u8] = b"VotingEngine";

type DecodedInstitution = entitybase::InstitutionInfo<u32, Vec<u8>, Vec<u8>>;

type DecodedRole = InstitutionRole<Vec<u8>, Vec<u8>, Vec<u8>>;

type DecodedAssignment = InstitutionAssignment<Vec<u8>, Vec<u8>, Vec<u8>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CompositionState {
    Unconstituted,
    Constituted,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum GuardError {
    PartialUnconstitutedState([u8; 4]),
    InstitutionDecodeFailed([u8; 4]),
    InstitutionIdentityChanged([u8; 4]),
    PersonCidInvalid([u8; 4]),
    MemberRoleDecodeFailed([u8; 4]),
    MemberRoleChanged([u8; 4]),
    AssignmentsDecodeFailed([u8; 4]),
    PermissionsChanged([u8; 4]),
    MemberCountOutOfRange {
        code: [u8; 4],
        min: u32,
        max: u32,
        found: u32,
    },
    AssignmentChanged([u8; 4]),
    DuplicateMember([u8; 4]),
    ConstitutedBodyRemoved([u8; 4]),
    ThresholdKeyMalformed,
    ProposalKeyMalformed,
    ThresholdDecodeFailed,
    ProposalDecodeFailed,
    FixedThresholdMissing([u8; 4]),
    FixedThresholdChanged {
        code: [u8; 4],
        expected: u32,
        found: u32,
    },
    RuntimeUpgradeVoteKeysMissing,
}

fn decode_exact<T: Decode>(raw: &[u8]) -> Result<T, ()> {
    let mut input = raw;
    let value = T::decode(&mut input).map_err(|_| ())?;
    if !input.is_empty() {
        return Err(());
    }
    Ok(value)
}

/// 只读取投票引擎提案稳定分类前缀，不解码业务字段，避免把普通提案完整布局冻结进 NodeGuard。
fn proposal_internal_code(raw: &[u8]) -> Result<Option<[u8; 4]>, GuardError> {
    let mut input = raw;
    u8::decode(&mut input)
        .map_err(|_| crate::guard::national_body_composition::GuardError::ProposalDecodeFailed)?;
    u8::decode(&mut input)
        .map_err(|_| crate::guard::national_body_composition::GuardError::ProposalDecodeFailed)?;
    u8::decode(&mut input)
        .map_err(|_| crate::guard::national_body_composition::GuardError::ProposalDecodeFailed)?;
    Option::<[u8; 4]>::decode(&mut input)
        .map_err(|_| crate::guard::national_body_composition::GuardError::ProposalDecodeFailed)
}

fn composition_state<F>(
    spec: &MemberCompositionSpec,
    read: &F,
) -> Result<CompositionState, GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let [institution_key, role_key, assignments_key, permission_key] =
        storage_key::composition_keys(spec);
    let institution = read(&institution_key);
    let role = read(&role_key);
    let rows = read(&assignments_key);
    let permissions = read(&permission_key);
    if institution.is_none() && role.is_none() && rows.is_none() && permissions.is_none() {
        return Ok(CompositionState::Unconstituted);
    }
    let Some(institution) = institution else {
        return Err(GuardError::PartialUnconstitutedState(spec.institution.code));
    };
    let institution: DecodedInstitution = decode_exact(&institution)
        .map_err(|_| GuardError::InstitutionDecodeFailed(spec.institution.code))?;
    if institution.institution_code != spec.institution.code {
        return Err(GuardError::InstitutionIdentityChanged(
            spec.institution.code,
        ));
    }
    // 创世先登记机构与法代岗位；成员岗位尚未建立时仍是合法的未组成状态。
    // 任职或权限已经存在却没有对应岗位，仍按孤立数据拒绝。
    if role.is_none() && rows.is_none() && permissions.is_none() {
        return Ok(CompositionState::Unconstituted);
    }
    let Some(role) = role else {
        return Err(GuardError::PartialUnconstitutedState(spec.institution.code));
    };
    let role: DecodedRole = decode_exact(&role)
        .map_err(|_| GuardError::MemberRoleDecodeFailed(spec.institution.code))?;
    if role.cid_number != spec.institution.cid_number.as_bytes()
        || role.role_code != spec.role_code
        || role.role_name != spec.role_name
        || role.role_status != InstitutionRoleStatus::Active
        || role.seats != spec.max_members
    {
        return Err(GuardError::MemberRoleChanged(spec.institution.code));
    }
    let permissions: Vec<entitybase::RoleBusinessPermission<Vec<u8>, Vec<u8>, Vec<u8>>> =
        permissions
            .map(|raw| decode_exact(&raw))
            .transpose()
            .map_err(|_| GuardError::PermissionsChanged(spec.institution.code))?
            .unwrap_or_default();
    let mut permission_keys = BTreeSet::new();
    if permissions.iter().any(|p| {
        p.role_subject.cid_number != role.cid_number
            || p.role_subject.role_code != role.role_code
            || !permission_keys.insert(p.encode())
    }) {
        return Err(GuardError::PermissionsChanged(spec.institution.code));
    }
    // 各业务的具体发起和表决权限仍由所属Runtime入口按同一RoleSubject裁决。
    // ValueQuery的缺失任职键就是空任职；机构资料和规则岗位必须真实存在。
    let assignments: Vec<DecodedAssignment> = rows
        .map(|raw| decode_exact(&raw))
        .transpose()
        .map_err(|_| GuardError::AssignmentsDecodeFailed(spec.institution.code))?
        .unwrap_or_default();
    if assignments.is_empty() {
        return Ok(CompositionState::Unconstituted);
    }
    let found = assignments.len() as u32;
    if found < spec.min_members || found > spec.max_members {
        return Err(GuardError::MemberCountOutOfRange {
            code: spec.institution.code,
            min: spec.min_members,
            max: spec.max_members,
            found,
        });
    }
    let mut members = BTreeSet::new();
    for a in assignments {
        if a.cid_number != spec.institution.cid_number.as_bytes()
            || a.role_code != spec.role_code
            || a.assignment_status != InstitutionAssignmentStatus::Active
            || (if role.term_required {
                a.term_start == 0 || a.term_end < a.term_start
            } else {
                a.term_start != 0 || a.term_end != 0
            })
        {
            return Err(GuardError::AssignmentChanged(spec.institution.code));
        }
        if !std::str::from_utf8(&a.person_cid).is_ok_and(|s| s.trim() == s)
            || !primitives::cid::number::parse_cid_number_parts_bytes(&a.person_cid)
                .is_ok_and(|p| p.institution == *b"CTZN")
        {
            return Err(GuardError::PersonCidInvalid(spec.institution.code));
        }
        if !members.insert(a.person_cid) {
            return Err(GuardError::DuplicateMember(spec.institution.code));
        }
    }
    Ok(crate::guard::national_body_composition::CompositionState::Constituted)
}

fn check_threshold<F>(key: &[u8], read: &F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let proposal_key = storage_key::proposal_for_threshold(key)?;
    let Some(raw_proposal) = read(&proposal_key) else {
        // 没有提案就无法归类为固定治理事项；普通投票的清理由 runtime 负责。
        return Ok(());
    };
    let Some(code) = proposal_internal_code(&raw_proposal)? else {
        return Ok(());
    };
    let Some(expected) = primitives::cid::code::fixed_governance_pass_threshold(&code) else {
        return Ok(());
    };
    let raw_threshold = read(key)
        .ok_or(crate::guard::national_body_composition::GuardError::FixedThresholdMissing(code))?;
    let threshold: u32 = decode_exact(&raw_threshold)
        .map_err(|_| crate::guard::national_body_composition::GuardError::ThresholdDecodeFailed)?;
    // 五类固定治理码不能在创世后新建同码机构，FRG 又以省岗位组账户作为投票上下文；
    // 因此阈值守卫按固定治理码覆盖全部合法上下文，不错误限定为 FRG 主账户。
    if threshold != expected {
        return Err(
            crate::guard::national_body_composition::GuardError::FixedThresholdChanged {
                code,
                expected,
                found: threshold,
            },
        );
    }
    Ok(())
}

/// 固定治理提案必须继续使用规范内部阈值快照；普通机构和六个国家单例不施加固定值。
fn check_proposal<F>(key: &[u8], read: &F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let threshold_key = storage_key::threshold_for_proposal(key)?;
    let Some(raw_proposal) = read(key) else {
        // 删除后的提案不再属于可分类的固定治理事项；终态清理由 runtime 负责。
        return Ok(());
    };
    if proposal_internal_code(&raw_proposal)?
        .and_then(|code| primitives::cid::code::fixed_governance_pass_threshold(&code))
        .is_some()
    {
        check_threshold(&threshold_key, read)?;
    }
    Ok(())
}

/// 启动允许三个机构尚未组成，但不允许半组成或非法组成。
pub(crate) fn check_full_state<F>(read: F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    for spec in member_composition_specs() {
        composition_state(&spec, &read)?;
    }
    Ok(())
}

/// 普通区块按 delta 检查组成状态单向性，并双向复核提案与固定阈值快照。
pub(crate) fn check_transition<FParent, FPost>(
    delta: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    parent: FParent,
    post: FPost,
    runtime_upgrade_vote_keys: Option<&[Vec<u8>]>,
) -> Result<(), GuardError>
where
    FParent: Fn(&[u8]) -> Option<Vec<u8>>,
    FPost: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let code_changed = delta.contains_key(sp_storage::well_known_keys::CODE);
    for spec in member_composition_specs() {
        let affected =
            crate::guard::national_body_composition::storage_key::composition_keys(&spec)
                .iter()
                .any(|key| delta.contains_key(key));
        if affected || code_changed {
            let before = composition_state(&spec, &parent)?;
            let after = composition_state(&spec, &post)?;
            if before == crate::guard::national_body_composition::CompositionState::Constituted
                && after == crate::guard::national_body_composition::CompositionState::Unconstituted
            {
                return Err(
                    crate::guard::national_body_composition::GuardError::ConstitutedBodyRemoved(
                        spec.institution.code,
                    ),
                );
            }
        }
    }
    for key in delta.keys().filter(|key| {
        key.starts_with(&crate::guard::national_body_composition::storage_key::threshold_prefix())
    }) {
        check_threshold(key, &post)?;
    }
    for key in delta.keys().filter(|key| {
        key.starts_with(&crate::guard::national_body_composition::storage_key::proposal_prefix())
    }) {
        check_proposal(key, &post)?;
    }
    if code_changed {
        let keys = runtime_upgrade_vote_keys.ok_or(
            crate::guard::national_body_composition::GuardError::RuntimeUpgradeVoteKeysMissing,
        )?;
        check_vote_state_keys(keys, &post)?;
    }
    Ok(())
}

/// 启动和 runtime 升级时同时复核提案与快照，防止只移除一侧绕过。
pub(crate) fn check_vote_state_keys<I, K, F>(keys: I, read: F) -> Result<(), GuardError>
where
    I: IntoIterator<Item = K>,
    K: AsRef<[u8]>,
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    for key in keys {
        let key = key.as_ref();
        if key
            .starts_with(&crate::guard::national_body_composition::storage_key::threshold_prefix())
        {
            check_threshold(key, &read)?;
        } else if key
            .starts_with(&crate::guard::national_body_composition::storage_key::proposal_prefix())
        {
            check_proposal(key, &read)?;
        }
    }
    Ok(())
}

pub mod storage_key {
    use super::*;
    use crate::guard::national_body_composition::GuardError;
    use crate::guard::national_body_composition::INTERNAL_VOTE_PALLET;
    use crate::guard::national_body_composition::VOTING_ENGINE_PALLET;
    use primitives::institution_constraints::MemberCompositionSpec;
    use sp_crypto_hashing::blake2_128;

    fn prefix(pallet: &[u8], storage: &[u8]) -> Vec<u8> {
        nodedesktop::storage::prefix(pallet, storage)
    }

    fn map_proposal(pallet: &[u8], storage: &[u8], id: u128) -> Vec<u8> {
        nodedesktop::storage::blake2_map(pallet, storage, &id.encode())
    }

    pub fn threshold_prefix() -> Vec<u8> {
        prefix(INTERNAL_VOTE_PALLET, b"InternalThresholdSnapshot")
    }

    pub fn proposal_prefix() -> Vec<u8> {
        prefix(VOTING_ENGINE_PALLET, b"Proposals")
    }

    pub fn proposal(id: u128) -> Vec<u8> {
        map_proposal(VOTING_ENGINE_PALLET, b"Proposals", id)
    }

    pub fn threshold(id: u128) -> Vec<u8> {
        map_proposal(INTERNAL_VOTE_PALLET, b"InternalThresholdSnapshot", id)
    }

    pub fn institution(cid: &[u8]) -> Vec<u8> {
        nodedesktop::storage::blake2_map(b"PublicEntity", b"Institutions", &cid.to_vec().encode())
    }
    pub fn institution_role(cid: &[u8], role: &[u8]) -> Vec<u8> {
        role_key(b"InstitutionRoles", cid, role)
    }
    pub fn institution_role_assignments(cid: &[u8], role: &[u8]) -> Vec<u8> {
        role_key(b"InstitutionRoleAssignments", cid, role)
    }
    fn role_key(table: &[u8], cid: &[u8], role: &[u8]) -> Vec<u8> {
        let mut key = nodedesktop::storage::prefix(b"PublicEntity", table);
        for value in [cid, role] {
            key.extend(nodedesktop::storage::blake2_128_concat(
                &value.to_vec().encode(),
            ));
        }
        key
    }

    pub fn composition_keys(spec: &MemberCompositionSpec) -> [Vec<u8>; 4] {
        let institution = spec.institution;
        [
            crate::guard::national_body_composition::storage_key::institution(
                institution.cid_number.as_bytes(),
            ),
            crate::guard::national_body_composition::storage_key::institution_role(
                institution.cid_number.as_bytes(),
                spec.role_code,
            ),
            crate::guard::national_body_composition::storage_key::institution_role_assignments(
                institution.cid_number.as_bytes(),
                spec.role_code,
            ),
            role_key(
                b"InstitutionRolePermissions",
                institution.cid_number.as_bytes(),
                spec.role_code,
            ),
        ]
    }

    // 只关联相同哈希后缀，不解析或冻结 runtime 的业务编号字段类型。
    fn paired_key(key: &[u8], from: &[u8], to: Vec<u8>) -> Result<Vec<u8>, ()> {
        let suffix = key.strip_prefix(from).ok_or(())?;
        if suffix.len() <= 16 || blake2_128(&suffix[16..]) != suffix[..16] {
            return Err(());
        }
        Ok([to, suffix.to_vec()].concat())
    }
    pub(super) fn proposal_for_threshold(key: &[u8]) -> Result<Vec<u8>, GuardError> {
        paired_key(key, &threshold_prefix(), proposal_prefix())
            .map_err(|_| GuardError::ThresholdKeyMalformed)
    }
    pub(super) fn threshold_for_proposal(key: &[u8]) -> Result<Vec<u8>, GuardError> {
        paired_key(key, &proposal_prefix(), threshold_prefix())
            .map_err(|_| GuardError::ProposalKeyMalformed)
    }
}
#[cfg(test)]
#[path = "../tests/composition.rs"]
mod tests;
