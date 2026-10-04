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

#[test]
fn first_adjustment_happens_at_interval_plus_one_and_window_is_exact() {
    new_test_ext().execute_with(|| {
        run_blocks(DIFFICULTY_ADJUSTMENT_INTERVAL, POW_TARGET_BLOCK_TIME_MS);
        assert_eq!(PowDifficulty::current_difficulty(), POW_INITIAL_DIFFICULTY);
        assert!(difficulty_adjusted_events().is_empty());

        run_blocks(1, POW_TARGET_BLOCK_TIME_MS);
        assert_eq!(PowDifficulty::current_difficulty(), POW_INITIAL_DIFFICULTY);

        System::assert_last_event(RuntimeEvent::PowDifficulty(Event::DifficultyAdjusted {
            block: FIRST_ADJUST_BLOCK,
            old_difficulty: POW_INITIAL_DIFFICULTY,
            new_difficulty: POW_INITIAL_DIFFICULTY,
            actual_window_ms: DIFFICULTY_TARGET_WINDOW_MS,
            target_window_ms: DIFFICULTY_TARGET_WINDOW_MS,
        }));

        assert_eq!(WindowStartMs::<Test>::get(), Some(Timestamp::now()));
    });
}

#[test]
fn raises_difficulty_when_blocks_are_too_fast() {
    new_test_ext().execute_with(|| {
        run_blocks(
            DIFFICULTY_ADJUSTMENT_INTERVAL + 1,
            POW_TARGET_BLOCK_TIME_MS / 2,
        );
        assert_eq!(
            PowDifficulty::current_difficulty(),
            POW_INITIAL_DIFFICULTY * 2
        );
    });
}

#[test]
fn lowers_difficulty_when_blocks_are_too_slow() {
    new_test_ext().execute_with(|| {
        run_blocks(
            DIFFICULTY_ADJUSTMENT_INTERVAL + 1,
            POW_TARGET_BLOCK_TIME_MS * 2,
        );
        assert_eq!(
            PowDifficulty::current_difficulty(),
            POW_INITIAL_DIFFICULTY / 2
        );
    });
}

#[test]
fn clamps_to_adjustment_bounds() {
    new_test_ext().execute_with(|| {
        let old = 100u64;
        CurrentDifficulty::<Test>::put(old);
        WindowStartMs::<Test>::put(999);
        WindowStartBlock::<Test>::put(1);
        System::set_block_number(FIRST_ADJUST_BLOCK);
        Timestamp::set_timestamp(1_000);
        set_extrinsic_count(2);
        PowDifficulty::on_finalize(FIRST_ADJUST_BLOCK);
        assert_eq!(
            PowDifficulty::current_difficulty(),
            old * DIFFICULTY_MAX_ADJUST_FACTOR
        );

        CurrentDifficulty::<Test>::put(old);
        WindowStartMs::<Test>::put(0);
        WindowStartBlock::<Test>::put(FIRST_ADJUST_BLOCK as u32);
        System::set_block_number(SECOND_ADJUST_BLOCK);
        Timestamp::set_timestamp(1_000_000_000);
        set_extrinsic_count(2);
        PowDifficulty::on_finalize(SECOND_ADJUST_BLOCK);
        assert_eq!(
            PowDifficulty::current_difficulty(),
            old / DIFFICULTY_MIN_ADJUST_FACTOR
        );
    });
}

#[test]
fn saturating_cast_prevents_u128_to_u64_wraparound() {
    new_test_ext().execute_with(|| {
        CurrentDifficulty::<Test>::put(u64::MAX - 1);
        WindowStartMs::<Test>::put(999);
        WindowStartBlock::<Test>::put(1);
        System::set_block_number(FIRST_ADJUST_BLOCK);
        Timestamp::set_timestamp(1_000);
        set_extrinsic_count(2);
        PowDifficulty::on_finalize(FIRST_ADJUST_BLOCK);

        assert_eq!(PowDifficulty::current_difficulty(), u64::MAX);
    });
}

#[test]
fn zero_difficulty_storage_is_repaired_without_panic() {
    new_test_ext().execute_with(|| {
        CurrentDifficulty::<Test>::put(0);
        WindowStartMs::<Test>::put(0);
        WindowStartBlock::<Test>::put(1);
        System::set_block_number(FIRST_ADJUST_BLOCK);
        Timestamp::set_timestamp(DIFFICULTY_TARGET_WINDOW_MS);
        set_extrinsic_count(2);
        PowDifficulty::on_finalize(FIRST_ADJUST_BLOCK);

        assert_eq!(PowDifficulty::current_difficulty(), 1);
        System::assert_last_event(RuntimeEvent::PowDifficulty(Event::DifficultyAdjusted {
            block: FIRST_ADJUST_BLOCK,
            old_difficulty: 0,
            new_difficulty: 1,
            actual_window_ms: DIFFICULTY_TARGET_WINDOW_MS,
            target_window_ms: DIFFICULTY_TARGET_WINDOW_MS,
        }));
    });
}

#[test]
fn runtime_rejects_empty_block_before_difficulty_state_changes() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        Timestamp::set_timestamp(30_000);
        set_extrinsic_count(1);

        let rejected = catch_unwind(AssertUnwindSafe(|| PowDifficulty::on_finalize(1)));
        assert!(
            rejected.is_err(),
            "runtime 必须独立拒绝只有 timestamp 的空块"
        );
        assert_eq!(PowDifficulty::current_difficulty(), POW_INITIAL_DIFFICULTY);
        assert_eq!(WindowStartMs::<Test>::get(), None);
    });
}

#[test]
fn runtime_accepts_timestamp_plus_transaction() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        Timestamp::set_timestamp(30_000);
        set_extrinsic_count(2);

        PowDifficulty::on_finalize(1);
        assert_eq!(WindowStartMs::<Test>::get(), Some(30_000));
    });
}

#[test]
fn staged_params_activate_next_block_without_changing_current_difficulty() {
    new_test_ext().execute_with(|| {
        let old_difficulty = PowDifficulty::current_difficulty();
        WindowStartBlock::<Test>::put(1);
        WindowStartMs::<Test>::put(10_000);

        let mut next = PowDifficultyParams::genesis_default();
        next.params_version += 1;
        next.target_block_time_ms = 120_000;
        next.adjustment_interval = 20;
        assert!(PowDifficulty::stage_params(next, 2).is_ok());

        System::set_block_number(2);
        let _ = PowDifficulty::on_initialize(2);
        assert_eq!(PowDifficulty::active_params(), next);
        assert_eq!(PendingParams::<Test>::get(), None);
        assert_eq!(PowDifficulty::current_difficulty(), old_difficulty);
        assert_eq!(WindowStartBlock::<Test>::get(), None);
        assert_eq!(WindowStartMs::<Test>::get(), None);

        Timestamp::set_timestamp(20_000);
        set_extrinsic_count(2);
        PowDifficulty::on_finalize(2);
        assert_eq!(WindowStartBlock::<Test>::get(), Some(2));
        assert_eq!(WindowStartMs::<Test>::get(), Some(20_000));
        assert_eq!(PowDifficulty::current_difficulty(), old_difficulty);
    });
}

#[test]
fn params_version_must_match_whether_values_changed() {
    new_test_ext().execute_with(|| {
        let active = PowDifficultyParams::genesis_default();
        let mut wrong = active;
        wrong.target_block_time_ms = 120_000;
        assert!(PowDifficulty::stage_params(wrong, 2).is_err());

        let mut unchanged_with_new_version = active;
        unchanged_with_new_version.params_version += 1;
        assert!(PowDifficulty::stage_params(unchanged_with_new_version, 2).is_err());
        assert_eq!(PendingParams::<Test>::get(), None);
    });
}

#[test]
fn unsupported_algorithm_version_is_rejected_before_staging() {
    new_test_ext().execute_with(|| {
        let mut next = PowDifficultyParams::genesis_default();
        next.params_version += 1;
        next.algorithm_version += 1;
        assert!(PowDifficulty::stage_params(next, 2).is_err());
        assert_eq!(PendingParams::<Test>::get(), None);
    });
}
