//! 使用保守执行预算，并计入完整公钥的存储证明。
//! 这些权重不是由 FRAME 基准测试生成的结果。
use frame_support::{traits::Get, weights::Weight};
pub trait WeightInfo {
    fn sr() -> Weight;
    fn first() -> Weight;
    fn stored() -> Weight;
}
pub struct SubstrateWeight<T>(core::marker::PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    fn sr() -> Weight {
        frame_support::weights::constants::ExtrinsicBaseWeight::get()
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(Weight::from_parts(0, 8_000))
    }
    fn first() -> Weight {
        Weight::from_parts(10_000_000_000, 8_000)
            .saturating_add(T::DbWeight::get().reads_writes(2, 1))
    }
    fn stored() -> Weight {
        Weight::from_parts(10_000_000_000, 8_000).saturating_add(T::DbWeight::get().reads(1))
    }
}
impl WeightInfo for () {
    fn sr() -> Weight {
        Weight::zero()
    }
    fn first() -> Weight {
        Weight::zero()
    }
    fn stored() -> Weight {
        Weight::zero()
    }
}
