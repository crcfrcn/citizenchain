#![cfg(test)]

use super::*;
use frame_support::{
    assert_ok, derive_impl,
    traits::{ConstU32, Hooks, UnixTime},
    BoundedVec,
};
use frame_system as system;
use sp_core::{sr25519, Pair as PairT};
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};
use votebase::types::{InstitutionCode, PMUL};

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
    pub type InternalVote = internalvote;

    #[runtime::pallet_index(3)]
    pub type PersonalAdmins = super;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
    type Block = Block;
    type AccountId = AccountId32;
    type Lookup = IdentityLookup<Self::AccountId>;
}

pub struct TestCitizenIdentityReader;
impl votebase::CitizenIdentityReader<AccountId32> for TestCitizenIdentityReader {
    fn voting_subject(
        who: &AccountId32,
        _scope: &votebase::PopulationScope,
    ) -> Option<votebase::CitizenSubject<AccountId32>> {
        Some(test_citizen_subject(who))
    }

    fn candidate_subject(
        who: &AccountId32,
        _scope: &votebase::PopulationScope,
    ) -> Option<votebase::CitizenSubject<AccountId32>> {
        Some(test_citizen_subject(who))
    }
}

fn test_citizen_subject(who: &AccountId32) -> votebase::CitizenSubject<AccountId32> {
    votebase::CitizenSubject {
        cid_number: <AccountId32 as AsRef<[u8]>>::as_ref(who)
            .to_vec()
            .try_into()
            .expect("account fits CID"),
        account_id: who.clone(),
    }
}

pub struct TestInternalAdminProvider;
impl votebase::InternalAdminProvider<AccountId32> for TestInternalAdminProvider {
    fn is_institution_admin(
        _institution_code: InstitutionCode,
        _cid_number: &[u8],
        _who: &AccountId32,
    ) -> bool {
        false
    }

    fn is_personal_admin(personal_account_id: AccountId32, who: &AccountId32) -> bool {
        PersonalAdmins::is_active_account_admin(PMUL, personal_account_id, who)
    }

    fn get_personal_admins(personal_account_id: AccountId32) -> Option<Vec<AccountId32>> {
        PersonalAdmins::active_account_admins(PMUL, personal_account_id)
    }

    fn is_pending_personal_admin(personal_account_id: AccountId32, who: &AccountId32) -> bool {
        PersonalAdmins::is_pending_account_admin_for_snapshot(PMUL, personal_account_id, who)
    }

    fn get_pending_personal_admins(personal_account_id: AccountId32) -> Option<Vec<AccountId32>> {
        PersonalAdmins::pending_account_admins_for_snapshot(PMUL, personal_account_id)
    }
}

pub struct TestTimeProvider;
impl UnixTime for TestTimeProvider {
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
    type CleanupKeysPerStep = ConstU32<64>;
    type CitizenIdentityReader = TestCitizenIdentityReader;
    type JointVoteResultCallback = ();
    type InternalVoteResultCallback = crate::InternalVoteExecutor<Test>;
    type InternalAdminProvider = TestInternalAdminProvider;
    type MaxAdminsPerInstitution = ConstU32<1989>;
    // 与正式Runtime相同，覆盖64人管理员动作的真实容量。
    type MaxProposalDataLen = ConstU32<{ 100 * 1024 }>;
    type MaxProposalObjectLen = ConstU32<{ 10 * 1024 }>;
    type MaxModuleTagLen = ConstU32<32>;
    type MaxManualExecutionAttempts = ConstU32<3>;
    type ExecutionRetryGraceBlocks = frame_support::traits::ConstU64<216>;
    type MaxExecutionRetryDeadlinesPerBlock = ConstU32<128>;
    type MaxCleanupActivationsPerBlock = ConstU32<50>;
    type MaxPendingRetryExpirationsPerBlock = ConstU32<16>;
    type TimeProvider = TestTimeProvider;
    type WeightInfo = ();
    type TrackHandlers = (InternalVote, ());
    type LegislationVoteResultCallback = ();
    type ElectionVoteResultCallback = ();
}

impl internalvote::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type InstitutionRoleProvider = ();
    type WeightInfo = ();
}

impl pallet::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type InternalVoteEngine = internalvote::Pallet<Test>;
    type MaxPersonalAccountAdmins = ConstU32<64>;
    type WeightInfo = ();
}

fn admin(index: u8) -> AccountId32 {
    let mut seed = [0u8; 32];
    seed[0] = 0x71;
    seed[1] = index;
    AccountId32::new(sr25519::Pair::from_seed(&seed).public().0)
}

fn personal_account_id() -> AccountId32 {
    AccountId32::new([0x51; 32])
}

fn admins(items: &[AccountId32]) -> pallet::AdminsOf<Test> {
    BoundedVec::try_from(
        items
            .iter()
            .cloned()
            .map(|account_id| adminbase::Admin {
                account_id,
                cid_number: Default::default(),
                family_name: "管理".as_bytes().to_vec().try_into().expect("name fits"),
                given_name: "员".as_bytes().to_vec().try_into().expect("name fits"),
            })
            .collect::<Vec<_>>(),
    )
    .expect("admins fit")
}

fn seed_active_admin_account(account: AccountId32, admins: pallet::AdminsOf<Test>, threshold: u32) {
    internalvote::ActivePersonalThresholds::<Test>::insert(account.clone(), threshold);
    pallet::AdminAccounts::<Test>::insert(
        account.clone(),
        adminbase::AdminAccount {
            cid_number: Default::default(),
            institution_code: PMUL,
            kind: adminbase::AdminAccountKind::PersonalMultisig,
            admins,
            creator_account_id: admin(0),
            created_at: 1,
            updated_at: 1,
            status: adminbase::AdminAccountStatus::Active,
        },
    );
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("test storage should build");
    let mut ext: sp_io::TestExternalities = storage.into();
    ext.execute_with(|| System::set_block_number(1));
    ext
}

mod unit;

mod security;

mod integration;

mod recovery;
