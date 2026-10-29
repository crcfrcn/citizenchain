#![cfg(test)]

use super::*;
use crate::pallet::{CreatorPlans, CreatorTierNames, PlatformPrice, RenewalIndex, Subscriptions};
use frame_support::{
    assert_noop, assert_ok, derive_impl,
    traits::{ConstU128, ConstU32, Hooks, UnixTime},
};
use frame_system as system;
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};
use std::cell::RefCell;

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
    pub type SquarePost = super;
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
    type ExistentialDeposit = ConstU128<1>;
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

thread_local! {
    static NOW_MS: RefCell<u64> = const { RefCell::new(1_700_000_000_000) };
    /// 测试中的当前 CID↔账户双向绑定；重绑时同一 CID 只保留一个当前账户。
    static IDENTITY_BINDINGS: RefCell<Vec<(Vec<u8>, AccountId32)>> = const { RefCell::new(Vec::new()) };
}

pub struct MockTime;
impl UnixTime for MockTime {
    fn now() -> core::time::Duration {
        core::time::Duration::from_millis(NOW_MS.with(|value| *value.borrow()))
    }
}

fn set_now(value: u64) {
    NOW_MS.with(|now| *now.borrow_mut() = value);
}

fn initialize_at(value: u64) {
    set_now(value);
    // 续费在 on_initialize 处理，余额变化必须落在 NodeGuard 的 finalize 前状态。
    <SquarePost as Hooks<u64>>::on_initialize(System::block_number());
}

pub struct TestCitizenIdentity;
impl SquarePostCitizenIdentityProvider<AccountId32> for TestCitizenIdentity {
    fn active_cid_number(account_id: &AccountId32) -> Option<Vec<u8>> {
        IDENTITY_BINDINGS.with(|bindings| {
            bindings
                .borrow()
                .iter()
                .find(|(_, current_account_id)| current_account_id == account_id)
                .map(|(cid_number, _)| cid_number.clone())
        })
    }

    fn current_account_id(cid_number: &[u8]) -> Option<AccountId32> {
        IDENTITY_BINDINGS.with(|bindings| {
            bindings
                .borrow()
                .iter()
                .find(|(current_cid_number, _)| current_cid_number.as_slice() == cid_number)
                .map(|(_, account_id)| account_id.clone())
        })
    }

    fn is_campaign_eligible(cid_number: &[u8], account_id: &AccountId32) -> bool {
        cid_number == VERIFIED_CID && account_id == &verified_account()
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn benchmark_seed_identity(account_id: &AccountId32) -> Vec<u8> {
        let cid_number = b"GD001-CTZN1-999999999-2026".to_vec();
        bind_identity(&cid_number, account_id.clone());
        cid_number
    }
}

pub struct MockInstitutionQuery;
impl entitybase::InstitutionMultisigQuery<AccountId32> for MockInstitutionQuery {
    fn lookup_institution_account(cid_number: &[u8], account_name: &[u8]) -> Option<AccountId32> {
        (cid_number == PLATFORM_CID
            && account_name == primitives::account_derive::RESERVED_NAME_FEE)
            .then(platform_fee_account)
    }

    fn lookup_cid(_addr: &AccountId32) -> Option<Vec<u8>> {
        None
    }
    fn lookup_org(_addr: &AccountId32) -> Option<primitives::cid::code::InstitutionCode> {
        None
    }
    fn lookup_admin_config(
        _addr: &AccountId32,
    ) -> Option<primitives::multisig::MultisigConfigSnapshot<AccountId32>> {
        None
    }
    fn account_exists(_addr: &AccountId32) -> bool {
        false
    }
}

impl crate::pallet::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type CitizenIdentity = TestCitizenIdentity;
    type Currency = Balances;
    type TimeProvider = MockTime;
    type InstitutionAccountQuery = MockInstitutionQuery;
    type InternalVoteEngine = ();
    type InstitutionRoleAuthorization = ();
    type MaxSquarePostIdLen = ConstU32<64>;
    type MaxSquareCidNumberLen = ConstU32<32>;
    type MaxSquareStorageReceiptIdLen = ConstU32<96>;
    type MaxSubscriptionRenewalsPerBlock = ConstU32<64>;
    type WeightInfo = ();
}

// 平台机构永久固定为公民链基金会，测试与 pallet 共用同一创世常量 CID。
const PLATFORM_CID: &[u8] = primitives::cid::china::citizenchain::CITIZENCHAIN_FOUNDATION
    .cid_number
    .as_bytes();
const PLATFORM_PRICE: u128 = 5_999_900;
const VERIFIED_CID: &[u8] = b"GD001-CTZN1-000000001-2026";
const VISITOR_CID: &[u8] = b"GD001-CTZN1-000000002-2026";
const SUBSCRIBER_CID: &[u8] = b"GD001-CTZN1-000000003-2026";
const POOR_CID: &[u8] = b"GD001-CTZN1-000000005-2026";
const CREATOR_CID: &[u8] = b"GD001-CTZN1-000000006-2026";

fn account(byte: u8) -> AccountId32 {
    AccountId32::new([byte; 32])
}
fn verified_account() -> AccountId32 {
    account(1)
}
fn visitor_account() -> AccountId32 {
    account(2)
}
fn subscriber_account_id() -> AccountId32 {
    account(3)
}
fn poor_account() -> AccountId32 {
    account(5)
}
fn creator_account_id() -> AccountId32 {
    account(6)
}
fn rebound_subscriber_account_id() -> AccountId32 {
    account(7)
}
fn rebound_creator_account_id() -> AccountId32 {
    account(8)
}
fn platform_fee_account() -> AccountId32 {
    account(9)
}

fn bounded_cid_number(value: &[u8]) -> crate::pallet::CidNumberOf<Test> {
    value.to_vec().try_into().expect("test CID fits")
}

fn subscriber_cid_number() -> crate::pallet::CidNumberOf<Test> {
    bounded_cid_number(SUBSCRIBER_CID)
}

fn creator_cid_number() -> crate::pallet::CidNumberOf<Test> {
    bounded_cid_number(CREATOR_CID)
}

fn bind_identity(cid_number: &[u8], account_id: AccountId32) {
    IDENTITY_BINDINGS.with(|bindings| {
        let mut bindings = bindings.borrow_mut();
        bindings.retain(|(current_cid_number, current_account_id)| {
            current_cid_number.as_slice() != cid_number && current_account_id != &account_id
        });
        bindings.push((cid_number.to_vec(), account_id));
    });
}

fn unbind_identity(cid_number: &[u8]) {
    IDENTITY_BINDINGS.with(|bindings| {
        bindings
            .borrow_mut()
            .retain(|(current_cid_number, _)| current_cid_number.as_slice() != cid_number);
    });
}

fn reset_identity_bindings() {
    IDENTITY_BINDINGS.with(|bindings| bindings.borrow_mut().clear());
    bind_identity(VERIFIED_CID, verified_account());
    bind_identity(VISITOR_CID, visitor_account());
    bind_identity(SUBSCRIBER_CID, subscriber_account_id());
    bind_identity(POOR_CID, poor_account());
    bind_identity(CREATOR_CID, creator_account_id());
}

fn platform_plan() -> SubscriptionPlan {
    platform_plan_for(MembershipLevel::Spark)
}

fn platform_plan_for(membership_level: MembershipLevel) -> SubscriptionPlan {
    SubscriptionPlan::Platform { membership_level }
}

fn creator_plan() -> SubscriptionPlan {
    SubscriptionPlan::Creator {
        tier_id: TierId::try_from(b"supporter".to_vec()).expect("tier id fits"),
        billing_period: BillingPeriod::Monthly,
    }
}

fn creator_tier(price_fen: u128) -> CreatorTier {
    CreatorTier {
        tier_id: TierId::try_from(b"supporter".to_vec()).expect("tier id fits"),
        prices_fen: PeriodPrices::try_from(vec![PeriodPrice {
            billing_period: BillingPeriod::Monthly,
            price_fen,
        }])
        .expect("period fits"),
    }
}

fn tier_name(value: &str) -> TierName {
    TierName::try_from(value.as_bytes().to_vec()).expect("tier name fits byte bound")
}

fn creator_tier_input(price_fen: u128, name: &str) -> CreatorTierInput {
    CreatorTierInput {
        tier_id: TierId::try_from(b"supporter".to_vec()).expect("tier id fits"),
        tier_name: tier_name(name),
        prices_fen: PeriodPrices::try_from(vec![PeriodPrice {
            billing_period: BillingPeriod::Monthly,
            price_fen,
        }])
        .expect("period fits"),
    }
}

fn setup_platform() {
    // 平台 CID 已是创世常量，无需绑定；仅播种测试所需三档价。
    PlatformPrice::<Test>::insert(MembershipLevel::Spark, PLATFORM_PRICE);
    PlatformPrice::<Test>::insert(MembershipLevel::Freedom, 199_900);
}

fn set_active_platform_member(account: AccountId32) {
    let cid_number =
        TestCitizenIdentity::active_cid_number(&account).expect("member identity is bound");
    Subscriptions::<Test>::insert(
        (
            bounded_cid_number(cid_number.as_slice()),
            IssuerKey::Platform,
        ),
        SubscriptionState {
            plan: platform_plan_for(MembershipLevel::Freedom),
            started_at: 1_699_000_000_000,
            last_charged_at: 1_699_000_000_000,
            charge_fen: 1,
            paid_until: 1_800_000_000_000,
            subscription_status: SubscriptionStatus::Active,
            price_fen: 1,
            suspend_reason: None,
        },
    );
}

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("test storage should build");
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![
            (subscriber_account_id(), 1_000_000_000),
            (poor_account(), 100),
            (creator_account_id(), 1_000_000_000),
            (rebound_subscriber_account_id(), 1_000_000_000),
        ],
        ..Default::default()
    }
    .assimilate_storage(&mut storage)
    .expect("balances genesis should build");
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);
        set_now(1_700_000_000_000);
        reset_identity_bindings();
    });
    ext
}

mod post;
mod subscription;
