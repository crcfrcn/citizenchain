//! WeightInfo 占位。
//!
//! 框架阶段统一走 Weight::zero() 占位,实装与 benchmarks.rs 联动后由
//! `frame-benchmarking` 自动生成,然后 `cargo run --features runtime-benchmarks --
//! benchmark pallet --pallet onchain-issuance --extrinsic '*' --output ...` 落盘。

use frame_support::weights::Weight;

pub trait WeightInfo {
    fn issue() -> Weight;
    fn mint() -> Weight;
    fn burn() -> Weight;
    fn transfer() -> Weight;
    fn close() -> Weight;
    fn monitor_freeze() -> Weight;
    fn monitor_unfreeze() -> Weight;
    fn monitor_confiscate() -> Weight;
    fn monitor_force_transfer() -> Weight;
    fn monitor_force_close() -> Weight;
}

/// 默认 zero weight 实现,仅供 mock runtime / 框架阶段 cargo check 通过。
///
/// 实装时换成 `SubstrateWeight<T>` 模式 + benchmarks 自动生成。
pub struct ZeroWeight;

impl WeightInfo for ZeroWeight {
    fn issue() -> Weight {
        Weight::zero()
    }
    fn mint() -> Weight {
        Weight::zero()
    }
    fn burn() -> Weight {
        Weight::zero()
    }
    fn transfer() -> Weight {
        Weight::zero()
    }
    fn close() -> Weight {
        Weight::zero()
    }
    fn monitor_freeze() -> Weight {
        Weight::zero()
    }
    fn monitor_unfreeze() -> Weight {
        Weight::zero()
    }
    fn monitor_confiscate() -> Weight {
        Weight::zero()
    }
    fn monitor_force_transfer() -> Weight {
        Weight::zero()
    }
    fn monitor_force_close() -> Weight {
        Weight::zero()
    }
}
