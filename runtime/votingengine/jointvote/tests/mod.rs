use super::*;
use votebase::PopulationScope;

use frame_support::{assert_noop, assert_ok, derive_impl, traits::ConstU32, traits::Hooks};
use frame_system as system;
use primitives::cid::china::{china_cb::CHINA_CB, china_ch::CHINA_CH};
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};
use votebase::{
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
    pub type VotingEngine = votebase;
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
    ) -> Option<votebase::CitizenSubject<AccountId32>> {
        referendum_subject(who)
    }

    fn candidate_subject(
        who: &AccountId32,
        _scope: &PopulationScope,
    ) -> Option<votebase::CitizenSubject<AccountId32>> {
        referendum_subject(who)
    }

    fn population_data(scope: &PopulationScope) -> Option<votebase::PopulationData> {
        Some(votebase::PopulationData {
            scope: scope.clone(),
            eligible_total: referendum_voters().len() as u64,
            eligibility_revision: 7,
            eligibility_date: 20_000,
        })
    }

    fn voting_subject_at(
        who: &AccountId32,
        population_data: &votebase::PopulationData,
    ) -> Option<votebase::CitizenSubject<AccountId32>> {
        (population_data.eligibility_revision == 7)
            .then(|| referendum_subject(who))
            .flatten()
    }
}

pub struct TestAdminProvider;
impl TestAdminProvider {
    fn institution_admins(
        institution_code: votebase::InstitutionCode,
        cid_number: &[u8],
    ) -> Option<Vec<AccountId32>> {
        match institution_code {
            votebase::NRC | votebase::PRC => CHINA_CB
                .iter()
                .find(|entry| entry.cid_number.as_bytes() == cid_number)
                .map(|entry| entry.admins.iter().copied().map(AccountId32::new).collect()),
            votebase::PRB => CHINA_CH
                .iter()
                .find(|entry| entry.cid_number.as_bytes() == cid_number)
                .map(|entry| entry.admins.iter().copied().map(AccountId32::new).collect()),
            _ => None,
        }
    }
}

impl InternalAdminProvider<AccountId32> for TestAdminProvider {
    fn is_institution_admin(
        institution_code: votebase::InstitutionCode,
        cid_number: &[u8],
        who: &AccountId32,
    ) -> bool {
        Self::institution_admins(institution_code, cid_number)
            .map(|admins| admins.contains(who))
            .unwrap_or(false)
    }
}

impl votebase::InstitutionRoleProvider<AccountId32> for TestAdminProvider {
    fn is_active_assignment(cid_number: &[u8], who: &AccountId32, role_code: &[u8]) -> bool {
        Self::active_accounts_for_role(cid_number, role_code).contains(who)
    }

    fn active_accounts_for_role(cid_number: &[u8], role_code: &[u8]) -> Vec<AccountId32> {
        let Ok(cid_text) = core::str::from_utf8(cid_number) else {
            return Vec::new();
        };
        let Some(code) = votebase::types::institution_code_from_cid_number(cid_text) else {
            return Vec::new();
        };
        let expected_role = if matches!(code, votebase::NRC | votebase::PRC) {
            primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
        } else if code == votebase::PRB {
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

// 本模块验证业务状态转换；有界预算由votebase专用测试与Runtime预算测试覆盖。
frame_support::parameter_types! { pub const LogicBudget: frame_support::weights::Weight = frame_support::weights::Weight::MAX; }

impl votebase::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxVoteNonceLength = ConstU32<64>;
    type MaxVoteSignatureLength = ConstU32<64>;
    type MaxAutoFinalizePerBlock = ConstU32<128>;
    type MaxAutoFinalizeWeightPerBlock = LogicBudget;
    type MaxExecutionWeightPerBlock = LogicBudget;
    type MaxCleanupWeightPerBlock = LogicBudget;
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
fn referendum_subject(who: &AccountId32) -> Option<votebase::CitizenSubject<AccountId32>> {
    let voters = referendum_voters();
    let cid_source = if who == &replacement_account_id() {
        &voters[0]
    } else {
        voters.iter().find(|voter| *voter == who)?
    };
    Some(votebase::CitizenSubject {
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

fn all_joint_institutions() -> Vec<(votebase::InstitutionCode, Vec<u8>)> {
    CHINA_CB
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let code = if index == 0 {
                votebase::NRC
            } else {
                votebase::PRC
            };
            (code, entry.cid_number.as_bytes().to_vec())
        })
        .chain(
            CHINA_CH
                .iter()
                .map(|entry| (votebase::PRB, entry.cid_number.as_bytes().to_vec())),
        )
        .collect()
}

fn admins_for(institution_code: votebase::InstitutionCode, cid_number: &[u8]) -> Vec<AccountId32> {
    TestAdminProvider::institution_admins(institution_code, cid_number)
        .expect("joint institution should have admins")
}

fn create_joint_proposal() -> u64 {
    let data = b"joint-test".to_vec();
    let hash = <Test as frame_system::Config>::Hashing::hash(data.as_slice());
    let mut business_object_hash = [0u8; 32];
    business_object_hash.copy_from_slice(hash.as_ref());
    let proposer_subject =
        votebase::types::AuthorizationSubject::Institution(votebase::types::RoleSubject {
            cid_number: nrc_cid_number().try_into().expect("bounded NRC CID"),
            role_code: primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
                .to_vec()
                .try_into()
                .expect("bounded committee role"),
        });
    let voter_subjects = all_joint_institutions()
        .into_iter()
        .map(|(code, cid)| {
            let role_code = if matches!(code, votebase::NRC | votebase::PRC) {
                primitives::governance_skeleton::ROLE_CODE_COMMITTEE_MEMBER
            } else {
                primitives::governance_skeleton::ROLE_CODE_DIRECTOR
            };
            votebase::types::AuthorizationSubject::Institution(votebase::types::RoleSubject {
                cid_number: cid.try_into().expect("bounded joint CID"),
                role_code: role_code.to_vec().try_into().expect("bounded role code"),
            })
        })
        .collect();
    let module_tag: frame_support::BoundedVec<u8, ConstU32<32>> = b"joint-test"
        .to_vec()
        .try_into()
        .expect("bounded module tag");
    let vote_plan = votebase::types::VotePlanOf::try_new(
        votebase::types::BusinessActionId {
            module_tag: module_tag.clone(),
            action_code: 0,
        },
        module_tag,
        proposer_subject,
        voter_subjects,
        votebase::types::VotingEngineKind::Joint,
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

fn finalize_institution(
    proposal_id: u64,
    institution_code: votebase::InstitutionCode,
    cid_number: Vec<u8>,
    approve: bool,
) {
    let admins = admins_for(institution_code, &cid_number);
    let threshold = votebase::types::fixed_governance_pass_threshold(&institution_code)
        .expect("fixed institution threshold") as usize;
    let required = if approve {
        threshold
    } else {
        admins.len().saturating_sub(threshold).saturating_add(1)
    };
    let voter_role_code: votebase::RoleCode = if institution_code == votebase::PRB {
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

mod unit;
