#![cfg(test)]

use super::*;
use adminbase::{AdminAccountKind, InstitutionAdminQuery};
use frame_support::{
    assert_noop, assert_ok, derive_impl,
    traits::{ConstU32, ConstU64},
};
use frame_system as system;
use primitives::cid::code::code_bytes;
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};

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
    pub type PrivateAdmins = super;

    #[runtime::pallet_index(3)]
    pub type InternalVote = internalvote;
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
    type MaxCleanupActivationsPerBlock = ConstU32<50>;
    type CleanupKeysPerStep = ConstU32<64>;
    type MaxProposalDataLen = ConstU32<{ 8 * 1024 }>;
    type MaxProposalObjectLen = ConstU32<{ 10 * 1024 }>;
    type MaxModuleTagLen = ConstU32<32>;
    type MaxManualExecutionAttempts = ConstU32<3>;
    type ExecutionRetryGraceBlocks = ConstU64<216>;
    type MaxExecutionRetryDeadlinesPerBlock = ConstU32<128>;
    type MaxPendingRetryExpirationsPerBlock = ConstU32<16>;
    type CitizenIdentityReader = ();
    type JointVoteResultCallback = ();
    type InternalVoteResultCallback = ();
    type InternalAdminProvider = ();
    type MaxAdminsPerInstitution = ConstU32<1989>;
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

impl Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxAdminsPerInstitution = ConstU32<1989>;
    type ChainPhase = (); // 恒 Genesis（测试默认）
    type CitizenIdentityBinding = (); // 恒不绑定（() 实现返回 false）
    type LegalRepresentativeQuery = (); // 恒无 LR（() 实现返回 None）
}

fn new_test_ext() -> sp_io::TestExternalities {
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("test storage should build");
    let mut ext: sp_io::TestExternalities = storage.into();
    ext.execute_with(|| System::set_block_number(1));
    ext
}

fn account(seed: u8) -> AccountId32 {
    AccountId32::new([seed; 32])
}

/// admins 保存显示姓名和授权账户；岗位、任期、来源等由 entity 管理。
fn admins(count: u8) -> Vec<Admin<AccountId32>> {
    (0..count)
        .map(|seed| Admin {
            account_id: account(seed),
            cid_number: Default::default(),
            family_name: "管理".as_bytes().to_vec().try_into().expect("name fits"),
            given_name: "员".as_bytes().to_vec().try_into().expect("name fits"),
        })
        .collect()
}

mod unit;

mod security;

mod integration;
