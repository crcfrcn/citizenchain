#![cfg(test)]

use super::*;
use frame_support::{
    derive_impl,
    traits::{Hooks, Time},
};
use frame_system as system;
use primitives::pow_const::{
    DIFFICULTY_ADJUSTMENT_INTERVAL, DIFFICULTY_MAX_ADJUST_FACTOR, DIFFICULTY_MIN_ADJUST_FACTOR,
    DIFFICULTY_TARGET_WINDOW_MS, POW_INITIAL_DIFFICULTY, POW_TARGET_BLOCK_TIME_MS,
};
use sp_runtime::{traits::IdentityLookup, BuildStorage};
use std::panic::{catch_unwind, AssertUnwindSafe};

type Block = frame_system::mocking::MockBlock<Test>;
const FIRST_ADJUST_BLOCK: u64 = DIFFICULTY_ADJUSTMENT_INTERVAL as u64 + 1;
const SECOND_ADJUST_BLOCK: u64 = FIRST_ADJUST_BLOCK + DIFFICULTY_ADJUSTMENT_INTERVAL as u64;

#[frame_support::runtime]
mod runtime {
    #[runtime::runtime]
    #[runtime::derive(
        RuntimeCall,
        RuntimeEvent,
        RuntimeError,
        RuntimeOrigin,
        RuntimeTask,
        RuntimeViewFunction
    )]
    pub struct Test;

    #[runtime::pallet_index(0)]
    pub type System = frame_system;
    #[runtime::pallet_index(1)]
    pub type Timestamp = pallet_timestamp;
    #[runtime::pallet_index(2)]
    pub type PowDifficulty = super;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for Test {
    type Block = Block;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
}

impl pallet_timestamp::Config for Test {
    type Moment = u64;
    type OnTimestampSet = ();
    type MinimumPeriod = frame_support::traits::ConstU64<1>;
    type WeightInfo = ();
}

impl Config for Test {
    type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("frame system genesis storage should build");
    sp_io::TestExternalities::new(storage)
}

fn run_blocks(count: u32, block_time_ms: u64) {
    for _ in 0..count {
        let block = System::block_number() + 1;
        let now_ms = Timestamp::now().saturating_add(block_time_ms);
        System::set_block_number(block);
        Timestamp::set_timestamp(now_ms);
        set_extrinsic_count(2);
        PowDifficulty::on_finalize(block);
    }
}

/// 模拟 Executive 在进入 on_finalize 前记录的区块 extrinsic 数量。
fn set_extrinsic_count(count: u32) {
    for _ in 0..count {
        System::note_applied_extrinsic(&Ok(().into()), Default::default());
    }
    System::note_finished_extrinsics();
}

fn difficulty_adjusted_events() -> Vec<Event<Test>> {
    System::events()
        .into_iter()
        .filter_map(|r| match r.event {
            RuntimeEvent::PowDifficulty(event) => Some(event),
            _ => None,
        })
        .collect()
}

mod unit;
