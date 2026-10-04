#![cfg(test)]

use super::*;
use frame_support::{
    assert_noop, assert_ok, derive_impl, parameter_types,
    traits::{ConstU32, Hooks},
};
use frame_system as system;
use primitives::cid::china::china_cb::CHINA_CB;
use sp_consensus_grandpa::AuthorityId as GrandpaAuthorityId;
use sp_core::{ed25519, Pair, Void};
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};
use votingengine::types::{AuthorizationSubject, VotingEngineKind};
use votingengine::InstitutionRoleProvider as _;

type Block = frame_system::mocking::MockBlock<Test>;

#[frame_support::runtime]
mod runtime {
    #[runtime::runtime]
    #[runtime::derive(
        RuntimeCall,
        RuntimeEvent,
        RuntimeError,
        RuntimeOrigin,
        RuntimeFreezeReason,
        RuntimeHoldReason,
        RuntimeSlashReason,
        RuntimeLockId,
        RuntimeTask,
        RuntimeViewFunction
    )]
    pub struct Test;

    #[runtime::pallet_index(0)]
    pub type System = frame_system;

    #[runtime::pallet_index(1)]
    pub type Grandpa = pallet_grandpa;

    #[runtime::pallet_index(2)]
    pub type VotingEngine = votingengine;

    #[runtime::pallet_index(99)]
    pub type InternalVote = internal_vote;

    #[runtime::pallet_index(3)]
    pub type GrandpaKeyChange = super;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
    type Block = Block;
    type AccountId = AccountId32;
    type Lookup = IdentityLookup<Self::AccountId>;
}

parameter_types! {
    pub const MaxGrandpaAuthorities: u32 = 64;
    pub const MaxGrandpaNominators: u32 = 0;
    pub const MaxSetIdSessionEntries: u64 = 16;
    pub const GrandpaChangeDelay: u64 = 30;
}

impl pallet_grandpa::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
    type MaxAuthorities = MaxGrandpaAuthorities;
    type MaxNominators = MaxGrandpaNominators;
    type MaxSetIdSessionEntries = MaxSetIdSessionEntries;
    type KeyOwnerProof = Void;
    type EquivocationReportSystem = ();
}

pub struct TestCitizenIdentityReader;
impl votingengine::CitizenIdentityReader<AccountId32> for TestCitizenIdentityReader {
    fn voting_subject(
        _who: &AccountId32,
        _scope: &votingengine::PopulationScope,
    ) -> Option<votingengine::CitizenSubject<AccountId32>> {
        None
    }

    fn candidate_subject(
        _who: &AccountId32,
        _scope: &votingengine::PopulationScope,
    ) -> Option<votingengine::CitizenSubject<AccountId32>> {
        None
    }
}

pub struct TestInternalAdminProvider;

impl TestInternalAdminProvider {
    fn institution_admins(
        institution_code: InstitutionCode,
        cid_number: &[u8],
    ) -> Option<sp_std::vec::Vec<AccountId32>> {
        match institution_code {
            NRC | PRC => CHINA_CB
                .iter()
                .find(|node| node.cid_number.as_bytes() == cid_number)
                .map(|node| {
                    node.admins
                        .iter()
                        .map(|raw| AccountId32::new(*raw))
                        .collect()
                }),
            _ => None,
        }
    }
}

impl votingengine::InternalAdminProvider<AccountId32> for TestInternalAdminProvider {
    fn is_institution_admin(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        who: &AccountId32,
    ) -> bool {
        let mut who_raw = [0u8; 32];
        who_raw.copy_from_slice(who.as_ref());
        match institution_code {
            NRC | PRC => CHINA_CB
                .iter()
                .find(|node| node.cid_number.as_bytes() == cid_number)
                .map(|node| node.admins.contains(&who_raw))
                .unwrap_or(false),
            _ => false,
        }
    }

    fn institution_threshold(institution_code: InstitutionCode, cid_number: &[u8]) -> Option<u32> {
        Self::institution_admins(institution_code, cid_number)?;
        primitives::cid::code::fixed_governance_pass_threshold(&institution_code)
    }
}

pub struct TestInstitutionRoleProvider;

fn committee_role() -> votingengine::types::RoleCode {
    primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
        .to_vec()
        .try_into()
        .expect("committee role fits protocol bound")
}

impl votingengine::InstitutionRoleProvider<AccountId32> for TestInstitutionRoleProvider {
    fn is_active_assignment(cid_number: &[u8], who: &AccountId32, role_code: &[u8]) -> bool {
        Self::active_accounts_for_role(cid_number, role_code).contains(who)
    }

    fn active_accounts_for_role(cid_number: &[u8], role_code: &[u8]) -> Vec<AccountId32> {
        if role_code != primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER {
            return Vec::new();
        }
        let Some(code) = core::str::from_utf8(cid_number)
            .ok()
            .and_then(votingengine::types::institution_code_from_cid_number)
        else {
            return Vec::new();
        };
        TestInternalAdminProvider::institution_admins(code, cid_number).unwrap_or_default()
    }
}

impl entity_primitives::InstitutionRoleAuthorizationQuery<AccountId32>
    for TestInstitutionRoleProvider
{
    fn role_has_permission(
        role_subject: &entity_primitives::RoleSubject<Vec<u8>, Vec<u8>>,
        business_action_id: &entity_primitives::BusinessActionId<Vec<u8>>,
        operation: entity_primitives::RolePermissionOperation,
    ) -> bool {
        role_subject.role_code.as_slice()
            == primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
            && business_action_id.module_tag.as_slice() == crate::MODULE_TAG
            && match business_action_id.action_code {
                entity_primitives::business_action::ACTION_GRANDPA_KEY_EMERGENCY_RECOVERY => true,
                entity_primitives::business_action::ACTION_GRANDPA_KEY_ROTATION => {
                    operation == entity_primitives::RolePermissionOperation::Propose
                }
                _ => false,
            }
    }

    fn is_authorized(
        admin: &AccountId32,
        role_subject: &entity_primitives::RoleSubject<Vec<u8>, Vec<u8>>,
        business_action_id: &entity_primitives::BusinessActionId<Vec<u8>>,
        operation: entity_primitives::RolePermissionOperation,
    ) -> bool {
        Self::role_has_permission(role_subject, business_action_id, operation)
            && Self::is_active_assignment(
                role_subject.cid_number.as_slice(),
                admin,
                role_subject.role_code.as_slice(),
            )
    }

    fn role_subjects_with_permission(
        cid_number: &[u8],
        business_action_id: &entity_primitives::BusinessActionId<Vec<u8>>,
        operation: entity_primitives::RolePermissionOperation,
    ) -> Vec<entity_primitives::RoleSubject<Vec<u8>, Vec<u8>>> {
        let role_subject = entity_primitives::RoleSubject {
            cid_number: cid_number.to_vec(),
            role_code: primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER.to_vec(),
        };
        Self::role_has_permission(&role_subject, business_action_id, operation)
            .then_some(role_subject)
            .into_iter()
            .collect()
    }
}

pub struct TestTimeProvider;

impl frame_support::traits::UnixTime for TestTimeProvider {
    fn now() -> core::time::Duration {
        core::time::Duration::from_secs(1_782_864_000) // 2026-07-01
    }
}

impl votingengine::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxVoteNonceLength = ConstU32<64>;
    type MaxVoteSignatureLength = ConstU32<64>;
    type MaxAutoFinalizePerBlock = ConstU32<64>;
    type MaxAutoFinalizeWeightPerBlock = votingengine::BlockWeightFraction<Test, 4>;
    type MaxExecutionWeightPerBlock = votingengine::BlockWeightFraction<Test, 4>;
    type MaxCleanupWeightPerBlock = votingengine::BlockWeightFraction<Test, 8>;
    type MaxProposalsPerExpiry = ConstU32<128>;
    type MaxInternalProposalMutexBindings = ConstU32<256>;
    type MaxActiveProposals = ConstU32<10>;
    type MaxCleanupStepsPerBlock = ConstU32<8>;
    type CleanupKeysPerStep = ConstU32<64>;
    type MaxProposalDataLen = ConstU32<256>;
    type MaxProposalObjectLen = ConstU32<{ 10 * 1024 }>;
    type MaxModuleTagLen = ConstU32<32>;
    type MaxManualExecutionAttempts = ConstU32<3>;
    type ExecutionRetryGraceBlocks = frame_support::traits::ConstU64<216>;
    type MaxExecutionRetryDeadlinesPerBlock = ConstU32<128>;
    type MaxCleanupActivationsPerBlock = ConstU32<50>;
    type MaxPendingRetryExpirationsPerBlock = ConstU32<16>;
    type CitizenIdentityReader = TestCitizenIdentityReader;
    type JointVoteResultCallback = ();
    type InternalVoteResultCallback = crate::InternalVoteExecutor<Test>;
    type InternalAdminProvider = TestInternalAdminProvider;
    type MaxAdminsPerInstitution = ConstU32<32>;
    type TimeProvider = TestTimeProvider;
    type WeightInfo = ();
    type TrackHandlers = (InternalVote, ());
    type LegislationVoteResultCallback = ();
    type ElectionVoteResultCallback = ();
}

impl internal_vote::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type InstitutionRoleProvider = TestInstitutionRoleProvider;
    type WeightInfo = ();
}

impl Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type GrandpaChangeDelay = GrandpaChangeDelay;
    type InternalVoteEngine = internal_vote::Pallet<Test>;
    type InstitutionRoleAuthorization = TestInstitutionRoleProvider;
    type WeightInfo = ();
}

fn grandpa_pair(node_index: usize) -> ed25519::Pair {
    let mut seed = [0u8; 32];
    seed[0] = (node_index + 1) as u8;
    ed25519::Pair::from_seed(&seed)
}

fn grandpa_public_key(node_index: usize) -> [u8; 32] {
    grandpa_pair(node_index).public().0
}

fn grandpa_authorities() -> sp_consensus_grandpa::AuthorityList {
    (0..3)
        .map(|index| (authority_id_from_key(grandpa_public_key(index)), 1))
        .collect()
}

fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("test storage should build");
    pallet_grandpa::GenesisConfig::<Test> {
        authorities: grandpa_authorities(),
        _config: Default::default(),
    }
    .assimilate_storage(&mut storage)
    .expect("grandpa genesis should assimilate");
    GenesisConfig::<Test>::default()
        .assimilate_storage(&mut storage)
        .expect("grandpakey-change genesis should assimilate");

    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);
        // 测试使用可签名的确定性 authority 私钥；创世目录只提供公开生产 key。
        for (node_index, node) in CHINA_CB.iter().enumerate().take(3) {
            let actor_cid_number = cb_cid(node_index);
            GrandpaKeyOwnerByKey::<Test>::remove(node.grandpa_key);
            CurrentGrandpaKeys::<Test>::insert(
                actor_cid_number.clone(),
                grandpa_public_key(node_index),
            );
            GrandpaKeyOwnerByKey::<Test>::insert(grandpa_public_key(node_index), actor_cid_number);
        }
    });
    ext
}

fn cb_admin(node_index: usize, admin_index: usize) -> AccountId32 {
    AccountId32::new(CHINA_CB[node_index].admins[admin_index])
}

fn cb_cid(node_index: usize) -> CidNumber {
    CHINA_CB[node_index]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("CHINA_CB CID fits")
}

fn prc_admin(index: usize) -> AccountId32 {
    cb_admin(1, index)
}

fn prc_cid() -> CidNumber {
    cb_cid(1)
}

fn key_pair(seed: u8) -> ed25519::Pair {
    let mut seed_bytes = [0u8; 32];
    seed_bytes[0] = seed;
    ed25519::Pair::from_seed(&seed_bytes)
}

fn identity_public_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    key[0] = 1;
    key
}

fn authority_id_from_key(key: [u8; 32]) -> GrandpaAuthorityId {
    GrandpaAuthorityId::from(ed25519::Public::from_raw(key))
}

fn pass_prc_proposal(node_index: usize, proposal_id: u64) {
    // 提案发起人已经由投票引擎自动记一票，这里只补足剩余固定阈值票。
    for admin_index in 1..6 {
        assert_ok!(cast_vote(
            cb_admin(node_index, admin_index),
            proposal_id,
            true
        ));
    }
    // 通过判定只负责入执行队列；业务回调统一由维护管线按权重预算异步执行。
    <VotingEngine as Hooks<u64>>::on_initialize(System::block_number());
}

fn finalize_grandpa_at(block: u64) {
    System::set_block_number(block);
    <Grandpa as Hooks<u64>>::on_finalize(block);
    System::set_block_number(block + 1);
    <GrandpaKeyChange as Hooks<u64>>::on_initialize(block + 1);
}

/// 获取最近一次 create_internal_proposal 分配的 proposal_id。
fn last_proposal_id() -> u64 {
    votingengine::Pallet::<Test>::next_proposal_id().saturating_sub(1)
}

/// 测试辅助:走投票引擎公开 `internal_vote` extrinsic 投票(统一入口)。
fn cast_vote(who: AccountId32, proposal_id: u64, approve: bool) -> DispatchResult {
    frame_support::storage::with_transaction(
        || -> frame_support::storage::TransactionOutcome<DispatchResult> {
            match internal_vote::Pallet::<Test>::do_internal_vote(
                who,
                proposal_id,
                internal_vote::InternalVoteTicketClaim::InstitutionRole(committee_role()),
                approve,
            ) {
                Ok(()) => frame_support::storage::TransactionOutcome::Commit(Ok(())),
                Err(e) => frame_support::storage::TransactionOutcome::Rollback(Err(e)),
            }
        },
    )
}

mod cases;
