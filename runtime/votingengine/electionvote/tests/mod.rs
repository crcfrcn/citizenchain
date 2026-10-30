#![cfg(test)]

//! electionvote 状态机测试 runtime。
//!
//! 普选使用 identity 人口数据生成提案快照，互选使用 VotePlan 岗位任职
//! 快照；测试覆盖创建、投票、超时、结果回调和分块清理。

use core::cell::RefCell;

use frame_support::{
    assert_noop, assert_ok, derive_impl,
    traits::{ConstU32, ConstU64},
};
use frame_system as system;
use primitives::cid::{
    china::china_lf::CHINA_LF,
    code::{institution_code_from_cid_number, InstitutionCode, NLG},
};
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};
use votebase::{
    CitizenIdentityReader, ElectionProposalFinalizer, ElectionVoteResultCallback,
    InstitutionRoleProvider, InternalAdminProvider, PopulationScope, ProposalExecutionOutcome,
    ProposalTrackHandler, STATUS_PASSED, STATUS_REJECTED,
};

use crate::{
    pallet::{
        ElectionCandidateTallies, ElectionCandidates, ElectionMetaStore, ElectionResults,
        ElectionTallyStore, Error, MutualElectionVotesByTicket, PopularElectionVotesByCid,
    },
    types::ElectionMode,
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
    pub type VotingEngine = votebase;

    #[runtime::pallet_index(2)]
    pub type ElectionVote = super;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
    type Block = Block;
    type AccountId = AccountId32;
    type Lookup = IdentityLookup<Self::AccountId>;
}

const INSTITUTION_CODE: InstitutionCode = NLG;

fn account(id: u8) -> AccountId32 {
    AccountId32::new([id; 32])
}

fn organizer_admin() -> AccountId32 {
    account(2)
}

fn institution_cid_number() -> votebase::types::CidNumber {
    CHINA_LF
        .iter()
        .find(|entry| institution_code_from_cid_number(entry.cid_number) == Some(INSTITUTION_CODE))
        .expect("national legislature CID should exist")
        .cid_number
        .as_bytes()
        .to_vec()
        .try_into()
        .expect("institution CID should fit")
}

fn target_admins() -> Vec<AccountId32> {
    vec![account(31), account(32), account(33)]
}

thread_local! {
    static POPULATION_COUNT: RefCell<u64> = const { RefCell::new(3) };
    static POPULATION_READY: RefCell<bool> = const { RefCell::new(true) };
}

pub struct TestCitizenIdentityReader;
pub struct TestInternalAdminProvider;
pub struct TestInstitutionRoleProvider;

const ORGANIZER_ROLE: &[u8] = b"ORGANIZER";
const TARGET_ROLE: &[u8] = b"MEMBER";
const SECOND_VOTER_ROLE: &[u8] = b"SECOND_MEMBER";
const ELECTED_ROLE: &[u8] = b"SPEAKER";

impl CitizenIdentityReader<AccountId32> for TestCitizenIdentityReader {
    fn citizen_subject(who: &AccountId32) -> Option<votebase::CitizenSubject<AccountId32>> {
        (*who != account(251)).then(|| test_citizen_subject(who))
    }

    fn voting_subject(
        who: &AccountId32,
        _scope: &PopulationScope,
    ) -> Option<votebase::CitizenSubject<AccountId32>> {
        (*who != account(250)).then(|| test_citizen_subject(who))
    }

    fn candidate_subject(
        who: &AccountId32,
        _scope: &PopulationScope,
    ) -> Option<votebase::CitizenSubject<AccountId32>> {
        (*who != account(249) && *who != account(251)).then(|| test_citizen_subject(who))
    }

    fn population_data(scope: &PopulationScope) -> Option<votebase::PopulationData> {
        if !POPULATION_READY.with(|ready| *ready.borrow()) {
            return None;
        }
        Some(votebase::PopulationData {
            scope: scope.clone(),
            eligible_total: POPULATION_COUNT.with(|count| *count.borrow()),
            eligibility_revision: 1,
            eligibility_date: 20_000,
        })
    }

    fn voting_subject_at(
        who: &AccountId32,
        population_data: &votebase::PopulationData,
    ) -> Option<votebase::CitizenSubject<AccountId32>> {
        ((0..population_data.eligible_total)
            .map(|offset| account(21u8.saturating_add(offset as u8)))
            .any(|voter| &voter == who)
            || *who == account(29))
        .then(|| test_citizen_subject(who))
    }
}

fn test_citizen_subject(who: &AccountId32) -> votebase::CitizenSubject<AccountId32> {
    let permanent_account = if *who == account(29) {
        account(21)
    } else {
        who.clone()
    };
    votebase::CitizenSubject {
        cid_number: <AccountId32 as AsRef<[u8]>>::as_ref(&permanent_account)
            .to_vec()
            .try_into()
            .expect("account fits CID"),
        account_id: who.clone(),
    }
}

impl InternalAdminProvider<AccountId32> for TestInternalAdminProvider {
    fn is_institution_admin(
        institution_code: InstitutionCode,
        cid_number: &[u8],
        who: &AccountId32,
    ) -> bool {
        institution_code == INSTITUTION_CODE
            && cid_number == institution_cid_number().as_slice()
            && *who == organizer_admin()
    }
}

impl InstitutionRoleProvider<AccountId32> for TestInstitutionRoleProvider {
    fn is_active_assignment(cid_number: &[u8], who: &AccountId32, role_code: &[u8]) -> bool {
        cid_number == institution_cid_number().as_slice()
            && who == &organizer_admin()
            && role_code == ORGANIZER_ROLE
    }

    fn active_accounts_for_role(cid_number: &[u8], role_code: &[u8]) -> Vec<AccountId32> {
        if cid_number == institution_cid_number().as_slice() && role_code == TARGET_ROLE {
            target_admins()
        } else if cid_number == institution_cid_number().as_slice()
            && role_code == SECOND_VOTER_ROLE
        {
            vec![target_admins()[0].clone()]
        } else if cid_number == institution_cid_number().as_slice() && role_code == ORGANIZER_ROLE {
            vec![organizer_admin()]
        } else {
            Vec::new()
        }
    }
}

pub struct TestTimeProvider;

impl frame_support::traits::UnixTime for TestTimeProvider {
    fn now() -> core::time::Duration {
        core::time::Duration::from_secs(1_782_864_000)
    }
}

impl votebase::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxVoteNonceLength = ConstU32<64>;
    type MaxVoteSignatureLength = ConstU32<64>;
    type MaxAutoFinalizePerBlock = ConstU32<64>;
    type MaxAutoFinalizeWeightPerBlock = votebase::BlockWeightFraction<Test, 4>;
    type MaxExecutionWeightPerBlock = votebase::BlockWeightFraction<Test, 4>;
    type MaxCleanupWeightPerBlock = votebase::BlockWeightFraction<Test, 8>;
    type MaxProposalsPerExpiry = ConstU32<128>;
    type MaxInternalProposalMutexBindings = ConstU32<256>;
    type MaxActiveProposals = ConstU32<10>;
    type MaxCleanupStepsPerBlock = ConstU32<8>;
    type CleanupKeysPerStep = ConstU32<2>;
    type MaxProposalDataLen = ConstU32<1024>;
    type MaxProposalObjectLen = ConstU32<{ 64 * 1024 }>;
    type MaxModuleTagLen = ConstU32<32>;
    type MaxManualExecutionAttempts = ConstU32<3>;
    type ExecutionRetryGraceBlocks = ConstU64<216>;
    type MaxExecutionRetryDeadlinesPerBlock = ConstU32<128>;
    type MaxCleanupActivationsPerBlock = ConstU32<50>;
    type MaxPendingRetryExpirationsPerBlock = ConstU32<16>;
    type CitizenIdentityReader = TestCitizenIdentityReader;
    type JointVoteResultCallback = ();
    type InternalVoteResultCallback = ();
    type InternalAdminProvider = TestInternalAdminProvider;
    type MaxAdminsPerInstitution = ConstU32<16>;
    type TimeProvider = TestTimeProvider;
    type WeightInfo = ();
    type TrackHandlers = (ElectionVote, ());
    type LegislationVoteResultCallback = ();
    type ElectionVoteResultCallback = ElectionVote;
}

impl crate::pallet::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxElectionCandidates = ConstU32<8>;
    type InstitutionRoleProvider = TestInstitutionRoleProvider;
    type WeightInfo = ();
}

fn new_test_ext() -> sp_io::TestExternalities {
    POPULATION_COUNT.with(|count| *count.borrow_mut() = 3);
    POPULATION_READY.with(|ready| *ready.borrow_mut() = true);
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("test storage should build");
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

fn elected_role_code() -> votebase::types::RoleCode {
    ELECTED_ROLE.to_vec().try_into().expect("role fits")
}

fn candidate(id: u8) -> votebase::CitizenSubject<AccountId32> {
    test_citizen_subject(&account(id))
}

fn vote_plan(mutual: bool) -> votebase::types::VotePlanOf<AccountId32> {
    let owner: frame_support::BoundedVec<
        u8,
        ConstU32<{ entitybase::BUSINESS_MODULE_TAG_MAX_BYTES }>,
    > = b"test-election-business"
        .to_vec()
        .try_into()
        .expect("owner fits");
    let proposer = votebase::AuthorizationSubject::Institution(entitybase::RoleSubject {
        cid_number: institution_cid_number(),
        role_code: ORGANIZER_ROLE.to_vec().try_into().expect("role fits"),
    });
    let voters = mutual
        .then(|| {
            votebase::AuthorizationSubject::Institution(entitybase::RoleSubject {
                cid_number: institution_cid_number(),
                role_code: TARGET_ROLE.to_vec().try_into().expect("role fits"),
            })
        })
        .into_iter()
        .collect();
    votebase::types::VotePlanOf::try_new(
        entitybase::BusinessActionId {
            module_tag: owner.clone(),
            action_code: 0,
        },
        owner,
        proposer,
        voters,
        votebase::VotingEngineKind::Election,
        [7u8; 32],
    )
    .expect("election plan valid")
}

fn multi_role_vote_plan() -> votebase::types::VotePlanOf<AccountId32> {
    let mut plan = vote_plan(true);
    plan.voter_subjects
        .try_push(votebase::AuthorizationSubject::Institution(
            entitybase::RoleSubject {
                cid_number: institution_cid_number(),
                role_code: SECOND_VOTER_ROLE.to_vec().try_into().expect("role fits"),
            },
        ))
        .expect("second role fits vote plan");
    plan
}

fn create_popular(candidates: Vec<votebase::CitizenSubject<AccountId32>>) -> u64 {
    ElectionVote::do_create_popular_election(
        organizer_admin(),
        vote_plan(false),
        institution_cid_number(),
        elected_role_code(),
        1,
        10,
        20,
        PopulationScope::Country,
        candidates,
    )
    .expect("popular election should be created")
}

fn create_mutual() -> u64 {
    ElectionVote::do_create_mutual_election(
        organizer_admin(),
        vote_plan(true),
        institution_cid_number(),
        elected_role_code(),
        1,
        10,
        20,
        vec![candidate(11), candidate(12)],
    )
    .expect("mutual election should be created")
}

mod tally {

    use crate::tally::*;

    fn subject(id: u8) -> votebase::CitizenSubject<u8> {
        votebase::CitizenSubject {
            cid_number: vec![id].try_into().expect("test CID fits"),
            account_id: id,
        }
    }

    #[test]
    fn selects_top_seats_without_tie() {
        let winners = select_winners(&[(subject(1), 9), (subject(2), 5), (subject(3), 7)], 2)
            .expect("unique winners");
        assert_eq!(winners[0].candidate_subject, subject(1));
        assert_eq!(winners[1].candidate_subject, subject(3));
    }

    #[test]
    fn accepts_tie_that_fits_remaining_seats() {
        let winners = select_winners(&[(subject(1), 9), (subject(2), 9), (subject(3), 1)], 2)
            .expect("tie fits");
        assert_eq!(winners.len(), 2);
        assert_eq!(winners[0].seat_index, 0);
        assert_eq!(winners[1].seat_index, 1);
    }

    #[test]
    fn rejects_tie_that_crosses_seat_boundary() {
        assert!(select_winners(&[(subject(1), 9), (subject(2), 8), (subject(3), 8)], 2).is_err());
    }
}

mod unit;
