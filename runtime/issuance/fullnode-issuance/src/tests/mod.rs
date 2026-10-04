#![cfg(test)]

use super::pallet::*;
use frame_support::{
    assert_noop, assert_ok, derive_impl,
    traits::{Hooks, VariantCountOf},
    weights::Weight,
};
use frame_system as system;
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};
use std::{cell::RefCell, thread_local};

type Block = frame_system::mocking::MockBlockU32<Test>;
type Balance = u128;

thread_local! {
    static MOCK_AUTHOR: RefCell<Option<AccountId32>> = const { RefCell::new(None) };
}

pub struct MockFindAuthor;

impl frame_support::traits::FindAuthor<AccountId32> for MockFindAuthor {
    fn find_author<'a, I>(_digests: I) -> Option<AccountId32>
    where
        I: 'a + IntoIterator<Item = (sp_runtime::ConsensusEngineId, &'a [u8])>,
    {
        MOCK_AUTHOR.with(|v| v.borrow().clone())
    }
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
    pub type FullnodeIssuance = super;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
    type Block = Block;
    type AccountId = AccountId32;
    type AccountData = pallet_balances::AccountData<Balance>;
    type DbWeight = frame_support::weights::constants::RocksDbWeight;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Nonce = u64;
}

impl pallet_balances::Config for Test {
    type MaxLocks = frame_support::traits::ConstU32<0>;
    type MaxReserves = frame_support::traits::ConstU32<0>;
    type ReserveIdentifier = [u8; 8];
    type Balance = Balance;
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = ();
    type ExistentialDeposit = frame_support::traits::ConstU128<1>;
    type AccountStore = System;
    type WeightInfo = ();
    type FreezeIdentifier = RuntimeFreezeReason;
    type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type DoneSlashHandler = ();
}

impl Config for Test {
    type Currency = Balances;
    type FindAuthor = MockFindAuthor;
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

fn account(n: u8) -> AccountId32 {
    AccountId32::new([n; 32])
}

mod cases;
