#![cfg(test)]

use super::*;
use core::cell::RefCell;
use frame_support::{
    assert_noop, assert_ok, derive_impl,
    traits::{ConstU128, ConstU32},
    BoundedVec,
};
use frame_system as system;
use sp_runtime::{
    traits::{Hash, IdentityLookup},
    AccountId32, BuildStorage, DispatchError,
};

type Balance = u128;
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
    pub type Balances = pallet_balances;

    #[runtime::pallet_index(2)]
    pub type VotingEngine = votingengine;

    #[runtime::pallet_index(99)]
    pub type InternalVote = internal_vote;

    #[runtime::pallet_index(8)]
    pub type ResolutionIssuance = super;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
    type Block = Block;
    type AccountId = AccountId32;
    type Lookup = IdentityLookup<Self::AccountId>;
    type AccountData = pallet_balances::AccountData<Balance>;
}

impl pallet_balances::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Balance = Balance;
    type DustRemoval = ();
    type ExistentialDeposit = ConstU128<10>;
    type AccountStore = System;
    type MaxLocks = ConstU32<0>;
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type FreezeIdentifier = RuntimeFreezeReason;
    type MaxFreezes = ConstU32<0>;
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type DoneSlashHandler = ();
    type WeightInfo = ();
}

pub struct EnsureNrcAdminForTest;
impl frame_support::traits::EnsureOrigin<RuntimeOrigin> for EnsureNrcAdminForTest {
    type Success = AccountId32;

    fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
        let who = frame_system::EnsureSigned::<AccountId32>::try_origin(o)?;
        if who == AccountId32::new([1u8; 32]) || who == AccountId32::new([4u8; 32]) {
            Ok(who)
        } else {
            Err(RuntimeOrigin::from(frame_system::RawOrigin::Signed(who)))
        }
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
        Ok(RuntimeOrigin::signed(AccountId32::new([1u8; 32])))
    }
}

thread_local! {
    static NEXT_JOINT_ID: RefCell<u64> = const { RefCell::new(100) };
}

pub struct TestJointVoteEngine;
impl TestJointVoteEngine {
    fn allocate_id() -> u64 {
        NEXT_JOINT_ID.with(|id| {
            let mut id = id.borrow_mut();
            let value = *id;
            *id = id.saturating_add(1);
            value
        })
    }
}

impl votingengine::JointVoteEngine<AccountId32> for TestJointVoteEngine {
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
        let proposal_id =
            Self::create_joint_proposal_with_data(who, actor_cid_number, vote_plan, data)?;
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

pub struct TestInternalAdminProvider;
impl votingengine::InternalAdminProvider<AccountId32> for TestInternalAdminProvider {
    fn is_institution_admin(
        institution_code: votingengine::types::InstitutionCode,
        cid_number: &[u8],
        who: &AccountId32,
    ) -> bool {
        let expected_cid = match institution_code {
            votingengine::types::NRC => primitives::cid::china::china_cb::CHINA_CB[0].cid_number,
            votingengine::types::PRC => primitives::cid::china::china_cb::CHINA_CB[1].cid_number,
            _ => return false,
        };
        cid_number == expected_cid.as_bytes() && who == &AccountId32::new([1u8; 32])
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
                == entity_primitives::business_action::ACTION_RESOLUTION_ISSUANCE
            && operation == entity_primitives::RolePermissionOperation::Propose
    }

    fn is_authorized(
        admin: &AccountId32,
        role_subject: &entity_primitives::RoleSubject<Vec<u8>, Vec<u8>>,
        business_action_id: &entity_primitives::BusinessActionId<Vec<u8>>,
        operation: entity_primitives::RolePermissionOperation,
    ) -> bool {
        let valid_cid = [
            primitives::cid::china::china_cb::CHINA_CB[0]
                .cid_number
                .as_bytes(),
            primitives::cid::china::china_cb::CHINA_CB[1]
                .cid_number
                .as_bytes(),
        ]
        .contains(&role_subject.cid_number.as_slice());
        admin == &AccountId32::new([1u8; 32])
            && valid_cid
            && Self::role_has_permission(role_subject, business_action_id, operation)
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
        let admin = AccountId32::new([1u8; 32]);
        Self::is_active_assignment(cid_number, &admin, role_code)
            .then_some(admin)
            .into_iter()
            .collect()
    }
}

pub struct TestCitizenIdentityReader;
impl votingengine::CitizenIdentityReader<AccountId32> for TestCitizenIdentityReader {
    fn voting_subject(
        who: &AccountId32,
        _scope: &votingengine::PopulationScope,
    ) -> Option<votingengine::CitizenSubject<AccountId32>> {
        Some(test_citizen_subject(who))
    }

    fn candidate_subject(
        who: &AccountId32,
        _scope: &votingengine::PopulationScope,
    ) -> Option<votingengine::CitizenSubject<AccountId32>> {
        Some(test_citizen_subject(who))
    }
}

fn test_citizen_subject(who: &AccountId32) -> votingengine::CitizenSubject<AccountId32> {
    votingengine::CitizenSubject {
        cid_number: <AccountId32 as AsRef<[u8]>>::as_ref(who)
            .to_vec()
            .try_into()
            .expect("account fits CID"),
        account_id: who.clone(),
    }
}

pub struct TestTimeProvider;
impl frame_support::traits::UnixTime for TestTimeProvider {
    fn now() -> core::time::Duration {
        core::time::Duration::from_secs(1_782_864_000)
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
    type MaxProposalDataLen = ConstU32<8192>;
    type MaxProposalObjectLen = ConstU32<{ 10 * 1024 }>;
    type MaxModuleTagLen = ConstU32<32>;
    type MaxManualExecutionAttempts = ConstU32<3>;
    type ExecutionRetryGraceBlocks = frame_support::traits::ConstU64<216>;
    type MaxExecutionRetryDeadlinesPerBlock = ConstU32<128>;
    type MaxCleanupActivationsPerBlock = ConstU32<50>;
    type MaxPendingRetryExpirationsPerBlock = ConstU32<16>;
    type CitizenIdentityReader = TestCitizenIdentityReader;
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

impl internal_vote::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type InstitutionRoleProvider = TestInstitutionRoleAuthorization;
    type WeightInfo = ();
}

impl pallet::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type ProposeOrigin = EnsureNrcAdminForTest;
    type JointVoteEngine = TestJointVoteEngine;
    type InstitutionRoleAuthorization = TestInstitutionRoleAuthorization;
    type MaxReasonLen = ConstU32<128>;
    type MaxAllocations = ConstU32<64>;
    type MaxTotalIssuance = ConstU128<14_434_973_780_000>;
    type MaxSingleIssuance = ConstU128<14_434_973_780_000>;
    type WeightInfo = ();
}

fn new_test_ext() -> sp_io::TestExternalities {
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("test storage should build");
    let mut ext: sp_io::TestExternalities = storage.into();
    ext.execute_with(|| {
        System::set_block_number(1);
        NEXT_JOINT_ID.with(|id| *id.borrow_mut() = 100);
        let recipients = reserve_council_accounts();
        let bounded: BoundedVec<AccountId32, ConstU32<64>> =
            recipients.try_into().expect("recipients should fit");
        pallet::AllowedRecipients::<Test>::put(bounded);
    });
    ext
}

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
            actor_cid_number: Some(actor_cid_number()),
            execution_account_id: None,
            subject_cid_numbers: Default::default(),
            start: 0u64,
            end: 100u64,
        },
    );
}

fn overwrite_proposal_data(
    proposal_id: u64,
    data: crate::proposal::IssuanceProposalData<AccountId32, Balance>,
) {
    let mut encoded = Vec::from(crate::MODULE_TAG);
    encoded.extend_from_slice(&codec::Encode::encode(&data));
    let bounded_data: BoundedVec<u8, <Test as votingengine::Config>::MaxProposalDataLen> =
        encoded.try_into().expect("proposal data should fit");
    let owner: BoundedVec<u8, <Test as votingengine::Config>::MaxModuleTagLen> = crate::MODULE_TAG
        .to_vec()
        .try_into()
        .expect("module tag should fit");
    votingengine::ProposalData::<Test>::insert(proposal_id, bounded_data);
    votingengine::ProposalOwner::<Test>::insert(proposal_id, owner);
}

fn call_joint_callback(
    proposal_id: u64,
    approved: bool,
) -> Result<votingengine::ProposalExecutionOutcome, DispatchError> {
    votingengine::pallet::CallbackExecutionScopes::<Test>::insert(proposal_id, ());
    let result = ResolutionIssuance::on_joint_vote_finalized(proposal_id, approved);
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

fn reason_ok() -> pallet::ReasonOf<Test> {
    b"issuance".to_vec().try_into().expect("reason should fit")
}

fn actor_cid_number() -> votingengine::types::CidNumber {
    primitives::cid::china::china_cb::CHINA_CB[0]
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("NRC CID should fit")
}

fn committee_role_code() -> votingengine::types::RoleCode {
    primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
        .to_vec()
        .try_into()
        .expect("committee role code should fit")
}

fn director_role_code() -> votingengine::types::RoleCode {
    primitives::governance_skeleton::ROLE_CODE_DIRECTOR
        .to_vec()
        .try_into()
        .expect("director role code should fit")
}

fn reserve_council_accounts() -> Vec<AccountId32> {
    primitives::cid::china::china_cb::CHINA_CB
        .iter()
        .skip(1)
        .map(|n| AccountId32::new(n.main_account))
        .collect()
}

fn allocations_ok(total: Balance) -> pallet::AllocationOf<Test> {
    let recipients = reserve_council_accounts();
    let count = recipients.len() as u128;
    let per = total / count;
    let mut left = total;
    let mut v = Vec::new();
    for (i, recipient_account_id) in recipients.into_iter().enumerate() {
        let amount = if i + 1 == count as usize { left } else { per };
        left = left.saturating_sub(amount);
        v.push(crate::proposal::RecipientAmount {
            recipient_account_id,
            amount,
        });
    }
    v.try_into().expect("allocations should fit")
}

mod cases;
