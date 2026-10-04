//! 手工估算占位 weights，待 benchmark CLI 生成后替换。

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use core::marker::PhantomData;
use frame_support::{
    traits::Get,
    weights::{constants::RocksDbWeight, Weight},
};

/// Weight functions for `private_manage`.
pub trait WeightInfo {
	fn update_institution_info() -> Weight;
	fn add_institution_account() -> Weight;
	/// 机构岗位任职人发起治理并创建岗位快照提案。
	fn propose_institution_governance() -> Weight;
	/// 机构岗位任职人发起关闭提案。
	fn propose_close_private_institution() -> Weight;
}

pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn update_institution_info() -> Weight {
		Weight::from_parts(45_334_000, 0)
			.saturating_add(Weight::from_parts(0, 3619))
			.saturating_add(T::DbWeight::get().reads(3))
			.saturating_add(T::DbWeight::get().writes(2))
	}
	fn add_institution_account() -> Weight {
		Weight::from_parts(45_334_000, 0)
			.saturating_add(Weight::from_parts(0, 3619))
			.saturating_add(T::DbWeight::get().reads(3))
			.saturating_add(T::DbWeight::get().writes(2))
	}
	fn propose_institution_governance() -> Weight {
		// 当前 benchmark 夹具不能执行完整凭证与 VotePlan 外部调用；按已实测
		// 机构提案路径的证明大小与读写量取保守上界，禁止继续沿用直写维护权重。
		Weight::from_parts(400_000_000, 0)
			.saturating_add(Weight::from_parts(0, 700_000))
			.saturating_add(T::DbWeight::get().reads(35))
			.saturating_add(T::DbWeight::get().writes(30))
	}
	fn propose_close_private_institution() -> Weight {
		Weight::from_parts(400_000_000, 0)
			.saturating_add(Weight::from_parts(0, 700_000))
			.saturating_add(T::DbWeight::get().reads(35))
			.saturating_add(T::DbWeight::get().writes(30))
	}
}

impl WeightInfo for () {
	fn update_institution_info() -> Weight {
		Weight::from_parts(45_334_000, 0)
			.saturating_add(Weight::from_parts(0, 3619))
			.saturating_add(RocksDbWeight::get().reads(3))
			.saturating_add(RocksDbWeight::get().writes(2))
	}
	fn add_institution_account() -> Weight {
		Weight::from_parts(45_334_000, 0)
			.saturating_add(Weight::from_parts(0, 3619))
			.saturating_add(RocksDbWeight::get().reads(3))
			.saturating_add(RocksDbWeight::get().writes(2))
	}
	fn propose_institution_governance() -> Weight {
		Weight::from_parts(400_000_000, 0)
			.saturating_add(Weight::from_parts(0, 700_000))
			.saturating_add(RocksDbWeight::get().reads(35))
			.saturating_add(RocksDbWeight::get().writes(30))
	}
	fn propose_close_private_institution() -> Weight {
		Weight::from_parts(400_000_000, 0)
			.saturating_add(Weight::from_parts(0, 700_000))
			.saturating_add(RocksDbWeight::get().reads(35))
			.saturating_add(RocksDbWeight::get().writes(30))
	}
}
