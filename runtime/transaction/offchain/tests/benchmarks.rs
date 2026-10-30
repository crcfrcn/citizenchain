//! 已实现的费率治理与批次签名成本基准；不打开Runtime禁用的链下业务。
#![cfg(feature = "runtime-benchmarks")]
use crate::{batch_item::batch_signing_hash, Call, Config, MaxL2FeeRateBp, Pallet};
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;
use sp_std::vec;

#[benchmarks]
mod benchmarks {
    use super::*;
    #[benchmark]
    fn set_max_l2_fee_rate() {
        #[extrinsic_call]
        _(RawOrigin::Root, 10);
        assert_eq!(MaxL2FeeRateBp::<T>::get(), 10);
    }
    #[benchmark]
    fn batch_signature_payload(n: Linear<1, 4096>) {
        let who: T::AccountId = account("bank", 0, 0);
        let bytes = vec![7u8; n as usize];
        let expected = batch_signing_hash(b"fixture-bank", b"DIRECTOR", &who, 1, &bytes);
        let measured = core::cell::Cell::new([0u8; 32]);
        #[block]
        {
            measured.set(core::hint::black_box(batch_signing_hash(
                b"fixture-bank",
                b"DIRECTOR",
                &who,
                1,
                core::hint::black_box(&bytes),
            )));
        }
        let measured = measured.get();
        assert_eq!(measured, expected);
        assert_ne!(
            measured,
            batch_signing_hash(b"fixture-bank", b"DIRECTOR", &who, 2, &bytes)
        );
    }
    impl_benchmark_test_suite!(Pallet, crate::tests::new_test_ext(), crate::tests::Test);
}
