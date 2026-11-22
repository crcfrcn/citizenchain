//! 定义带备注原生转账的执行及数据库读写权重。

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{constants::RocksDbWeight, Weight}};
use core::marker::PhantomData;

/// FRAME实测权重接口。
pub trait WeightInfo {
    fn transfer_with_remark() -> Weight;
}
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Storage: `System::Account` (r:1 w:1)
	/// Proof: `System::Account` (`max_values`: None, `max_size`: Some(128), added: 2603, mode: `MaxEncodedLen`)
	fn transfer_with_remark() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `105`
		//  Estimated: `3593`
		// Minimum execution time: 25_000_000 picoseconds.
		Weight::from_parts(26_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3593))
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}
}
impl WeightInfo for () {
	/// Storage: `System::Account` (r:1 w:1)
	/// Proof: `System::Account` (`max_values`: None, `max_size`: Some(128), added: 2603, mode: `MaxEncodedLen`)
	fn transfer_with_remark() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `105`
		//  Estimated: `3593`
		// Minimum execution time: 25_000_000 picoseconds.
		Weight::from_parts(26_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3593))
			.saturating_add(RocksDbWeight::get().reads(1))
			.saturating_add(RocksDbWeight::get().writes(1))
	}
}
