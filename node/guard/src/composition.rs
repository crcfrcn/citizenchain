use adminbase::Admin;
use adminbase::InstitutionAdmins;
use codec::Decode;
use codec::Encode;
use entitybase::InstitutionAdminAssignment;
use entitybase::InstitutionAssignmentStatus;
use entitybase::InstitutionRole;
use entitybase::InstitutionRoleStatus;
use primitives::institution_constraints::member_composition_specs;
use primitives::institution_constraints::MemberCompositionSpec;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
#[cfg(test)]
use votebase::Proposal;

const INTERNAL_VOTE_PALLET: &[u8] = b"InternalVote";

const VOTING_ENGINE_PALLET: &[u8] = b"VotingEngine";

type DecodedAdminAccount = InstitutionAdmins<Vec<Admin<[u8; 32]>>>;

type DecodedRole = InstitutionRole<Vec<u8>, Vec<u8>, Vec<u8>>;

type DecodedAssignment = InstitutionAdminAssignment<Vec<u8>, [u8; 32], Vec<u8>, Vec<u8>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CompositionState {
    Unconstituted,
    Constituted,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum GuardError {
    PartialUnconstitutedState([u8; 4]),
    AdminAccountDecodeFailed([u8; 4]),
    AdminIdentityChanged([u8; 4]),
    InvalidAdminPersonName([u8; 4]),
    MemberRoleDecodeFailed([u8; 4]),
    MemberRoleChanged([u8; 4]),
    AssignmentsDecodeFailed([u8; 4]),
    MemberCountOutOfRange {
        code: [u8; 4],
        min: u32,
        max: u32,
        found: u32,
    },
    AssignmentChanged([u8; 4]),
    DuplicateMember([u8; 4]),
    AdminMemberSetMismatch([u8; 4]),
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
    let [admin_key, role_key, assignments_key] =
        crate::guard::national_body_composition::storage_key::composition_keys(spec);
    let raw = [read(&admin_key), read(&role_key), read(&assignments_key)];
    if raw.iter().all(Option::is_none) {
        return Ok(crate::guard::national_body_composition::CompositionState::Unconstituted);
    }
    if raw.iter().any(Option::is_none) {
        return Err(
            crate::guard::national_body_composition::GuardError::PartialUnconstitutedState(
                spec.institution.code,
            ),
        );
    }

    let account: DecodedAdminAccount = decode_exact(raw[0].as_deref().unwrap_or_default())
        .map_err(|_| {
            crate::guard::national_body_composition::GuardError::AdminAccountDecodeFailed(
                spec.institution.code,
            )
        })?;
    if account.institution_code != spec.institution.code {
        return Err(
            crate::guard::national_body_composition::GuardError::AdminIdentityChanged(
                spec.institution.code,
            ),
        );
    }
    if account.admins.iter().any(|admin| {
        admin.family_name.is_empty()
            || admin.given_name.is_empty()
            || core::str::from_utf8(admin.family_name.as_slice()).is_err()
            || core::str::from_utf8(admin.given_name.as_slice()).is_err()
    }) {
        return Err(
            crate::guard::national_body_composition::GuardError::InvalidAdminPersonName(
                spec.institution.code,
            ),
        );
    }

    let role: DecodedRole = decode_exact(raw[1].as_deref().unwrap_or_default()).map_err(|_| {
        crate::guard::national_body_composition::GuardError::MemberRoleDecodeFailed(
            spec.institution.code,
        )
    })?;
    if role.cid_number != spec.institution.cid_number.as_bytes()
        || role.role_code != spec.role_code
        || role.role_name != spec.role_name
        || role.role_status != InstitutionRoleStatus::Active
    {
        return Err(
            crate::guard::national_body_composition::GuardError::MemberRoleChanged(
                spec.institution.code,
            ),
        );
    }

    let assignments: Vec<DecodedAssignment> = decode_exact(raw[2].as_deref().unwrap_or_default())
        .map_err(|_| {
        crate::guard::national_body_composition::GuardError::AssignmentsDecodeFailed(
            spec.institution.code,
        )
    })?;
    let found = assignments.len() as u32;
    if found < spec.min_members || found > spec.max_members {
        return Err(
            crate::guard::national_body_composition::GuardError::MemberCountOutOfRange {
                code: spec.institution.code,
                min: spec.min_members,
                max: spec.max_members,
                found,
            },
        );
    }
    let mut members = BTreeSet::new();
    for assignment in assignments {
        if assignment.cid_number != spec.institution.cid_number.as_bytes()
            || assignment.role_code != spec.role_code
            || assignment.assignment_status != InstitutionAssignmentStatus::Active
        {
            return Err(
                crate::guard::national_body_composition::GuardError::AssignmentChanged(
                    spec.institution.code,
                ),
            );
        }
        if !members.insert(assignment.account_id) {
            return Err(
                crate::guard::national_body_composition::GuardError::DuplicateMember(
                    spec.institution.code,
                ),
            );
        }
    }
    let admins = account
        .admins
        .into_iter()
        .map(|admin| admin.account_id)
        .collect::<BTreeSet<_>>();
    if admins.len() != members.len() || admins != members {
        return Err(
            crate::guard::national_body_composition::GuardError::AdminMemberSetMismatch(
                spec.institution.code,
            ),
        );
    }
    Ok(crate::guard::national_body_composition::CompositionState::Constituted)
}

fn check_threshold<F>(key: &[u8], read: &F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let id = crate::guard::national_body_composition::storage_key::threshold_id(key)?;
    let proposal_key = crate::guard::national_body_composition::storage_key::proposal(id);
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
    let id = crate::guard::national_body_composition::storage_key::proposal_id(key)?;
    let Some(raw_proposal) = read(key) else {
        // 删除后的提案不再属于可分类的固定治理事项；终态清理由 runtime 负责。
        return Ok(());
    };
    if proposal_internal_code(&raw_proposal)?
        .and_then(|code| primitives::cid::code::fixed_governance_pass_threshold(&code))
        .is_some()
    {
        check_threshold(
            &crate::guard::national_body_composition::storage_key::threshold(id),
            read,
        )?;
    }
    Ok(())
}

/// 启动和 block#0 完整导入允许三个机构尚未组成，但不允许半组成或非法组成。
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

/// block#0 导入态复用完整状态校验；阈值表在创世为空。
pub(crate) fn check_imported_state(state: &BTreeMap<Vec<u8>, Vec<u8>>) -> Result<(), GuardError> {
    check_full_state(|key| state.get(key).cloned())?;
    check_vote_state_keys(state.keys(), |key| state.get(key).cloned())
}

/// 启动、完整状态导入和 runtime 升级时同时复核提案与快照，防止只移除一侧绕过。
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
    use primitives::institution_constraints::member_composition_specs;
    use primitives::institution_constraints::MemberCompositionSpec;
    use sp_crypto_hashing::blake2_128;

    fn prefix(pallet: &[u8], storage: &[u8]) -> Vec<u8> {
        nodedesktop::storage::prefix(pallet, storage)
    }

    fn map_u64(pallet: &[u8], storage: &[u8], id: u64) -> Vec<u8> {
        nodedesktop::storage::blake2_map(pallet, storage, &id.encode())
    }

    pub fn threshold_prefix() -> Vec<u8> {
        prefix(INTERNAL_VOTE_PALLET, b"InternalThresholdSnapshot")
    }

    pub fn proposal_prefix() -> Vec<u8> {
        prefix(VOTING_ENGINE_PALLET, b"Proposals")
    }

    pub fn proposal(id: u64) -> Vec<u8> {
        map_u64(VOTING_ENGINE_PALLET, b"Proposals", id)
    }

    pub fn threshold(id: u64) -> Vec<u8> {
        map_u64(INTERNAL_VOTE_PALLET, b"InternalThresholdSnapshot", id)
    }

    pub fn composition_keys(spec: &MemberCompositionSpec) -> [Vec<u8>; 3] {
        let institution = spec.institution;
        [
            crate::guard::governance_skeleton::storage_key::account_id(
                institution.cid_number.as_bytes(),
            ),
            crate::guard::governance_skeleton::storage_key::institution_role(
                institution.cid_number.as_bytes(),
                spec.role_code,
            ),
            crate::guard::governance_skeleton::storage_key::institution_role_assignments(
                institution.cid_number.as_bytes(),
                spec.role_code,
            ),
        ]
    }

    fn parse_map_id(key: &[u8], prefix: &[u8]) -> Result<u64, ()> {
        let encoded = key.strip_prefix(prefix).ok_or(())?;
        if encoded.len() != 24 || blake2_128(&encoded[16..]) != encoded[..16] {
            return Err(());
        }
        let mut input = &encoded[16..];
        let id = u64::decode(&mut input).map_err(|_| ())?;
        if !input.is_empty() {
            return Err(());
        }
        Ok(id)
    }

    /// 完整状态导入分区所需的 key：三个组成机构的精确 key，加内部阈值及提案表。
    pub fn is_relevant(key: &[u8]) -> bool {
        member_composition_specs()
            .iter()
            .flat_map(composition_keys)
            .any(|expected| expected == key)
            || key.starts_with(&threshold_prefix())
            || key.starts_with(&proposal_prefix())
    }

    pub(super) fn threshold_id(key: &[u8]) -> Result<u64, GuardError> {
        parse_map_id(key, &threshold_prefix())
            .map_err(|_| crate::guard::national_body_composition::GuardError::ThresholdKeyMalformed)
    }

    pub(super) fn proposal_id(key: &[u8]) -> Result<u64, GuardError> {
        parse_map_id(key, &proposal_prefix())
            .map_err(|_| crate::guard::national_body_composition::GuardError::ProposalKeyMalformed)
    }
}
#[cfg(test)]
#[path = "../tests/composition.rs"]
mod tests;
