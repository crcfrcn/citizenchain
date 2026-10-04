//! 国家级成员机构组成与固定内部阈值的节点永久策略。
//!
//! NSN、NRP、NED 在 block#0 可以尚未组成；首次组成后，指定成员岗位、人数区间以及
//! admins 中的账户集合必须等于成员账户集合，且不得退回未组成状态。NLG、NSP、PRS 只冻结身份，
//! 不冻结岗位或 admins；六个国家级单例均不冻结账户级动态阈值。五类固定治理机构的
//! 内部提案阈值快照按固定治理码复核。

use std::collections::{BTreeMap, BTreeSet};

use admin_primitives::{Admin, InstitutionAdmins};
use codec::{Decode, Encode};
use entity_primitives::{
    InstitutionAdminAssignment, InstitutionAssignmentStatus, InstitutionRole, InstitutionRoleStatus,
};
use primitives::institution_constraints::{member_composition_specs, MemberCompositionSpec};
#[cfg(test)]
use votingengine::Proposal;

use super::governance_skeleton;

const INTERNAL_VOTE_PALLET: &[u8] = b"InternalVote";
const VOTING_ENGINE_PALLET: &[u8] = b"VotingEngine";

type DecodedAdminAccount = InstitutionAdmins<Vec<Admin<[u8; 32]>>>;
type DecodedRole = InstitutionRole<Vec<u8>, Vec<u8>, Vec<u8>>;
type DecodedAssignment = InstitutionAdminAssignment<Vec<u8>, [u8; 32], Vec<u8>, Vec<u8>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CompositionState {
    Unconstituted,
    Constituted,
}

#[derive(Debug, Eq, PartialEq)]
pub enum GuardError {
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

pub mod storage_key {
    use super::*;
    use sp_crypto_hashing::blake2_128;

    fn prefix(pallet: &[u8], storage: &[u8]) -> Vec<u8> {
        crate::shared::storage_keys::prefix(pallet, storage)
    }

    fn map_u64(pallet: &[u8], storage: &[u8], id: u64) -> Vec<u8> {
        crate::shared::storage_keys::blake2_map(pallet, storage, &id.encode())
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
            governance_skeleton::storage_key::account_id(institution.cid_number.as_bytes()),
            governance_skeleton::storage_key::institution_role(
                institution.cid_number.as_bytes(),
                spec.role_code,
            ),
            governance_skeleton::storage_key::institution_role_assignments(
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
        parse_map_id(key, &threshold_prefix()).map_err(|_| GuardError::ThresholdKeyMalformed)
    }

    pub(super) fn proposal_id(key: &[u8]) -> Result<u64, GuardError> {
        parse_map_id(key, &proposal_prefix()).map_err(|_| GuardError::ProposalKeyMalformed)
    }
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
    u8::decode(&mut input).map_err(|_| GuardError::ProposalDecodeFailed)?;
    u8::decode(&mut input).map_err(|_| GuardError::ProposalDecodeFailed)?;
    u8::decode(&mut input).map_err(|_| GuardError::ProposalDecodeFailed)?;
    Option::<[u8; 4]>::decode(&mut input).map_err(|_| GuardError::ProposalDecodeFailed)
}

fn composition_state<F>(
    spec: &MemberCompositionSpec,
    read: &F,
) -> Result<CompositionState, GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let [admin_key, role_key, assignments_key] = storage_key::composition_keys(spec);
    let raw = [read(&admin_key), read(&role_key), read(&assignments_key)];
    if raw.iter().all(Option::is_none) {
        return Ok(CompositionState::Unconstituted);
    }
    if raw.iter().any(Option::is_none) {
        return Err(GuardError::PartialUnconstitutedState(spec.institution.code));
    }

    let account: DecodedAdminAccount = decode_exact(raw[0].as_deref().unwrap_or_default())
        .map_err(|_| GuardError::AdminAccountDecodeFailed(spec.institution.code))?;
    if account.institution_code != spec.institution.code {
        return Err(GuardError::AdminIdentityChanged(spec.institution.code));
    }
    if account.admins.iter().any(|admin| {
        admin.family_name.is_empty()
            || admin.given_name.is_empty()
            || core::str::from_utf8(admin.family_name.as_slice()).is_err()
            || core::str::from_utf8(admin.given_name.as_slice()).is_err()
    }) {
        return Err(GuardError::InvalidAdminPersonName(spec.institution.code));
    }

    let role: DecodedRole = decode_exact(raw[1].as_deref().unwrap_or_default())
        .map_err(|_| GuardError::MemberRoleDecodeFailed(spec.institution.code))?;
    if role.cid_number != spec.institution.cid_number.as_bytes()
        || role.role_code != spec.role_code
        || role.role_name != spec.role_name
        || role.role_status != InstitutionRoleStatus::Active
    {
        return Err(GuardError::MemberRoleChanged(spec.institution.code));
    }

    let assignments: Vec<DecodedAssignment> =
        decode_exact(raw[2].as_deref().unwrap_or_default())
            .map_err(|_| GuardError::AssignmentsDecodeFailed(spec.institution.code))?;
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
    for assignment in assignments {
        if assignment.cid_number != spec.institution.cid_number.as_bytes()
            || assignment.role_code != spec.role_code
            || assignment.assignment_status != InstitutionAssignmentStatus::Active
        {
            return Err(GuardError::AssignmentChanged(spec.institution.code));
        }
        if !members.insert(assignment.account_id) {
            return Err(GuardError::DuplicateMember(spec.institution.code));
        }
    }
    let admins = account
        .admins
        .into_iter()
        .map(|admin| admin.account_id)
        .collect::<BTreeSet<_>>();
    if admins.len() != members.len() || admins != members {
        return Err(GuardError::AdminMemberSetMismatch(spec.institution.code));
    }
    Ok(CompositionState::Constituted)
}

fn check_threshold<F>(key: &[u8], read: &F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let id = storage_key::threshold_id(key)?;
    let proposal_key = storage_key::proposal(id);
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
    let raw_threshold = read(key).ok_or(GuardError::FixedThresholdMissing(code))?;
    let threshold: u32 =
        decode_exact(&raw_threshold).map_err(|_| GuardError::ThresholdDecodeFailed)?;
    // 五类固定治理码不能在创世后新建同码机构，FRG 又以省岗位组账户作为投票上下文；
    // 因此阈值守卫按固定治理码覆盖全部合法上下文，不错误限定为 FRG 主账户。
    if threshold != expected {
        return Err(GuardError::FixedThresholdChanged {
            code,
            expected,
            found: threshold,
        });
    }
    Ok(())
}

/// 固定治理提案必须继续使用规范内部阈值快照；普通机构和六个国家单例不施加固定值。
fn check_proposal<F>(key: &[u8], read: &F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    let id = storage_key::proposal_id(key)?;
    let Some(raw_proposal) = read(key) else {
        // 删除后的提案不再属于可分类的固定治理事项；终态清理由 runtime 负责。
        return Ok(());
    };
    if proposal_internal_code(&raw_proposal)?
        .and_then(|code| primitives::cid::code::fixed_governance_pass_threshold(&code))
        .is_some()
    {
        check_threshold(&storage_key::threshold(id), read)?;
    }
    Ok(())
}

/// 启动和 block#0 完整导入允许三个机构尚未组成，但不允许半组成或非法组成。
pub fn check_full_state<F>(read: F) -> Result<(), GuardError>
where
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    for spec in member_composition_specs() {
        composition_state(&spec, &read)?;
    }
    Ok(())
}

/// 普通区块按 delta 检查组成状态单向性，并双向复核提案与固定阈值快照。
pub fn check_transition<FParent, FPost>(
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
        let affected = storage_key::composition_keys(&spec)
            .iter()
            .any(|key| delta.contains_key(key));
        if affected || code_changed {
            let before = composition_state(&spec, &parent)?;
            let after = composition_state(&spec, &post)?;
            if before == CompositionState::Constituted && after == CompositionState::Unconstituted {
                return Err(GuardError::ConstitutedBodyRemoved(spec.institution.code));
            }
        }
    }
    for key in delta
        .keys()
        .filter(|key| key.starts_with(&storage_key::threshold_prefix()))
    {
        check_threshold(key, &post)?;
    }
    for key in delta
        .keys()
        .filter(|key| key.starts_with(&storage_key::proposal_prefix()))
    {
        check_proposal(key, &post)?;
    }
    if code_changed {
        let keys = runtime_upgrade_vote_keys.ok_or(GuardError::RuntimeUpgradeVoteKeysMissing)?;
        check_vote_state_keys(keys, &post)?;
    }
    Ok(())
}

/// block#0 导入态复用完整状态校验；阈值表在创世为空。
pub fn check_imported_state(state: &BTreeMap<Vec<u8>, Vec<u8>>) -> Result<(), GuardError> {
    check_full_state(|key| state.get(key).cloned())?;
    check_vote_state_keys(state.keys(), |key| state.get(key).cloned())
}

/// 启动、完整状态导入和 runtime 升级时同时复核提案与快照，防止只移除一侧绕过。
pub fn check_vote_state_keys<I, K, F>(keys: I, read: F) -> Result<(), GuardError>
where
    I: IntoIterator<Item = K>,
    K: AsRef<[u8]>,
    F: Fn(&[u8]) -> Option<Vec<u8>>,
{
    for key in keys {
        let key = key.as_ref();
        if key.starts_with(&storage_key::threshold_prefix()) {
            check_threshold(key, &read)?;
        } else if key.starts_with(&storage_key::proposal_prefix()) {
            check_proposal(key, &read)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use entity_primitives::InstitutionAssignmentSource;
    use votingengine::types::{ProposalSubjectCidNumbers, PROPOSAL_KIND_INTERNAL, STAGE_INTERNAL};

    fn account(index: u32) -> [u8; 32] {
        let mut account = [0u8; 32];
        account[..4].copy_from_slice(&index.to_le_bytes());
        account
    }

    fn formed_state(spec: &MemberCompositionSpec, count: u32) -> BTreeMap<Vec<u8>, Vec<u8>> {
        let admin_accounts = (0..count).map(account).collect::<Vec<_>>();
        let role = DecodedRole {
            cid_number: spec.institution.cid_number.as_bytes().to_vec(),
            role_code: spec.role_code.to_vec(),
            role_name: spec.role_name.to_vec(),
            term_required: false,
            role_status: InstitutionRoleStatus::Active,
        };
        let assignments = admin_accounts
            .iter()
            .copied()
            .map(|account_id| DecodedAssignment {
                cid_number: spec.institution.cid_number.as_bytes().to_vec(),
                account_id,
                role_code: spec.role_code.to_vec(),
                term_start: 0,
                term_end: 0,
                assignment_source: InstitutionAssignmentSource::PopularElection,
                assignment_source_ref: b"election".to_vec(),
                assignment_status: InstitutionAssignmentStatus::Active,
            })
            .collect::<Vec<_>>();
        let account = DecodedAdminAccount {
            institution_code: spec.institution.code,
            admins: admin_accounts
                .into_iter()
                .map(|account_id| Admin {
                    account_id,
                    // Phase 1: 公民 CID 与创世一致留空。
                    cid_number: Default::default(),
                    family_name: "管理".as_bytes().to_vec().try_into().expect("name fits"),
                    given_name: "员".as_bytes().to_vec().try_into().expect("name fits"),
                })
                .collect(),
        };
        let [admin_key, role_key, assignments_key] = storage_key::composition_keys(spec);
        BTreeMap::from([
            (admin_key, account.encode()),
            (role_key, role.encode()),
            (assignments_key, assignments.encode()),
        ])
    }

    #[test]
    fn unconstituted_and_valid_constituted_states_pass() {
        assert_eq!(check_full_state(|_| None), Ok(()));
        let mut state = BTreeMap::new();
        for spec in member_composition_specs() {
            state.extend(formed_state(&spec, spec.min_members));
        }
        assert_eq!(check_full_state(|key| state.get(key).cloned()), Ok(()));
    }

    #[test]
    fn member_names_do_not_define_membership_but_must_be_valid() {
        let spec = member_composition_specs()[0];
        let admin_key = storage_key::composition_keys(&spec)[0].clone();
        let mut state = formed_state(&spec, spec.min_members);
        let mut account: DecodedAdminAccount =
            decode_exact(state.get(&admin_key).expect("admin account exists"))
                .expect("admin account decodes");
        account.admins[0].family_name = "张".as_bytes().to_vec().try_into().expect("name fits");
        account.admins[0].given_name = "三".as_bytes().to_vec().try_into().expect("name fits");
        state.insert(admin_key.clone(), account.encode());
        assert_eq!(check_full_state(|key| state.get(key).cloned()), Ok(()));

        let mut account: DecodedAdminAccount =
            decode_exact(state.get(&admin_key).expect("admin account exists"))
                .expect("admin account decodes");
        account.admins[0].given_name = Vec::new().try_into().expect("empty name fits");
        state.insert(admin_key, account.encode());
        assert_eq!(
            check_full_state(|key| state.get(key).cloned()),
            Err(GuardError::InvalidAdminPersonName(spec.institution.code))
        );
    }

    #[test]
    fn partial_or_out_of_range_composition_is_rejected() {
        let spec = member_composition_specs()[0];
        let mut partial = formed_state(&spec, spec.min_members);
        partial.remove(&storage_key::composition_keys(&spec)[1]);
        assert_eq!(
            composition_state(&spec, &|key| partial.get(key).cloned()),
            Err(GuardError::PartialUnconstitutedState(spec.institution.code))
        );

        let below = formed_state(&spec, spec.min_members - 1);
        assert_eq!(
            composition_state(&spec, &|key| below.get(key).cloned()),
            Err(GuardError::MemberCountOutOfRange {
                code: spec.institution.code,
                min: spec.min_members,
                max: spec.max_members,
                found: spec.min_members - 1,
            })
        );
    }

    #[test]
    fn constituted_body_cannot_return_to_unconstituted() {
        let spec = member_composition_specs()[0];
        let parent = formed_state(&spec, spec.min_members);
        let delta = storage_key::composition_keys(&spec)
            .into_iter()
            .map(|key| (key, None))
            .collect();
        assert_eq!(
            check_transition(&delta, |key| parent.get(key).cloned(), |_| None, None),
            Err(GuardError::ConstitutedBodyRemoved(spec.institution.code))
        );
    }

    #[test]
    fn fixed_governance_threshold_snapshot_is_enforced() {
        let fixed = primitives::governance_skeleton::fixed_institutions()[0];
        let subject_cid_numbers: ProposalSubjectCidNumbers = vec![fixed
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("CID fits")]
        .try_into()
        .expect("single subject fits");
        let proposal: Proposal<u32, [u8; 32]> = Proposal {
            kind: PROPOSAL_KIND_INTERNAL,
            stage: STAGE_INTERNAL,
            status: 0,
            internal_code: Some(fixed.code),
            actor_cid_number: Some(
                fixed
                    .cid_number
                    .as_bytes()
                    .to_vec()
                    .try_into()
                    .expect("CID fits"),
            ),
            execution_account_id: None,
            subject_cid_numbers,
            start: 1u32,
            end: 2u32,
        };
        let id = 7;
        let expected = primitives::cid::code::fixed_governance_pass_threshold(&fixed.code)
            .expect("fixed threshold");
        let threshold_key = storage_key::threshold(id);
        let state = BTreeMap::from([
            (storage_key::proposal(id), proposal.encode()),
            (threshold_key.clone(), (expected - 1).encode()),
        ]);
        let delta = BTreeMap::from([(threshold_key, Some((expected - 1).encode()))]);
        assert_eq!(
            check_transition(&delta, |_| None, |key| state.get(key).cloned(), None),
            Err(GuardError::FixedThresholdChanged {
                code: fixed.code,
                expected,
                found: expected - 1,
            })
        );
        assert_eq!(
            check_imported_state(&state),
            Err(GuardError::FixedThresholdChanged {
                code: fixed.code,
                expected,
                found: expected - 1,
            })
        );

        let proposal_only = BTreeMap::from([(storage_key::proposal(id), proposal.encode())]);
        let removed = BTreeMap::from([(storage_key::threshold(id), None)]);
        assert_eq!(
            check_transition(
                &removed,
                |_| None,
                |key| proposal_only.get(key).cloned(),
                None,
            ),
            Err(GuardError::FixedThresholdMissing(fixed.code))
        );
        assert_eq!(check_transition(&removed, |_| None, |_| None, None), Ok(()));
    }

    #[test]
    fn runtime_upgrade_rechecks_existing_fixed_threshold_snapshots() {
        let fixed = primitives::governance_skeleton::fixed_institutions()[0];
        let subject_cid_numbers: ProposalSubjectCidNumbers = vec![fixed
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("CID fits")]
        .try_into()
        .expect("single subject fits");
        let proposal: Proposal<u32, [u8; 32]> = Proposal {
            kind: PROPOSAL_KIND_INTERNAL,
            stage: STAGE_INTERNAL,
            status: 0,
            internal_code: Some(fixed.code),
            actor_cid_number: Some(
                fixed
                    .cid_number
                    .as_bytes()
                    .to_vec()
                    .try_into()
                    .expect("CID fits"),
            ),
            execution_account_id: None,
            subject_cid_numbers,
            start: 1u32,
            end: 2u32,
        };
        let id = 8;
        let expected = primitives::cid::code::fixed_governance_pass_threshold(&fixed.code)
            .expect("fixed threshold");
        let threshold_key = storage_key::threshold(id);
        let state = BTreeMap::from([
            (storage_key::proposal(id), proposal.encode()),
            (threshold_key.clone(), (expected - 1).encode()),
        ]);
        let delta = BTreeMap::from([(sp_storage::well_known_keys::CODE.to_vec(), Some(vec![1]))]);
        assert_eq!(
            check_transition(
                &delta,
                |key| state.get(key).cloned(),
                |key| state.get(key).cloned(),
                Some(std::slice::from_ref(&threshold_key)),
            ),
            Err(GuardError::FixedThresholdChanged {
                code: fixed.code,
                expected,
                found: expected - 1,
            })
        );
        assert_eq!(
            check_transition(
                &delta,
                |key| state.get(key).cloned(),
                |key| state.get(key).cloned(),
                None,
            ),
            Err(GuardError::RuntimeUpgradeVoteKeysMissing)
        );
    }

    #[test]
    fn non_member_singletons_do_not_enter_composition_guard() {
        for code in [
            primitives::cid::code::NLG,
            primitives::cid::code::NSP,
            primitives::cid::code::PRS,
        ] {
            let singleton = primitives::institution_constraints::singleton_institutions()
                .into_iter()
                .find(|item| item.code == code)
                .expect("singleton exists");
            assert!(!storage_key::is_relevant(
                &governance_skeleton::storage_key::account_id(singleton.cid_number.as_bytes(),)
            ));
            assert!(!storage_key::is_relevant(
                &governance_skeleton::storage_key::institution_role(
                    singleton.cid_number.as_bytes(),
                    b"RUNTIME_ROLE",
                )
            ));
        }
    }

    #[test]
    fn six_singleton_threshold_snapshots_are_not_frozen_by_node_guard() {
        for (index, singleton) in primitives::institution_constraints::singleton_institutions()
            .into_iter()
            .enumerate()
        {
            let subject_cid_numbers: ProposalSubjectCidNumbers = vec![singleton
                .cid_number
                .as_bytes()
                .to_vec()
                .try_into()
                .expect("CID fits")]
            .try_into()
            .expect("single subject fits");
            let proposal: Proposal<u32, [u8; 32]> = Proposal {
                kind: PROPOSAL_KIND_INTERNAL,
                stage: STAGE_INTERNAL,
                status: 0,
                internal_code: Some(singleton.code),
                actor_cid_number: Some(
                    singleton
                        .cid_number
                        .as_bytes()
                        .to_vec()
                        .try_into()
                        .expect("CID fits"),
                ),
                execution_account_id: None,
                subject_cid_numbers,
                start: 1u32,
                end: 2u32,
            };
            let id = index as u64 + 20;
            let threshold_key = storage_key::threshold(id);
            let state = BTreeMap::from([
                (storage_key::proposal(id), proposal.encode()),
                (threshold_key.clone(), 1u32.encode()),
            ]);
            assert_eq!(
                check_vote_state_keys([threshold_key], |key| state.get(key).cloned()),
                Ok(()),
                "六个国家级单例不属于固定阈值策略",
            );
        }
    }

    #[test]
    fn fixed_proposal_without_canonical_threshold_is_rejected() {
        let fixed = primitives::governance_skeleton::fixed_institutions()[0];
        let subject_cid_numbers: ProposalSubjectCidNumbers = vec![fixed
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("CID fits")]
        .try_into()
        .expect("single subject fits");
        let id = 40;
        let proposal_key = storage_key::proposal(id);
        let proposal: Proposal<u32, [u8; 32]> = Proposal {
            kind: PROPOSAL_KIND_INTERNAL,
            stage: STAGE_INTERNAL,
            status: 0,
            internal_code: Some(fixed.code),
            actor_cid_number: Some(
                fixed
                    .cid_number
                    .as_bytes()
                    .to_vec()
                    .try_into()
                    .expect("CID fits"),
            ),
            execution_account_id: None,
            subject_cid_numbers,
            start: 1u32,
            end: 2u32,
        };
        let state = BTreeMap::from([(proposal_key.clone(), proposal.encode())]);
        let delta = BTreeMap::from([(proposal_key, state.values().next().cloned())]);
        assert_eq!(
            check_transition(&delta, |_| None, |key| state.get(key).cloned(), None),
            Err(GuardError::FixedThresholdMissing(fixed.code))
        );
    }
}
