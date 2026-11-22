//! 装配公权管理员测试Runtime、运行阶段和CID身份夹具。

#![cfg(test)]

use super::*;
use adminbase::{AdminAccountKind, InstitutionAdminQuery};
use frame_support::{
    assert_noop, assert_ok, derive_impl,
    traits::{ConstU32, ConstU64},
};
use frame_system as system;
use primitives::cid::code::code_bytes;
use primitives::count_const::NRC_ADMIN_COUNT;
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
    pub type VotingEngine = engingcore;

    #[runtime::pallet_index(2)]
    pub type PublicAdmins = super;

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

// 本模块验证业务状态转换；有界预算由engingcore专用测试与Runtime预算测试覆盖。
frame_support::parameter_types! { pub const LogicBudget: frame_support::weights::Weight = frame_support::weights::Weight::MAX; }

impl engingcore::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxVoteNonceLength = ConstU32<64>;
    type MaxVoteSignatureLength = ConstU32<64>;
    type MaxAutoFinalizePerBlock = ConstU32<64>;
    type MaxAutoFinalizeWeightPerBlock = LogicBudget;
    type MaxExecutionWeightPerBlock = LogicBudget;
    type MaxCleanupWeightPerBlock = LogicBudget;
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

pub struct TestCitizenIdentityBinding;

impl adminbase::CitizenIdentityBindingQuery<AccountId32> for TestCitizenIdentityBinding {
    fn current_account(cid_number: &[u8]) -> Option<AccountId32> {
        (cid_number == b"CN220-CTZN2-198805200-2026").then(|| CURRENT_ACCOUNT_ID_SEED.with(|seed| account(seed.get())))
    }

    fn matches_citizen_account(cid_number: &[u8], account: &AccountId32) -> bool {
        cid_number == b"CN220-CTZN2-198805200-2026"
            && CURRENT_ACCOUNT_ID_SEED.with(|seed| account == &self::account(seed.get()))
    }
}

thread_local! {
    /// 测试相位开关;默认 Genesis(false),每个测试经 new_test_ext 复位。
    static IS_OPERATION: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
    /// 指定测试公民 CID 当前链上绑定账户的 seed，用于模拟换绑前后的唯一授权账户。
    static CURRENT_ACCOUNT_ID_SEED: core::cell::Cell<u8> = const { core::cell::Cell::new(9) };
}

/// 可切换的相位 mock:Genesis(默认)放行、Operation 强制。
pub struct TestChainPhase;
impl adminbase::ChainPhaseCheck for TestChainPhase {
    fn is_operation() -> bool {
        IS_OPERATION.with(|cell| cell.get())
    }
}

fn set_operation_phase(is_operation: bool) {
    IS_OPERATION.with(|cell| cell.set(is_operation));
}

fn set_current_account_id_seed(seed: u8) {
    CURRENT_ACCOUNT_ID_SEED.with(|cell| cell.set(seed));
}

impl Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type MaxAdminsPerInstitution = ConstU32<1989>;
    type CitizenIdentityBinding = TestCitizenIdentityBinding;
    type ChainPhase = TestChainPhase;
}

fn new_test_ext() -> sp_io::TestExternalities {
    set_operation_phase(false); // 每个测试默认 Genesis 期。
    set_current_account_id_seed(9);
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
            family_name: Default::default(),
            given_name: Default::default(),
        })
        .collect()
}

fn indexed_admins(count: u32) -> Vec<Admin<AccountId32>> {
    (0..count)
        .map(|index| {
            let mut raw = [0u8; 32];
            raw[..4].copy_from_slice(&index.to_le_bytes());
            Admin {
                account_id: AccountId32::new(raw),
                cid_number: Default::default(),
                family_name: Default::default(),
                given_name: Default::default(),
            }
        })
        .collect()
}

mod unit;

mod security;

mod integration;

/// 钱包基础夹具使用现有CID生成器；初始化钱包显式提供，不从锚点或个人CID猜测。
fn wallet_fixture_cid(code: InstitutionCode, seed: u8) -> Vec<u8> {
    primitives::cid::generator::generate_cid_number(
        primitives::cid::generator::GenerateCidNumberInput {
            public_key: &format!("0x{seed:064x}"),
            p1: "0",
            province_code: "GZ",
            province_name: "贵州省",
            city_code: "018",
            city_name: "贵阳市",
            year: 2026,
            institution: primitives::cid::code::institution_code_text(&code).unwrap(),
        },
    )
    .unwrap()
    .into_bytes()
}

fn wallet_roster_fixture(code: InstitutionCode, cid: &[u8], count: u32) -> Vec<AccountId32> {
    let records = (0..count)
        .map(|index| {
            let mut bytes = [0u8; 32];
            bytes[..4].copy_from_slice(&index.to_le_bytes());
            Admin {
                account_id: AccountId32::new(bytes),
                cid_number: Default::default(),
                family_name: adminbase::FamilyName::truncate_from("测".as_bytes().to_vec()),
                given_name: adminbase::GivenName::truncate_from("试".as_bytes().to_vec()),
            }
        })
        .collect::<Vec<_>>();
    let anchors = records
        .iter()
        .map(|record| record.account_id.clone())
        .collect();
    // Pre-upgrade roster: exercise explicit initialization independently of the new setter.
    AdminAccounts::<Test>::insert(AdminCidNumber::try_from(cid.to_vec()).unwrap(),
        InstitutionAdmins { institution_code: code, admins: records.try_into().unwrap() });
    anchors
}

fn initialize_wallet_roster(code: InstitutionCode, cid: &[u8], anchors: &[AccountId32]) {
    for anchor in anchors {
        assert_ok!(<PublicAdmins as InstitutionAdminWalletLifecycle<
            AccountId32,
        >>::initialize_institution_wallet(
            code,
            cid.to_vec(),
            anchor.clone(),
            anchor.clone(),
            None,
        ));
    }
}

fn wallet_change(
    anchor: &AccountId32,
    old: AccountId32,
    new: AccountId32,
    revision: u64,
) -> InstitutionAdminWalletChange<AccountId32> {
    InstitutionAdminWalletChange {
        admin_account: anchor.clone(),
        expected_wallet: old,
        new_wallet: new,
        expected_revision: revision,
    }
}
