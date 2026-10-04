use super::*;
use votingengine::PopulationScope;

use frame_support::{assert_noop, assert_ok, derive_impl, traits::ConstU32, traits::Hooks};
use frame_system as system;
use primitives::cid::china::{china_cb::CHINA_CB, china_ch::CHINA_CH};
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};
use votingengine::{
    traits::{
        CitizenIdentityReader, InternalAdminProvider, JointVoteEngine, JointVoteResultCallback,
    },
    ProposalExecutionOutcome, STAGE_REFERENDUM, STATUS_EXECUTED, STATUS_PASSED, STATUS_VOTING,
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
    pub type JointVote = super;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
    type Block = Block;
    type AccountId = AccountId32;
    type Lookup = IdentityLookup<Self::AccountId>;
}

pub struct TestTimeProvider;
impl frame_support::traits::UnixTime for TestTimeProvider {
    fn now() -> core::time::Duration {
        core::time::Duration::from_secs(1_783_987_200)
    }
}

pub struct TestCitizenIdentityReader;
impl CitizenIdentityReader<AccountId32> for TestCitizenIdentityReader {
    fn voting_subject(
        who: &AccountId32,
        _scope: &PopulationScope,
    ) -> Option<votingengine::CitizenSubject<AccountId32>> {
        referendum_subject(who)
    }

    fn candidate_subject(
        who: &AccountId32,
        _scope: &PopulationScope,
    ) -> Option<votingengine::CitizenSubject<AccountId32>> {
        referendum_subject(who)
    }

    fn population_data(scope: &PopulationScope) -> Option<votingengine::PopulationData> {
        Some(votingengine::PopulationData {
            scope: scope.clone(),
            eligible_total: referendum_voters().len() as u64,
            eligibility_revision: 7,
            eligibility_date: 20_000,
        })
    }

    fn voting_subject_at(
        who: &AccountId32,
        population_data: &votingengine::PopulationData,
    ) -> Option<votingengine::CitizenSubject<AccountId32>> {
        (population_data.eligibility_revision == 7)
            .then(|| referendum_subject(who))
            .flatten()
    }
}

pub struct TestAdminProvider;
impl TestAdminProvider {
    fn institution_admins(
        institution_code: votingengine::InstitutionCode,
        cid_number: &[u8],
    ) -> Option<Vec<AccountId32>> {
        match institution_code {
            votingengine::NRC | votingengine::PRC => CHINA_CB
                .iter()
                .find(|entry| entry.cid_number.as_bytes() == cid_number)
                .map(|entry| entry.admins.iter().copied().map(AccountId32::new).collect()),
            votingengine::PRB => CHINA_CH
                .iter()
                .find(|entry| entry.cid_number.as_bytes() == cid_number)
                .map(|entry| entry.admins.iter().copied().map(AccountId32::new).collect()),
            _ => None,
        }
    }
}

impl InternalAdminProvider<AccountId32> for TestAdminProvider {
    fn is_institution_admin(
        institution_code: votingengine::InstitutionCode,
        cid_number: &[u8],
        who: &AccountId32,
    ) -> bool {
        Self::institution_admins(institution_code, cid_number)
            .map(|admins| admins.contains(who))
            .unwrap_or(false)
    }
}

impl votingengine::InstitutionRoleProvider<AccountId32> for TestAdminProvider {
    fn is_active_assignment(cid_number: &[u8], who: &AccountId32, role_code: &[u8]) -> bool {
        Self::active_accounts_for_role(cid_number, role_code).contains(who)
    }

    fn active_accounts_for_role(cid_number: &[u8], role_code: &[u8]) -> Vec<AccountId32> {
        let Ok(cid_text) = core::str::from_utf8(cid_number) else {
            return Vec::new();
        };
        let Some(code) = votingengine::types::institution_code_from_cid_number(cid_text) else {
            return Vec::new();
        };
        let expected_role = if matches!(code, votingengine::NRC | votingengine::PRC) {
            primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
        } else if code == votingengine::PRB {
            primitives::governance_skeleton::ROLE_CODE_DIRECTOR
        } else {
            return Vec::new();
        };
        if role_code != expected_role {
            return Vec::new();
        }
        Self::institution_admins(code, cid_number).unwrap_or_default()
    }
}

pub struct TestJointCallback;
impl JointVoteResultCallback for TestJointCallback {
    fn on_joint_vote_finalized(
        _vote_proposal_id: u64,
        _approved: bool,
    ) -> Result<ProposalExecutionOutcome, DispatchError> {
        Ok(ProposalExecutionOutcome::Executed)
    }
}

impl votingengine::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxVoteNonceLength = ConstU32<64>;
    type MaxVoteSignatureLength = ConstU32<64>;
    type MaxAutoFinalizePerBlock = ConstU32<128>;
    type MaxAutoFinalizeWeightPerBlock = votingengine::BlockWeightFraction<Test, 2>;
    type MaxExecutionWeightPerBlock = votingengine::BlockWeightFraction<Test, 2>;
    type MaxCleanupWeightPerBlock = votingengine::BlockWeightFraction<Test, 4>;
    type MaxProposalsPerExpiry = ConstU32<128>;
    type MaxInternalProposalMutexBindings = ConstU32<256>;
    type MaxActiveProposals = ConstU32<10>;
    type MaxCleanupStepsPerBlock = ConstU32<4>;
    type MaxCleanupActivationsPerBlock = ConstU32<64>;
    type CleanupKeysPerStep = ConstU32<8>;
    type MaxProposalDataLen = ConstU32<4096>;
    type MaxProposalObjectLen = ConstU32<10_240>;
    type MaxModuleTagLen = ConstU32<32>;
    type MaxManualExecutionAttempts = ConstU32<3>;
    type ExecutionRetryGraceBlocks = frame_support::traits::ConstU64<216>;
    type MaxExecutionRetryDeadlinesPerBlock = ConstU32<128>;
    type MaxPendingRetryExpirationsPerBlock = ConstU32<16>;
    type CitizenIdentityReader = TestCitizenIdentityReader;
    type JointVoteResultCallback = TestJointCallback;
    type InternalVoteResultCallback = ();
    type InternalAdminProvider = TestAdminProvider;
    type MaxAdminsPerInstitution = ConstU32<32>;
    type TimeProvider = TestTimeProvider;
    type WeightInfo = ();
    type TrackHandlers = (JointVote, ());
    type LegislationVoteResultCallback = ();
    type ElectionVoteResultCallback = ();
}

impl Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type InstitutionRoleProvider = TestAdminProvider;
    type WeightInfo = ();
}

fn new_test_ext() -> sp_io::TestExternalities {
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("frame system genesis storage should build");
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

fn referendum_voters() -> [AccountId32; 3] {
    [
        AccountId32::new([201; 32]),
        AccountId32::new([202; 32]),
        AccountId32::new([203; 32]),
    ]
}

fn replacement_account_id() -> AccountId32 {
    AccountId32::new([205; 32])
}

/// 更换绑定账户只更改签名账户；永久公民 CID 不变。
fn referendum_subject(who: &AccountId32) -> Option<votingengine::CitizenSubject<AccountId32>> {
    let voters = referendum_voters();
    let cid_source = if who == &replacement_account_id() {
        &voters[0]
    } else {
        voters.iter().find(|voter| *voter == who)?
    };
    Some(votingengine::CitizenSubject {
        cid_number: <AccountId32 as AsRef<[u8]>>::as_ref(cid_source)
            .to_vec()
            .try_into()
            .expect("account fits CID"),
        account_id: who.clone(),
    })
}

fn nrc_admin() -> AccountId32 {
    AccountId32::new(CHINA_CB[0].admins[0])
}

fn nrc_cid_number() -> Vec<u8> {
    CHINA_CB[0].cid_number.as_bytes().to_vec()
}

fn all_joint_institutions() -> Vec<(votingengine::InstitutionCode, Vec<u8>)> {
    CHINA_CB
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let code = if index == 0 {
                votingengine::NRC
            } else {
                votingengine::PRC
            };
            (code, entry.cid_number.as_bytes().to_vec())
        })
        .chain(
            CHINA_CH
                .iter()
                .map(|entry| (votingengine::PRB, entry.cid_number.as_bytes().to_vec())),
        )
        .collect()
}

fn admins_for(
    institution_code: votingengine::InstitutionCode,
    cid_number: &[u8],
) -> Vec<AccountId32> {
    TestAdminProvider::institution_admins(institution_code, cid_number)
        .expect("joint institution should have admins")
}

fn create_joint_proposal() -> u64 {
    let data = b"joint-test".to_vec();
    let hash = <Test as frame_system::Config>::Hashing::hash(data.as_slice());
    let mut business_object_hash = [0u8; 32];
    business_object_hash.copy_from_slice(hash.as_ref());
    let proposer_subject =
        votingengine::types::AuthorizationSubject::Institution(votingengine::types::RoleSubject {
            cid_number: nrc_cid_number().try_into().expect("bounded NRC CID"),
            role_code: primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                .to_vec()
                .try_into()
                .expect("bounded committee role"),
        });
    let voter_subjects = all_joint_institutions()
        .into_iter()
        .map(|(code, cid)| {
            let role_code = if matches!(code, votingengine::NRC | votingengine::PRC) {
                primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
            } else {
                primitives::governance_skeleton::ROLE_CODE_DIRECTOR
            };
            votingengine::types::AuthorizationSubject::Institution(
                votingengine::types::RoleSubject {
                    cid_number: cid.try_into().expect("bounded joint CID"),
                    role_code: role_code.to_vec().try_into().expect("bounded role code"),
                },
            )
        })
        .collect();
    let module_tag: frame_support::BoundedVec<u8, ConstU32<32>> = b"joint-test"
        .to_vec()
        .try_into()
        .expect("bounded module tag");
    let vote_plan = votingengine::types::VotePlanOf::try_new(
        votingengine::types::BusinessActionId {
            module_tag: module_tag.clone(),
            action_code: 0,
        },
        module_tag,
        proposer_subject,
        voter_subjects,
        votingengine::types::VotingEngineKind::Joint,
        business_object_hash,
    )
    .expect("valid joint vote plan");
    <JointVote as JointVoteEngine<AccountId32>>::create_joint_proposal_with_data(
        nrc_admin(),
        nrc_cid_number(),
        vote_plan,
        data,
    )
    .expect("joint proposal should be created")
}

#[test]
fn joint_proposal_binds_role_plan_and_never_writes_admin_snapshot() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal();
        let plan = votingengine::ProposalVotePlans::<Test>::get(proposal_id)
            .expect("joint proposal must bind vote plan");
        assert_eq!(plan.voter_subjects.len(), CHINA_CB.len() + CHINA_CH.len());
        assert_eq!(
            votingengine::VoterSnapshot::<Test>::iter_prefix(proposal_id).count(),
            CHINA_CB.len() + CHINA_CH.len()
        );
        assert_eq!(
            votingengine::InstitutionTicketCountSnapshot::<Test>::iter_prefix(proposal_id).count(),
            CHINA_CB.len() + CHINA_CH.len()
        );
        assert_eq!(
            votingengine::AdminSnapshot::<Test>::iter_prefix(proposal_id).count(),
            0,
            "migrated joint proposals must not write administrator snapshots"
        );
    });
}

#[test]
fn institution_ticket_count_preserves_same_account_id_in_multiple_roles() {
    new_test_ext().execute_with(|| {
        let proposal_id = 77;
        let account = AccountId32::new([77; 32]);
        let second = AccountId32::new([78; 32]);
        let cid_a: votingengine::types::CidNumber =
            nrc_cid_number().try_into().expect("bounded NRC CID");
        let cid_b: votingengine::types::CidNumber = CHINA_CB[1]
            .cid_number
            .as_bytes()
            .to_vec()
            .try_into()
            .expect("bounded PRC CID");
        let role = |cid: votingengine::types::CidNumber, code: &[u8]| {
            votingengine::types::AuthorizationSubject::Institution(
                votingengine::types::RoleSubject {
                    cid_number: cid,
                    role_code: code.to_vec().try_into().expect("bounded test role"),
                },
            )
        };

        let role_a = role(cid_a.clone(), b"ROLE_A");
        let role_b = role(cid_a.clone(), b"ROLE_B");
        let role_c = role(cid_b.clone(), b"ROLE_C");
        assert_ok!(VotingEngine::snapshot_role_voters(
            proposal_id,
            role_a.clone(),
            vec![account.clone(), second]
        ));
        assert_ok!(VotingEngine::snapshot_role_voters(
            proposal_id,
            role_b.clone(),
            vec![account.clone()]
        ));
        assert_ok!(VotingEngine::snapshot_role_voters(
            proposal_id,
            role_c.clone(),
            vec![account.clone()]
        ));

        assert_eq!(
            VotingEngine::institution_ticket_count(proposal_id, cid_a),
            Some(3),
            "同一账户在同一机构的两个岗位各保留一张票据"
        );
        assert_eq!(
            VotingEngine::institution_ticket_count(proposal_id, cid_b),
            Some(1)
        );
        assert!(VotingEngine::is_subject_voter_in_snapshot(
            proposal_id,
            role_a,
            &account
        ));
        assert!(VotingEngine::is_subject_voter_in_snapshot(
            proposal_id,
            role_b,
            &account
        ));
        assert!(VotingEngine::is_subject_voter_in_snapshot(
            proposal_id,
            role_c,
            &account
        ));
    });
}

fn finalize_institution(
    proposal_id: u64,
    institution_code: votingengine::InstitutionCode,
    cid_number: Vec<u8>,
    approve: bool,
) {
    let admins = admins_for(institution_code, &cid_number);
    let threshold = votingengine::types::fixed_governance_pass_threshold(&institution_code)
        .expect("fixed institution threshold") as usize;
    let required = if approve {
        threshold
    } else {
        admins.len().saturating_sub(threshold).saturating_add(1)
    };
    let voter_role_code: votingengine::RoleCode = if institution_code == votingengine::PRB {
        primitives::governance_skeleton::ROLE_CODE_DIRECTOR
    } else {
        primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
    }
    .to_vec()
    .try_into()
    .expect("institution role code should fit");
    for admin in admins.into_iter().take(required) {
        assert_ok!(JointVote::cast_admin(
            RuntimeOrigin::signed(admin),
            proposal_id,
            cid_number
                .clone()
                .try_into()
                .expect("institution CID should fit"),
            voter_role_code.clone(),
            approve,
        ));
    }
}

#[test]
fn joint_internal_requires_all_105_weight() {
    assert!(!is_joint_unanimous(JOINT_VOTE_PASS_THRESHOLD - 1));
    assert!(is_joint_unanimous(JOINT_VOTE_PASS_THRESHOLD));
}

#[test]
fn joint_referendum_uses_strict_majority() {
    assert!(!is_jointreferendum_vote_passed(50, 100));
    assert!(is_jointreferendum_vote_passed(51, 100));
    assert!(is_jointreferendum_vote_rejected(50, 100));
    assert!(!is_jointreferendum_vote_rejected(49, 100));
}

#[test]
fn joint_referendum_fails_closed_without_population() {
    assert!(!is_jointreferendum_vote_passed(1, 0));
    assert!(!is_jointreferendum_vote_rejected(1, 0));
}

#[test]
fn all_105_weight_via_cast_admin_passes_and_executes() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal();
        for (code, institution) in all_joint_institutions() {
            finalize_institution(proposal_id, code, institution, true);
        }

        assert_eq!(
            JointTallies::<Test>::get(proposal_id).yes,
            JOINT_VOTE_PASS_THRESHOLD
        );
        assert_eq!(
            VotingEngine::proposals(proposal_id).unwrap().status,
            STATUS_PASSED
        );

        <VotingEngine as Hooks<u64>>::on_initialize(System::block_number());
        assert_eq!(
            VotingEngine::proposals(proposal_id).unwrap().status,
            STATUS_EXECUTED
        );
    });
}

#[test]
fn one_institution_rejection_via_cast_admin_enters_referendum() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal();
        let cid_number = CHINA_CB[1].cid_number.as_bytes().to_vec();
        finalize_institution(proposal_id, votingengine::PRC, cid_number, false);

        let proposal = VotingEngine::proposals(proposal_id).expect("proposal should exist");
        assert_eq!(proposal.stage, STAGE_REFERENDUM);
        assert_eq!(proposal.status, STATUS_VOTING);
    });
}

#[test]
fn joint_internal_timeout_via_public_finalizer_enters_referendum() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal();
        let end = VotingEngine::proposals(proposal_id).unwrap().end;
        System::set_block_number(end + 1);

        assert_ok!(VotingEngine::finalize_proposal(
            RuntimeOrigin::signed(AccountId32::new([250; 32])),
            proposal_id,
        ));

        let proposal = VotingEngine::proposals(proposal_id).expect("proposal should exist");
        assert_eq!(proposal.stage, STAGE_REFERENDUM);
        assert_eq!(proposal.status, STATUS_VOTING);
    });
}

#[test]
fn cast_referendum_extrinsic_uses_frozen_snapshot_and_strict_majority() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal();
        let cid_number = CHINA_CB[1].cid_number.as_bytes().to_vec();
        finalize_institution(proposal_id, votingengine::PRC, cid_number, false);

        let voters = referendum_voters();
        assert_ok!(JointVote::cast_referendum(
            RuntimeOrigin::signed(voters[0].clone()),
            proposal_id,
            true,
        ));
        assert_eq!(
            VotingEngine::proposals(proposal_id).unwrap().status,
            STATUS_VOTING
        );
        assert_ok!(JointVote::cast_referendum(
            RuntimeOrigin::signed(voters[1].clone()),
            proposal_id,
            true,
        ));

        assert_eq!(ReferendumTallies::<Test>::get(proposal_id).yes, 2);
        assert_eq!(
            VotingEngine::proposals(proposal_id).unwrap().status,
            STATUS_PASSED
        );
    });
}

#[test]
fn newly_added_voter_cannot_enter_existing_snapshot() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal();
        let cid_number = CHINA_CB[1].cid_number.as_bytes().to_vec();
        finalize_institution(proposal_id, votingengine::PRC, cid_number, false);

        let post_snapshot_account = AccountId32::new([204; 32]);
        assert!(JointVote::cast_referendum(
            RuntimeOrigin::signed(post_snapshot_account),
            proposal_id,
            true,
        )
        .is_err());
        let post_snapshot_cid: votingengine::types::CidNumber =
            vec![204; 32].try_into().expect("account fits CID");
        assert!(!ReferendumVotesByCid::<Test>::contains_key(
            proposal_id,
            post_snapshot_cid,
        ));
    });
}

#[test]
fn same_citizen_cid_cannot_vote_twice_after_account_id_replacement() {
    new_test_ext().execute_with(|| {
        let proposal_id = create_joint_proposal();
        let cid_number = CHINA_CB[1].cid_number.as_bytes().to_vec();
        finalize_institution(proposal_id, votingengine::PRC, cid_number, false);

        let first_account_id = referendum_voters()[0].clone();
        assert_ok!(JointVote::cast_referendum(
            RuntimeOrigin::signed(first_account_id.clone()),
            proposal_id,
            true,
        ));
        assert_noop!(
            JointVote::cast_referendum(
                RuntimeOrigin::signed(replacement_account_id()),
                proposal_id,
                false,
            ),
            votingengine::Error::<Test>::AlreadyVoted
        );

        let subject = referendum_subject(&first_account_id).expect("first voter is eligible");
        let ticket = ReferendumVotesByCid::<Test>::get(proposal_id, &subject.cid_number)
            .expect("complete citizen referendum ticket should exist");
        assert_eq!(ticket.voter_subject, subject);
        assert!(ticket.approve);
        assert_eq!(ReferendumTallies::<Test>::get(proposal_id).yes, 1);
        assert_eq!(ReferendumTallies::<Test>::get(proposal_id).no, 0);
    });
}
