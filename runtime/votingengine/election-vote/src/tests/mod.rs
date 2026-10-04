#![cfg(test)]

//! election-vote 状态机测试 runtime。
//!
//! 普选使用 citizen-identity 人口数据生成提案快照，互选使用 VotePlan 岗位任职
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
use votingengine::{
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
    pub type VotingEngine = votingengine;

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

fn institution_cid_number() -> votingengine::types::CidNumber {
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
    fn citizen_subject(who: &AccountId32) -> Option<votingengine::CitizenSubject<AccountId32>> {
        (*who != account(251)).then(|| test_citizen_subject(who))
    }

    fn voting_subject(
        who: &AccountId32,
        _scope: &PopulationScope,
    ) -> Option<votingengine::CitizenSubject<AccountId32>> {
        (*who != account(250)).then(|| test_citizen_subject(who))
    }

    fn candidate_subject(
        who: &AccountId32,
        _scope: &PopulationScope,
    ) -> Option<votingengine::CitizenSubject<AccountId32>> {
        (*who != account(249) && *who != account(251)).then(|| test_citizen_subject(who))
    }

    fn population_data(scope: &PopulationScope) -> Option<votingengine::PopulationData> {
        if !POPULATION_READY.with(|ready| *ready.borrow()) {
            return None;
        }
        Some(votingengine::PopulationData {
            scope: scope.clone(),
            eligible_total: POPULATION_COUNT.with(|count| *count.borrow()),
            eligibility_revision: 1,
            eligibility_date: 20_000,
        })
    }

    fn voting_subject_at(
        who: &AccountId32,
        population_data: &votingengine::PopulationData,
    ) -> Option<votingengine::CitizenSubject<AccountId32>> {
        ((0..population_data.eligible_total)
            .map(|offset| account(21u8.saturating_add(offset as u8)))
            .any(|voter| &voter == who)
            || *who == account(29))
        .then(|| test_citizen_subject(who))
    }
}

fn test_citizen_subject(who: &AccountId32) -> votingengine::CitizenSubject<AccountId32> {
    let permanent_account = if *who == account(29) {
        account(21)
    } else {
        who.clone()
    };
    votingengine::CitizenSubject {
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

fn elected_role_code() -> votingengine::types::RoleCode {
    ELECTED_ROLE.to_vec().try_into().expect("role fits")
}

fn candidate(id: u8) -> votingengine::CitizenSubject<AccountId32> {
    test_citizen_subject(&account(id))
}

fn vote_plan(mutual: bool) -> votingengine::types::VotePlanOf<AccountId32> {
    let owner: frame_support::BoundedVec<
        u8,
        ConstU32<{ entity_primitives::BUSINESS_MODULE_TAG_MAX_BYTES }>,
    > = b"test-election-business"
        .to_vec()
        .try_into()
        .expect("owner fits");
    let proposer =
        votingengine::AuthorizationSubject::Institution(entity_primitives::RoleSubject {
            cid_number: institution_cid_number(),
            role_code: ORGANIZER_ROLE.to_vec().try_into().expect("role fits"),
        });
    let voters = mutual
        .then(|| {
            votingengine::AuthorizationSubject::Institution(entity_primitives::RoleSubject {
                cid_number: institution_cid_number(),
                role_code: TARGET_ROLE.to_vec().try_into().expect("role fits"),
            })
        })
        .into_iter()
        .collect();
    votingengine::types::VotePlanOf::try_new(
        entity_primitives::BusinessActionId {
            module_tag: owner.clone(),
            action_code: 0,
        },
        owner,
        proposer,
        voters,
        votingengine::VotingEngineKind::Election,
        [7u8; 32],
    )
    .expect("election plan valid")
}

fn multi_role_vote_plan() -> votingengine::types::VotePlanOf<AccountId32> {
    let mut plan = vote_plan(true);
    plan.voter_subjects
        .try_push(votingengine::AuthorizationSubject::Institution(
            entity_primitives::RoleSubject {
                cid_number: institution_cid_number(),
                role_code: SECOND_VOTER_ROLE.to_vec().try_into().expect("role fits"),
            },
        ))
        .expect("second role fits vote plan");
    plan
}

fn create_popular(candidates: Vec<votingengine::CitizenSubject<AccountId32>>) -> u64 {
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

#[test]
fn popular_election_uses_population_snapshot_and_generates_result() {
    new_test_ext().execute_with(|| {
        let candidates = vec![candidate(11), candidate(12)];
        let voters = [account(21), account(22), account(23)];
        let proposal_id = create_popular(candidates.clone());

        // 创建后人口增长不能把新账户塞进既有普选；Popular 不保存全量选民表。
        POPULATION_COUNT.with(|count| *count.borrow_mut() = 4);
        assert_noop!(
            ElectionVote::cast_popular_vote(
                RuntimeOrigin::signed(account(24)),
                proposal_id,
                candidates[0].clone()
            ),
            Error::<Test>::VoterNotEligible
        );

        assert_ok!(ElectionVote::cast_popular_vote(
            RuntimeOrigin::signed(voters[0].clone()),
            proposal_id,
            candidates[0].clone()
        ));
        assert_ok!(ElectionVote::cast_popular_vote(
            RuntimeOrigin::signed(voters[1].clone()),
            proposal_id,
            candidates[0].clone()
        ));
        assert_ok!(ElectionVote::cast_popular_vote(
            RuntimeOrigin::signed(voters[2].clone()),
            proposal_id,
            candidates[1].clone()
        ));

        let proposal = votingengine::pallet::Proposals::<Test>::get(proposal_id).unwrap();
        assert_eq!(proposal.status, STATUS_PASSED);
        assert_eq!(
            VotingEngine::population_eligible_total_of(proposal_id),
            Some(3)
        );
        let winners = ElectionResults::<Test>::get(proposal_id).unwrap();
        assert_eq!(winners[0].candidate_subject, candidates[0]);
        assert_eq!(winners[0].votes, 2);
        assert_eq!(
            ElectionVote::on_election_vote_finalized(proposal_id, true),
            Ok(ProposalExecutionOutcome::Executed)
        );
    });
}

#[test]
fn mutual_election_uses_role_assignment_snapshot() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_mutual();
        let admins = target_admins();
        for voter in admins.iter() {
            assert_ok!(ElectionVote::cast_mutual_vote(
                RuntimeOrigin::signed(voter.clone()),
                proposal_id,
                TARGET_ROLE.to_vec().try_into().expect("role fits"),
                candidate(11)
            ));
        }

        let meta = ElectionMetaStore::<Test>::get(proposal_id).unwrap();
        assert_eq!(meta.mode, ElectionMode::Mutual);
        assert_eq!(meta.actor_cid_number, institution_cid_number());
        assert_eq!(meta.role_code, elected_role_code());
        assert_eq!(
            votingengine::pallet::Proposals::<Test>::get(proposal_id)
                .unwrap()
                .status,
            STATUS_PASSED
        );
    });
}

#[test]
fn creation_rejects_vote_plan_without_mutual_voter_role() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            ElectionVote::do_create_mutual_election(
                organizer_admin(),
                vote_plan(false),
                institution_cid_number(),
                elected_role_code(),
                1,
                10,
                20,
                vec![candidate(11)],
            ),
            Error::<Test>::InvalidVotePlan
        );
    });
}

#[test]
fn popular_creation_rejects_invalid_candidate_subject_and_bad_shape() {
    new_test_ext().execute_with(|| {
        let invalid_subject = votingengine::CitizenSubject {
            cid_number: candidate(12).cid_number,
            // 249 有普通公民/投票身份，但没有竞选身份，不能进入候选人快照。
            account_id: account(249),
        };
        assert_noop!(
            ElectionVote::do_create_popular_election(
                organizer_admin(),
                vote_plan(false),
                institution_cid_number(),
                elected_role_code(),
                1,
                10,
                20,
                PopulationScope::Country,
                vec![candidate(11), invalid_subject]
            ),
            Error::<Test>::CandidateSubjectInvalid
        );
        assert_noop!(
            ElectionVote::do_create_popular_election(
                account(9),
                vote_plan(false),
                institution_cid_number(),
                elected_role_code(),
                1,
                10,
                20,
                PopulationScope::Country,
                vec![candidate(11), candidate(12)]
            ),
            Error::<Test>::NotOrganizerAdmin
        );
    });
}

#[test]
fn popular_creation_rejects_population_data_that_is_not_ready() {
    new_test_ext().execute_with(|| {
        POPULATION_READY.with(|ready| *ready.borrow_mut() = false);
        assert_noop!(
            ElectionVote::do_create_popular_election(
                organizer_admin(),
                vote_plan(false),
                institution_cid_number(),
                elected_role_code(),
                1,
                10,
                20,
                PopulationScope::Country,
                vec![candidate(11), candidate(12)]
            ),
            votingengine::Error::<Test>::PopulationDataNotReady
        );
        assert_eq!(votingengine::Proposals::<Test>::iter().count(), 0);
        assert_eq!(
            votingengine::ProposalPopulationSnapshots::<Test>::iter().count(),
            0
        );
    });
}

#[test]
fn cast_rejects_wrong_voter_candidate_stage_and_duplicate_vote() {
    new_test_ext().execute_with(|| {
        let candidates = vec![candidate(11), candidate(12)];
        let voters = [account(21), account(22), account(23)];
        let proposal_id = create_popular(candidates.clone());
        assert_noop!(
            ElectionVote::cast_popular_vote(
                RuntimeOrigin::signed(account(99)),
                proposal_id,
                candidates[0].clone()
            ),
            Error::<Test>::VoterNotEligible
        );
        assert_noop!(
            ElectionVote::cast_popular_vote(
                RuntimeOrigin::signed(voters[0].clone()),
                proposal_id,
                candidate(99)
            ),
            Error::<Test>::CandidateNotInSnapshot
        );
        assert_noop!(
            ElectionVote::cast_mutual_vote(
                RuntimeOrigin::signed(voters[0].clone()),
                proposal_id,
                TARGET_ROLE.to_vec().try_into().expect("role fits"),
                candidates[0].clone()
            ),
            votingengine::Error::<Test>::InvalidProposalStage
        );
        assert_ok!(ElectionVote::cast_popular_vote(
            RuntimeOrigin::signed(voters[0].clone()),
            proposal_id,
            candidates[0].clone()
        ));
        assert_noop!(
            ElectionVote::cast_popular_vote(
                RuntimeOrigin::signed(voters[0].clone()),
                proposal_id,
                candidates[0].clone()
            ),
            votingengine::Error::<Test>::AlreadyVoted
        );
    });
}

#[test]
fn same_citizen_cid_cannot_vote_twice_after_account_id_replacement() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_popular(vec![candidate(11), candidate(12)]);
        let original_account_id = account(21);
        let replacement_account_id = account(29);
        let voter_subject = test_citizen_subject(&original_account_id);

        assert_ok!(ElectionVote::cast_popular_vote(
            RuntimeOrigin::signed(original_account_id),
            proposal_id,
            candidate(11),
        ));
        let ticket = PopularElectionVotesByCid::<Test>::get(proposal_id, &voter_subject.cid_number)
            .expect("complete popular election ticket is stored");
        assert_eq!(ticket.voter_subject, voter_subject);
        assert_eq!(ticket.candidate_subject, candidate(11));

        assert_noop!(
            ElectionVote::cast_popular_vote(
                RuntimeOrigin::signed(replacement_account_id),
                proposal_id,
                candidate(11),
            ),
            votingengine::Error::<Test>::AlreadyVoted
        );
    });
}

#[test]
fn candidate_snapshot_rejects_duplicate_permanent_cid() {
    new_test_ext().execute_with(|| {
        let duplicate_cid_subject = votingengine::CitizenSubject {
            cid_number: candidate(11).cid_number,
            account_id: account(12),
        };
        assert_noop!(
            ElectionVote::do_create_popular_election(
                organizer_admin(),
                vote_plan(false),
                institution_cid_number(),
                elected_role_code(),
                1,
                10,
                20,
                PopulationScope::Country,
                vec![candidate(11), duplicate_cid_subject],
            ),
            Error::<Test>::DuplicateCandidateCid
        );
    });
}

#[test]
fn same_admin_can_vote_once_for_each_frozen_institution_role() {
    new_test_ext().execute_with(|| {
        let proposal_id = ElectionVote::do_create_mutual_election(
            organizer_admin(),
            multi_role_vote_plan(),
            institution_cid_number(),
            elected_role_code(),
            1,
            10,
            20,
            vec![candidate(11), candidate(12)],
        )
        .expect("multi-role mutual election should be created");
        let voter = target_admins()[0].clone();

        assert_ok!(ElectionVote::cast_mutual_vote(
            RuntimeOrigin::signed(voter.clone()),
            proposal_id,
            TARGET_ROLE.to_vec().try_into().expect("role fits"),
            candidate(11),
        ));
        assert_ok!(ElectionVote::cast_mutual_vote(
            RuntimeOrigin::signed(voter),
            proposal_id,
            SECOND_VOTER_ROLE.to_vec().try_into().expect("role fits"),
            candidate(11),
        ));

        assert_eq!(
            MutualElectionVotesByTicket::<Test>::iter_prefix(proposal_id).count(),
            2
        );
        assert_eq!(
            ElectionCandidateTallies::<Test>::get(proposal_id, candidate(11).cid_number),
            2
        );
    });
}

#[test]
fn timeout_is_rejected_before_expiry_then_finalizes_no_vote_election() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_popular(vec![candidate(11), candidate(12)]);
        let proposal = votingengine::pallet::Proposals::<Test>::get(proposal_id).unwrap();
        assert_noop!(
            ElectionVote::finalize_election_popular_timeout(&proposal, proposal_id),
            votingengine::Error::<Test>::VoteNotExpired
        );
        System::set_block_number(proposal.end + 1);
        assert_ok!(VotingEngine::finalize_proposal(
            RuntimeOrigin::signed(account(99)),
            proposal_id
        ));
        assert_eq!(
            votingengine::pallet::Proposals::<Test>::get(proposal_id)
                .unwrap()
                .status,
            STATUS_REJECTED
        );
    });
}

#[test]
fn election_cleanup_removes_all_track_storage() {
    new_test_ext().execute_with(|| {
        let candidates = vec![candidate(11), candidate(12)];
        let voters = [account(21), account(22), account(23)];
        let proposal_id = create_popular(candidates.clone());
        assert_ok!(ElectionVote::cast_popular_vote(
            RuntimeOrigin::signed(voters[0].clone()),
            proposal_id,
            candidates[0].clone()
        ));

        assert!(
            <ElectionVote as ProposalTrackHandler<u64, AccountId32>>::cleanup_chunk(
                votingengine::PROPOSAL_KIND_ELECTION,
                proposal_id,
                1,
            )
            .is_some()
        );
        let _ = <ElectionVote as ProposalTrackHandler<u64, AccountId32>>::cleanup_chunk(
            votingengine::PROPOSAL_KIND_ELECTION,
            proposal_id,
            8,
        );
        assert_eq!(
            <ElectionVote as ProposalTrackHandler<u64, AccountId32>>::cleanup_terminal(
                votingengine::PROPOSAL_KIND_ELECTION,
                proposal_id,
            ),
            Some(())
        );

        assert!(!ElectionMetaStore::<Test>::contains_key(proposal_id));
        assert!(!ElectionCandidates::<Test>::contains_key(proposal_id));
        assert!(!ElectionResults::<Test>::contains_key(proposal_id));
        assert!(PopularElectionVotesByCid::<Test>::iter_prefix(proposal_id)
            .next()
            .is_none());
        assert!(
            MutualElectionVotesByTicket::<Test>::iter_prefix(proposal_id)
                .next()
                .is_none()
        );
        assert!(ElectionCandidateTallies::<Test>::iter_prefix(proposal_id)
            .next()
            .is_none());
        assert!(!ElectionTallyStore::<Test>::contains_key(proposal_id));
    });
}

#[test]
fn result_callback_fails_closed_for_incomplete_or_foreign_result() {
    new_test_ext().execute_with(|| {
        assert_eq!(
            ElectionVote::on_election_vote_finalized(999, true),
            Ok(ProposalExecutionOutcome::Ignored)
        );
        let proposal_id = create_popular(vec![candidate(11), candidate(12)]);
        assert_eq!(
            ElectionVote::on_election_vote_finalized(proposal_id, true),
            Ok(ProposalExecutionOutcome::FatalFailed)
        );
        assert_eq!(
            ElectionVote::on_election_vote_finalized(proposal_id, false),
            Ok(ProposalExecutionOutcome::Executed)
        );
    });
}
