#![cfg(test)]

use super::*;
use codec::Decode;
use core::cell::RefCell;
use frame_support::{assert_noop, assert_ok, derive_impl, traits::ConstU32};
use frame_system as system;
use sp_runtime::{
    traits::{Hash, IdentityLookup},
    AccountId32, BuildStorage, DispatchError,
};

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
    pub type VotingEngine = votingengine;

    #[runtime::pallet_index(99)]
    pub type InternalVote = internal_vote;

    #[runtime::pallet_index(2)]
    pub type RuntimeUpgrade = super;

    #[runtime::pallet_index(3)]
    pub type Timestamp = pallet_timestamp;

    #[runtime::pallet_index(4)]
    pub type PowDifficulty = pow_difficulty;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
    type Block = Block;
    type AccountId = AccountId32;
    type Lookup = IdentityLookup<Self::AccountId>;
}

impl pallet_timestamp::Config for Test {
    type Moment = u64;
    type OnTimestampSet = ();
    type MinimumPeriod = frame_support::traits::ConstU64<1>;
    type WeightInfo = ();
}

impl pow_difficulty::Config for Test {
    type WeightInfo = ();
}

pub struct EnsureJointProposerForTest;
impl frame_support::traits::EnsureOrigin<RuntimeOrigin> for EnsureJointProposerForTest {
    type Success = AccountId32;

    fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
        let who = frame_system::EnsureSigned::<AccountId32>::try_origin(o)?;
        if who == nrc_admin() || who == prc_admin() || who == ordinary_staff_admin() {
            Ok(who)
        } else {
            Err(RuntimeOrigin::from(frame_system::RawOrigin::Signed(who)))
        }
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
        Err(())
    }
}

pub struct EnsureNrcAdminForTest;
impl frame_support::traits::EnsureOrigin<RuntimeOrigin> for EnsureNrcAdminForTest {
    type Success = AccountId32;

    fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
        let who = frame_system::EnsureSigned::<AccountId32>::try_origin(o)?;
        if who == nrc_admin() {
            Ok(who)
        } else {
            Err(RuntimeOrigin::from(frame_system::RawOrigin::Signed(who)))
        }
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
        Err(())
    }
}

thread_local! {
    static NEXT_JOINT_ID: RefCell<u64> = const { RefCell::new(100) };
}
thread_local! {
    static EXEC_SHOULD_FAIL: RefCell<bool> = const { RefCell::new(false) };
}

pub struct TestJointVoteEngine;
impl TestJointVoteEngine {
    fn allocate_id() -> u64 {
        NEXT_JOINT_ID.with(|id| {
            let mut id = id.borrow_mut();
            let proposal_id = *id;
            *id = id.saturating_add(1);
            proposal_id
        })
    }

    fn store_data(
        proposal_id: u64,
        vote_plan: votingengine::types::VotePlanOf<AccountId32>,
        data: Vec<u8>,
    ) -> Result<(), DispatchError> {
        let bounded_data: frame_support::BoundedVec<
            u8,
            <Test as votingengine::Config>::MaxProposalDataLen,
        > = data
            .try_into()
            .map_err(|_| DispatchError::Other("proposal data too large"))?;
        let owner: frame_support::BoundedVec<u8, <Test as votingengine::Config>::MaxModuleTagLen> =
            vote_plan
                .proposal_owner
                .to_vec()
                .try_into()
                .map_err(|_| DispatchError::Other("module tag too large"))?;
        votingengine::ProposalData::<Test>::insert(proposal_id, bounded_data);
        votingengine::ProposalOwner::<Test>::insert(proposal_id, owner);
        votingengine::ProposalVotePlans::<Test>::insert(proposal_id, vote_plan);
        Ok(())
    }
}

impl votingengine::JointVoteEngine<AccountId32> for TestJointVoteEngine {
    // 测试 mock 模拟投票引擎“已准备好投票上下文”的创建入口，
    // runtime-upgrade 测试不再传入人口快照或联合签名材料。
    fn create_joint_proposal_with_data(
        _who: AccountId32,
        _actor_cid_number: Vec<u8>,
        vote_plan: votingengine::types::VotePlanOf<AccountId32>,
        data: Vec<u8>,
    ) -> Result<u64, DispatchError> {
        let hash = <Test as frame_system::Config>::Hashing::hash(data.as_slice());
        if hash.as_ref() != vote_plan.business_object_hash.as_slice() {
            return Err(DispatchError::Other("business object hash mismatch"));
        }
        let proposal_id = Self::allocate_id();
        Self::store_data(proposal_id, vote_plan, data)?;
        Ok(proposal_id)
    }

    fn create_joint_proposal_with_data_and_object(
        who: AccountId32,
        actor_cid_number: Vec<u8>,
        vote_plan: votingengine::types::VotePlanOf<AccountId32>,
        data: Vec<u8>,
        object_kind: u8,
        object_data: Vec<u8>,
    ) -> Result<u64, DispatchError> {
        let object_hash = <Test as frame_system::Config>::Hashing::hash(&object_data);
        if object_hash.as_ref() != vote_plan.business_object_hash.as_slice() {
            return Err(DispatchError::Other("business object hash mismatch"));
        }
        let _ = (who, actor_cid_number);
        let proposal_id = Self::allocate_id();
        Self::store_data(proposal_id, vote_plan, data)?;
        let object_len = u32::try_from(object_data.len())
            .map_err(|_| DispatchError::Other("proposal object too large"))?;
        let object_hash = <Test as frame_system::Config>::Hashing::hash(&object_data);
        let bounded_object: frame_support::BoundedVec<
            u8,
            <Test as votingengine::Config>::MaxProposalObjectLen,
        > = object_data
            .try_into()
            .map_err(|_| DispatchError::Other("proposal object too large"))?;
        votingengine::ProposalObject::<Test>::insert(proposal_id, bounded_object);
        votingengine::ProposalObjectMeta::<Test>::insert(
            proposal_id,
            votingengine::ProposalObjectMetadata {
                kind: object_kind,
                object_len,
                object_hash,
            },
        );
        Ok(proposal_id)
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
    type MaxCleanupActivationsPerBlock = ConstU32<50>;
    type CleanupKeysPerStep = ConstU32<64>;
    type MaxProposalDataLen = ConstU32<{ 100 * 1024 }>;
    type MaxProposalObjectLen = ConstU32<{ 10 * 1024 }>;
    type MaxModuleTagLen = ConstU32<32>;
    type MaxManualExecutionAttempts = ConstU32<3>;
    type ExecutionRetryGraceBlocks = frame_support::traits::ConstU64<216>;
    type MaxExecutionRetryDeadlinesPerBlock = ConstU32<128>;
    type MaxPendingRetryExpirationsPerBlock = ConstU32<16>;
    type CitizenIdentityReader = ();
    type JointVoteResultCallback = ();
    type InternalVoteResultCallback = ();
    type InternalAdminProvider = TestInternalAdminProvider;
    type MaxAdminsPerInstitution = ConstU32<32>;
    type TimeProvider = TestTimeProvider;
    type WeightInfo = ();
    type TrackHandlers = (InternalVote, ());
    type LegislationVoteResultCallback = ();
    type ElectionVoteResultCallback = ();
}

pub struct TestInternalAdminProvider;
impl votingengine::InternalAdminProvider<AccountId32> for TestInternalAdminProvider {
    fn is_institution_admin(
        institution_code: votingengine::InstitutionCode,
        cid_number: &[u8],
        who: &AccountId32,
    ) -> bool {
        (institution_code == votingengine::types::NRC
            && cid_number == nrc_cid().as_slice()
            && *who == nrc_admin())
            || (institution_code == votingengine::types::PRC
                && cid_number == prc_cid().as_slice()
                && *who == prc_admin())
    }
}

pub struct TestInstitutionRoleAuthorization;
impl entity_primitives::InstitutionRoleAuthorizationQuery<AccountId32>
    for TestInstitutionRoleAuthorization
{
    fn role_has_permission(
        role_subject: &entity_primitives::RoleSubject<Vec<u8>, Vec<u8>>,
        business_action_id: &entity_primitives::BusinessActionId<Vec<u8>>,
        operation: entity_primitives::RolePermissionOperation,
    ) -> bool {
        role_subject.role_code == primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
            && business_action_id.module_tag == crate::MODULE_TAG
            && business_action_id.action_code
                == entity_primitives::business_action::ACTION_RUNTIME_UPGRADE
            && operation == entity_primitives::RolePermissionOperation::Propose
    }

    fn is_authorized(
        admin: &AccountId32,
        role_subject: &entity_primitives::RoleSubject<Vec<u8>, Vec<u8>>,
        business_action_id: &entity_primitives::BusinessActionId<Vec<u8>>,
        operation: entity_primitives::RolePermissionOperation,
    ) -> bool {
        let valid_actor = (role_subject.cid_number == nrc_cid().to_vec() && admin == &nrc_admin())
            || (role_subject.cid_number == prc_cid().to_vec() && admin == &prc_admin());
        valid_actor && Self::role_has_permission(role_subject, business_action_id, operation)
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

impl votingengine::InstitutionRoleProvider<AccountId32> for TestInstitutionRoleAuthorization {
    fn is_active_assignment(cid_number: &[u8], who: &AccountId32, role_code: &[u8]) -> bool {
        role_code == primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
            && [votingengine::types::NRC, votingengine::types::PRC]
                .into_iter()
                .any(|code| {
                    <TestInternalAdminProvider as votingengine::InternalAdminProvider<
                        AccountId32,
                    >>::is_institution_admin(code, cid_number, who)
                })
    }

    fn active_accounts_for_role(cid_number: &[u8], role_code: &[u8]) -> Vec<AccountId32> {
        [nrc_admin(), prc_admin()]
            .into_iter()
            .filter(|admin| Self::is_active_assignment(cid_number, admin, role_code))
            .collect()
    }
}

impl internal_vote::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type InstitutionRoleProvider = TestInstitutionRoleAuthorization;
    type WeightInfo = ();
}

// 测试用开发者直升开关：默认开启，可通过 thread_local 控制。
thread_local! {
    static DEV_UPGRADE_ENABLED: RefCell<bool> = const { RefCell::new(true) };
}
pub struct TestDeveloperUpgradeCheck;
impl genesis_pallet::DeveloperUpgradeCheck for TestDeveloperUpgradeCheck {
    fn is_enabled() -> bool {
        DEV_UPGRADE_ENABLED.with(|v| *v.borrow())
    }
}

impl pallet::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type ProposeOrigin = EnsureJointProposerForTest;
    type DeveloperUpgradeOrigin = EnsureNrcAdminForTest;
    type JointVoteEngine = TestJointVoteEngine;
    type InstitutionRoleAuthorization = TestInstitutionRoleAuthorization;
    type RuntimeCodeExecutor = TestRuntimeCodeExecutor;
    type DeveloperUpgradeCheck = TestDeveloperUpgradeCheck;
    type MaxReasonLen = ConstU32<64>;
    type MaxRuntimeCodeSize = ConstU32<1024>;
    type WeightInfo = ();
}

thread_local! {
    static RUNTIME_CODE_EXECUTED: RefCell<bool> = const { RefCell::new(false) };
}

pub struct TestRuntimeCodeExecutor;
impl RuntimeCodeExecutor for TestRuntimeCodeExecutor {
    fn execute_runtime_code(
        _code: &[u8],
        params: pow_difficulty::PowDifficultyParams,
        activate_at: u32,
    ) -> DispatchResult {
        let should_fail = EXEC_SHOULD_FAIL.with(|v| *v.borrow());
        if should_fail {
            return Err(DispatchError::Other("set_code failed"));
        }
        pow_difficulty::Pallet::<Test>::stage_params(params, activate_at)?;
        RUNTIME_CODE_EXECUTED.with(|v| *v.borrow_mut() = true);
        Ok(())
    }
}

fn new_test_ext() -> sp_io::TestExternalities {
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("test storage should build");
    let mut ext: sp_io::TestExternalities = storage.into();
    ext.execute_with(|| {
        RUNTIME_CODE_EXECUTED.with(|v| *v.borrow_mut() = false);
        EXEC_SHOULD_FAIL.with(|v| *v.borrow_mut() = false);
        NEXT_JOINT_ID.with(|id| *id.borrow_mut() = 100);
        DEV_UPGRADE_ENABLED.with(|v| *v.borrow_mut() = true);
    });
    ext
}

fn nrc_admin() -> AccountId32 {
    AccountId32::new([1u8; 32])
}

fn outsider() -> AccountId32 {
    AccountId32::new([2u8; 32])
}

fn ordinary_staff_admin() -> AccountId32 {
    AccountId32::new([4u8; 32])
}

fn prc_admin() -> AccountId32 {
    AccountId32::new([3u8; 32])
}

fn nrc_cid() -> votingengine::types::CidNumber {
    primitives::cid::china::china_cb::CHINA_CB[0]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("NRC CID fits runtime bound")
}

fn prc_cid() -> votingengine::types::CidNumber {
    primitives::cid::china::china_cb::CHINA_CB[1]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("PRC CID fits runtime bound")
}

fn committee_role() -> votingengine::types::RoleCode {
    primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
        .to_vec()
        .try_into()
        .expect("committee role fits runtime bound")
}

fn reason_ok() -> pallet::ReasonOf<Test> {
    b"upgrade reason"
        .to_vec()
        .try_into()
        .expect("reason should fit")
}

fn code_ok() -> pallet::CodeOf<Test> {
    vec![1, 2, 3, 4, 5]
        .try_into()
        .expect("runtime code should fit")
}

/// 从 ProposalData 读取并跳过 MODULE_TAG 后 decode 提案摘要。
fn decode_proposal(proposal_id: u64) -> pallet::Proposal<Test> {
    let raw = votingengine::Pallet::<Test>::get_proposal_data(proposal_id)
        .expect("proposal data should exist");
    let tag = crate::MODULE_TAG;
    assert!(
        raw.len() >= tag.len() && &raw[..tag.len()] == tag,
        "MODULE_TAG mismatch"
    );
    pallet::Proposal::<Test>::decode(&mut &raw[tag.len()..]).expect("should decode proposal")
}

fn propose_ok() {
    assert_ok!(RuntimeUpgrade::propose_runtime_upgrade(
        RuntimeOrigin::signed(nrc_admin()),
        nrc_cid(),
        committee_role(),
        reason_ok(),
        code_ok(),
        pow_difficulty::PowDifficultyParams::genesis_default()
    ));
}

/// 在投票引擎中插入一个 PASSED 状态的 Proposal，使回调执行结果写入可用。
/// 测试 mock 的 TestJointVoteEngine 不创建真实 Proposals 条目，
/// 需手工补一个以模拟真实回调上下文。
fn insert_engine_proposal(proposal_id: u64) {
    insert_engine_proposal_with_status(proposal_id, votingengine::STATUS_PASSED);
}

fn insert_engine_proposal_with_status(proposal_id: u64, status: u8) {
    insert_engine_proposal_with_stage_and_status(proposal_id, votingengine::STAGE_JOINT, status);
}

fn insert_engine_proposal_with_stage_and_status(proposal_id: u64, stage: u8, status: u8) {
    votingengine::pallet::Proposals::<Test>::insert(
        proposal_id,
        votingengine::Proposal {
            kind: votingengine::PROPOSAL_KIND_JOINT,
            stage,
            status,
            internal_code: None,
            actor_cid_number: Some(
                primitives::cid::china::china_cb::CHINA_CB[0]
                    .cid_number
                    .as_bytes()
                    .to_vec()
                    .try_into()
                    .expect("NRC CID fits runtime bound"),
            ),
            execution_account_id: None,
            subject_cid_numbers: Default::default(),
            start: 0u64,
            end: 100u64,
            // 这是 votingengine::Proposal 的固定字段，非 runtime-upgrade 入参。
        },
    );
}

fn call_joint_callback(
    proposal_id: u64,
    approved: bool,
) -> Result<votingengine::ProposalExecutionOutcome, DispatchError> {
    votingengine::pallet::CallbackExecutionScopes::<Test>::insert(proposal_id, ());
    let result = RuntimeUpgrade::on_joint_vote_finalized(proposal_id, approved);
    votingengine::pallet::CallbackExecutionScopes::<Test>::remove(proposal_id);
    match result {
        Ok(outcome) => {
            if approved {
                votingengine::pallet::Proposals::<Test>::mutate(proposal_id, |maybe| {
                    if let Some(proposal) = maybe {
                        proposal.status = match outcome {
                            votingengine::ProposalExecutionOutcome::Executed => {
                                votingengine::STATUS_EXECUTED
                            }
                            votingengine::ProposalExecutionOutcome::FatalFailed => {
                                votingengine::STATUS_EXECUTION_FAILED
                            }
                            _ => proposal.status,
                        };
                    }
                });
            }
            Ok(outcome)
        }
        Err(err) => Err(err),
    }
}

mod cases;
