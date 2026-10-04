//! PoW 难度模块 benchmark 定义。

#![cfg(feature = "runtime-benchmarks")]

use frame_benchmarking::v2::*;
use frame_support::traits::Hooks;
use primitives::pow_const::{DIFFICULTY_ADJUSTMENT_INTERVAL, DIFFICULTY_TARGET_WINDOW_MS};
use sp_runtime::traits::SaturatedConversion;

use crate::{
    pallet::{
        ActiveParams, Config, CurrentDifficulty, Pallet, PendingParams, WindowStartBlock,
        WindowStartMs,
    },
    PendingPowDifficultyParams, PowDifficultyParams,
};

#[benchmarks]
mod benchmarks {
    use super::*;

    /// benchmark 直接调用 hook，需模拟 Executive 已记录 timestamp 与一笔用户交易。
    fn note_non_empty_block<T: Config>() {
        for _ in 0..2 {
            frame_system::Pallet::<T>::note_applied_extrinsic(&Ok(().into()), Default::default());
        }
        frame_system::Pallet::<T>::note_finished_extrinsics();
    }

    #[benchmark]
    fn on_initialize_adjustment() {
        // 模拟真正的"结算块"路径，覆盖一次完整的难度计算和窗口推进。
        let n: frame_system::pallet_prelude::BlockNumberFor<T> =
            (DIFFICULTY_ADJUSTMENT_INTERVAL + 1).saturated_into();
        frame_system::Pallet::<T>::set_block_number(n);
        CurrentDifficulty::<T>::put(1_000u64);
        WindowStartMs::<T>::put(1_000u64);
        WindowStartBlock::<T>::put(1u32);
        pallet_timestamp::Pallet::<T>::set_timestamp(
            (1_000u64.saturating_add(DIFFICULTY_TARGET_WINDOW_MS)).saturated_into(),
        );
        note_non_empty_block::<T>();

        #[block]
        {
            let _ = Pallet::<T>::on_initialize(n);
            Pallet::<T>::on_finalize(n);
        }

        assert_eq!(
            WindowStartMs::<T>::get(),
            Some(1_000u64.saturating_add(DIFFICULTY_TARGET_WINDOW_MS))
        );
    }

    #[benchmark]
    fn on_initialize_activate_params() {
        // 参数只能由 runtime 升级在前一块暂存；激活块原子切换参数并重建窗口。
        let n: frame_system::pallet_prelude::BlockNumberFor<T> = 2u32.saturated_into();
        frame_system::Pallet::<T>::set_block_number(n);
        let active = PowDifficultyParams::genesis_default();
        let mut next = active;
        next.params_version = active.params_version.saturating_add(1);
        next.target_block_time_ms = next.target_block_time_ms.saturating_add(1_000);
        ActiveParams::<T>::put(active);
        PendingParams::<T>::put(PendingPowDifficultyParams {
            params: next,
            activate_at: 2,
        });
        WindowStartMs::<T>::put(1_000u64);
        WindowStartBlock::<T>::put(1u32);
        pallet_timestamp::Pallet::<T>::set_timestamp(12_000u64.saturated_into());
        note_non_empty_block::<T>();

        #[block]
        {
            let _ = Pallet::<T>::on_initialize(n);
            Pallet::<T>::on_finalize(n);
        }

        assert_eq!(ActiveParams::<T>::get(), next);
        assert!(PendingParams::<T>::get().is_none());
        assert_eq!(WindowStartBlock::<T>::get(), Some(2));
        assert_eq!(WindowStartMs::<T>::get(), Some(12_000));
    }

    #[benchmark]
    fn on_initialize_start_window() {
        // 模拟链刚启动或窗口状态丢失后，首个有时间戳区块建立窗口起点。
        let n: frame_system::pallet_prelude::BlockNumberFor<T> = 1u32.saturated_into();
        frame_system::Pallet::<T>::set_block_number(n);
        WindowStartMs::<T>::kill();
        WindowStartBlock::<T>::kill();
        pallet_timestamp::Pallet::<T>::set_timestamp(6_000u64.saturated_into());
        note_non_empty_block::<T>();

        #[block]
        {
            let _ = Pallet::<T>::on_initialize(n);
            Pallet::<T>::on_finalize(n);
        }

        assert_eq!(WindowStartMs::<T>::get(), Some(6_000u64));
    }

    #[benchmark]
    fn on_initialize_idle() {
        // 普通区块不触发调整，也不重建窗口，只验证空转路径预算。
        let n: frame_system::pallet_prelude::BlockNumberFor<T> = 2u32.saturated_into();
        frame_system::Pallet::<T>::set_block_number(n);
        WindowStartMs::<T>::put(1_000u64);
        WindowStartBlock::<T>::put(1u32);
        pallet_timestamp::Pallet::<T>::set_timestamp(12_000u64.saturated_into());
        note_non_empty_block::<T>();

        #[block]
        {
            let _ = Pallet::<T>::on_initialize(n);
            Pallet::<T>::on_finalize(n);
        }

        assert_eq!(WindowStartMs::<T>::get(), Some(1_000u64));
    }

    impl_benchmark_test_suite!(Pallet, crate::tests::new_test_ext(), crate::tests::Test,);
}
