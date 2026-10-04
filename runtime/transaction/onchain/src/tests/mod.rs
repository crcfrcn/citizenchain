#![cfg(test)]

use super::*;
use frame_support::{
    assert_noop, assert_ok, derive_impl, dispatch::GetDispatchInfo, parameter_types,
    traits::VariantCountOf, weights::ConstantMultiplier,
};
use frame_system as system;
use pallet_transaction_payment::OnChargeTransaction;
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};
use std::{cell::RefCell, thread_local};

type Block = frame_system::mocking::MockBlockU32<Test>;
type Balance = u128;

thread_local! {
    static MOCK_AUTHOR: RefCell<Option<AccountId32>> = const { RefCell::new(None) };
}

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
    pub type TransactionPayment = pallet_transaction_payment;
    #[runtime::pallet_index(3)]
    pub type FullnodeIssuance = fullnode_issuance;
    #[runtime::pallet_index(4)]
    pub type OnchainTransaction = crate::pallet;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
    type Block = Block;
    type AccountId = AccountId32;
    type AccountData = pallet_balances::AccountData<Balance>;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Nonce = u64;
}

parameter_types! {
    pub static TestExistentialDeposit: Balance = 1;
}

impl pallet_balances::Config for Test {
    type MaxLocks = frame_support::traits::ConstU32<0>;
    type MaxReserves = frame_support::traits::ConstU32<0>;
    type ReserveIdentifier = [u8; 8];
    type Balance = Balance;
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = ();
    type ExistentialDeposit = TestExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
    type FreezeIdentifier = RuntimeFreezeReason;
    type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type DoneSlashHandler = ();
}

impl pallet_transaction_payment::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type OnChargeTransaction = pallet_transaction_payment::FungibleAdapter<Balances, ()>;
    // 费用只允许来自 FeeRoute 五类协议，测试环境同样禁止产生 FRAME weight/length 费用。
    type WeightToFee = ConstantMultiplier<Balance, frame_support::traits::ConstU128<0>>;
    type LengthToFee = ConstantMultiplier<Balance, frame_support::traits::ConstU128<0>>;
    type FeeMultiplierUpdate = ();
    type OperationalFeeMultiplier = frame_support::traits::ConstU8<1>;
    type WeightInfo = ();
}

pub struct MockFindAuthor;
impl FindAuthor<AccountId32> for MockFindAuthor {
    fn find_author<'a, I>(_digests: I) -> Option<AccountId32>
    where
        I: 'a + IntoIterator<Item = (sp_runtime::ConsensusEngineId, &'a [u8])>,
    {
        MOCK_AUTHOR.with(|v| v.borrow().clone())
    }
}

impl fullnode_issuance::Config for Test {
    type Currency = Balances;
    type FindAuthor = MockFindAuthor;
    type WeightInfo = ();
}

impl crate::pallet::Config for Test {
    type Currency = Balances;
    type MaxTransferRemarkLen = frame_support::traits::ConstU32<99>;
    // 与 runtime 同口径：只转发 `primitives::fee_policy`，测试也不另立数字。
    type OnchainMinFee =
        frame_support::traits::ConstU128<{ primitives::fee_policy::ONCHAIN_MIN_FEE }>;
    type OnchainFeeRate = TestOnchainFeeRate;
    type VoteFlatFee = frame_support::traits::ConstU128<{ primitives::fee_policy::VOTE_FLAT_FEE }>;
}

frame_support::parameter_types! {
    pub const TestOnchainFeeRate: sp_runtime::Perbill = primitives::fee_policy::ONCHAIN_FEE_RATE;
}

struct MockNrcAccountProvider;
impl NrcAccountProvider<AccountId32> for MockNrcAccountProvider {
    fn nrc_account() -> Option<AccountId32> {
        Some(AccountId32::new(
            primitives::cid::china::china_cb::CHINA_CB[0].main_account,
        ))
    }
}

struct MockNrcAccountProviderNone;
impl NrcAccountProvider<AccountId32> for MockNrcAccountProviderNone {
    fn nrc_account() -> Option<AccountId32> {
        None
    }
}

struct MockSafetyFundAccountProvider;
impl SafetyFundAccountProvider<AccountId32> for MockSafetyFundAccountProvider {
    fn safety_fund_account() -> AccountId32 {
        AccountId32::new(primitives::cid::china::china_cb::SAFETY_FUND_ACCOUNT)
    }
}

fn account(n: u8) -> AccountId32 {
    AccountId32::new([n; 32])
}

fn new_test_ext() -> sp_io::TestExternalities {
    TestExistentialDeposit::set(1);
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("system genesis build should succeed");
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(account(1), 1_000), (account(2), 1_000), (account(3), 3)],
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .expect("balances genesis build should succeed");
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

struct FeeRouteOnchain;
impl CallFeeRoute<AccountId32, RuntimeCall, Balance> for FeeRouteOnchain {
    fn fee_route(
        who: &AccountId32,
        _call: &RuntimeCall,
    ) -> primitives::fee_policy::FeeRoute<AccountId32, Balance> {
        primitives::fee_policy::FeeRoute::Onchain {
            transaction_amount: 50_000,
            payer_account_id: who.clone(),
        }
    }
}

struct FeeRouteVote;
impl CallFeeRoute<AccountId32, RuntimeCall, Balance> for FeeRouteVote {
    fn fee_route(
        who: &AccountId32,
        _call: &RuntimeCall,
    ) -> primitives::fee_policy::FeeRoute<AccountId32, Balance> {
        primitives::fee_policy::FeeRoute::Vote {
            payer_account_id: who.clone(),
        }
    }
}

struct FeeRouteOffchain;
impl CallFeeRoute<AccountId32, RuntimeCall, Balance> for FeeRouteOffchain {
    fn fee_route(
        _who: &AccountId32,
        _call: &RuntimeCall,
    ) -> primitives::fee_policy::FeeRoute<AccountId32, Balance> {
        primitives::fee_policy::FeeRoute::Offchain {
            fee_amount: 88,
            payer_account_id: primitives::fee_policy::OffchainFeePayer::BatchItemPayers,
        }
    }
}

struct FeeRouteFree;
impl CallFeeRoute<AccountId32, RuntimeCall, Balance> for FeeRouteFree {
    fn fee_route(
        _who: &AccountId32,
        _call: &RuntimeCall,
    ) -> primitives::fee_policy::FeeRoute<AccountId32, Balance> {
        primitives::fee_policy::FeeRoute::Free
    }
}

struct FeeRouteReject;
impl CallFeeRoute<AccountId32, RuntimeCall, Balance> for FeeRouteReject {
    fn fee_route(
        _who: &AccountId32,
        _call: &RuntimeCall,
    ) -> primitives::fee_policy::FeeRoute<AccountId32, Balance> {
        primitives::fee_policy::FeeRoute::Reject
    }
}

struct FeeRouteTinyOnchain;
impl CallFeeRoute<AccountId32, RuntimeCall, Balance> for FeeRouteTinyOnchain {
    fn fee_route(
        who: &AccountId32,
        _call: &RuntimeCall,
    ) -> primitives::fee_policy::FeeRoute<AccountId32, Balance> {
        primitives::fee_policy::FeeRoute::Onchain {
            transaction_amount: 1,
            payer_account_id: who.clone(),
        }
    }
}

struct FeeRouteTinyAccount2;
impl CallFeeRoute<AccountId32, RuntimeCall, Balance> for FeeRouteTinyAccount2 {
    fn fee_route(
        _who: &AccountId32,
        _call: &RuntimeCall,
    ) -> primitives::fee_policy::FeeRoute<AccountId32, Balance> {
        primitives::fee_policy::FeeRoute::Onchain {
            transaction_amount: 1,
            payer_account_id: account(2),
        }
    }
}

fn sample_call() -> RuntimeCall {
    RuntimeCall::System(frame_system::Call::remark {
        remark: vec![1, 2, 3],
    })
}

fn has_fee_share_burn_event(reason: pallet::BurnReason, amount: u128) -> bool {
    fee_share_burn_event_count(reason, amount) > 0
}

fn fee_share_burn_event_count(reason: pallet::BurnReason, amount: u128) -> usize {
    System::events()
        .iter()
        .filter(|r| {
            matches!(
                &r.event,
                RuntimeEvent::OnchainTransaction(pallet::Event::FeeShareBurnt {
                    reason: event_reason,
                    amount: event_amount,
                }) if *event_reason == reason && *event_amount == amount
            )
        })
        .count()
}

fn fee_share_burn_event_total() -> usize {
    System::events()
        .iter()
        .filter(|r| {
            matches!(
                &r.event,
                RuntimeEvent::OnchainTransaction(pallet::Event::FeeShareBurnt { .. })
            )
        })
        .count()
}

fn has_fee_paid_event() -> bool {
    System::events().iter().any(|r| {
        matches!(
            r.event,
            RuntimeEvent::OnchainTransaction(pallet::Event::FeePaid { .. })
        )
    })
}

mod cases;
