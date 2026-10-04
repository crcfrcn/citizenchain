#![cfg(test)]

use super::pallet::*;
use codec::Decode;
use frame_support::{
    derive_impl,
    traits::{Get, OnFinalize, OnInitialize, VariantCountOf},
};
use frame_system as system;
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};
use std::{cell::RefCell, thread_local};

type Block = frame_system::mocking::MockBlock<Test>;
type Balance = u128;

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
    pub type ProvincialBankInterest = super;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
    type Block = Block;
    type AccountId = AccountId32;
    type AccountData = pallet_balances::AccountData<Balance>;
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

thread_local! {
    static BLOCKS_PER_YEAR_FOR_TEST: RefCell<u64> = const { RefCell::new(10) };
}

pub struct BlocksPerYearForTest;
impl Get<u64> for BlocksPerYearForTest {
    fn get() -> u64 {
        BLOCKS_PER_YEAR_FOR_TEST.with(|v| *v.borrow())
    }
}

impl Config for Test {
    type Currency = Balances;
    type BlocksPerYear = BlocksPerYearForTest;
    type WeightInfo = ();
}

fn set_blocks_per_year(v: u64) {
    BLOCKS_PER_YEAR_FOR_TEST.with(|p| *p.borrow_mut() = v);
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    set_blocks_per_year(10);
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("frame system genesis storage should build");
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| System::set_block_number(0));
    ext
}

fn run_to_block(n: u64) {
    while System::block_number() < n {
        let block = System::block_number() + 1;
        System::set_block_number(block);
        System::on_initialize(block);
        ProvincialBankInterest::on_initialize(block);
        ProvincialBankInterest::on_finalize(block);
        System::on_finalize(block);
    }
}

fn provincialbank_account(index: usize) -> AccountId32 {
    AccountId32::decode(&mut &primitives::cid::china::china_ch::CHINA_CH[index].main_account[..])
        .expect("main_account must decode")
}

mod cases;
