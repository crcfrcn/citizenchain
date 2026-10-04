//! 省储行固定年度利息 benchmark。

#![cfg(feature = "runtime-benchmarks")]

use crate::{pallet, Config, Pallet};
use frame_benchmarking::v2::*;
use frame_support::traits::{Get, Hooks};

#[benchmarks]
mod benchmarks {
    use super::*;

    /// 年度边界 finalize：向固定 43 家省储行主账户发行，并原子更新三项审计状态。
    #[benchmark]
    fn on_finalize_settlement() {
        let blocks_per_year = T::BlocksPerYear::get();
        let n: frame_system::pallet_prelude::BlockNumberFor<T> =
            u32::try_from(blocks_per_year.max(1))
                .unwrap_or(u32::MAX)
                .into();
        frame_system::Pallet::<T>::set_block_number(n);
        pallet::LastSettledYear::<T>::put(0u32);

        #[block]
        {
            Pallet::<T>::on_finalize(n);
        }

        assert_eq!(pallet::LastSettledYear::<T>::get(), 1u32);
        assert!(pallet::TotalProvincialBankInterestIssued::<T>::get() > 0);
        assert!(pallet::LastProvincialBankInterestAudit::<T>::get().is_some());
    }

    impl_benchmark_test_suite!(Pallet, crate::tests::new_test_ext(), crate::tests::Test,);
}
