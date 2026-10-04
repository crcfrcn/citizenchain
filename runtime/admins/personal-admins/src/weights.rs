//! Weight functions for `personal_admins`.
//!
//! 当前为创世前保守非零权重,避免复杂写 storage 操作以零成本进入 runtime。
//! 精确数值在补齐 benchmark 后由 frame-benchmarking 自动生成覆盖。

use frame_support::weights::Weight;

pub trait WeightInfo {
    fn propose_admin_set_change() -> Weight;
}

pub struct SubstrateWeight<T>(core::marker::PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    fn propose_admin_set_change() -> Weight {
        Weight::from_parts(120_000_000, 16_000)
    }
}

impl WeightInfo for () {
    fn propose_admin_set_change() -> Weight {
        Weight::from_parts(120_000_000, 16_000)
    }
}
