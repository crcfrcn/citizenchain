//! 定义PoW难度调整的执行及数据库读写权重。

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use core::marker::PhantomData;
use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};

/// Weight functions for `difficulty`.
pub trait WeightInfo {
	/// Storage: `Difficulty::PendingParams` (r:1 w:0)
	/// Proof: `Difficulty::PendingParams` (`max_values`: Some(1), `max_size`: Some(38), added: 533, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::ActiveParams` (r:1 w:0)
	/// Proof: `Difficulty::ActiveParams` (`max_values`: Some(1), `max_size`: Some(34), added: 529, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartBlock` (r:1 w:1)
	/// Proof: `Difficulty::WindowStartBlock` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `Timestamp::Now` (r:1 w:0)
	/// Proof: `Timestamp::Now` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartMs` (r:1 w:1)
	/// Proof: `Difficulty::WindowStartMs` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::CurrentDifficulty` (r:1 w:1)
	/// Proof: `Difficulty::CurrentDifficulty` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::LastAdjustment` (r:0 w:1)
	/// Proof: `Difficulty::LastAdjustment` (`max_values`: Some(1), `max_size`: Some(36), added: 531, mode: `MaxEncodedLen`)
	fn on_initialize_adjustment() -> Weight;
	/// Storage: `Difficulty::PendingParams` (r:1 w:1)
	/// Proof: `Difficulty::PendingParams` (`max_values`: Some(1), `max_size`: Some(38), added: 533, mode: `MaxEncodedLen`)
	/// Storage: `Timestamp::Now` (r:1 w:0)
	/// Proof: `Timestamp::Now` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::ActiveParams` (r:0 w:1)
	/// Proof: `Difficulty::ActiveParams` (`max_values`: Some(1), `max_size`: Some(34), added: 529, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartBlock` (r:0 w:1)
	/// Proof: `Difficulty::WindowStartBlock` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartMs` (r:0 w:1)
	/// Proof: `Difficulty::WindowStartMs` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	fn on_initialize_activate_params() -> Weight;
	/// Storage: `Difficulty::PendingParams` (r:1 w:0)
	/// Proof: `Difficulty::PendingParams` (`max_values`: Some(1), `max_size`: Some(38), added: 533, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::ActiveParams` (r:1 w:0)
	/// Proof: `Difficulty::ActiveParams` (`max_values`: Some(1), `max_size`: Some(34), added: 529, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartBlock` (r:1 w:1)
	/// Proof: `Difficulty::WindowStartBlock` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartMs` (r:1 w:1)
	/// Proof: `Difficulty::WindowStartMs` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Timestamp::Now` (r:1 w:0)
	/// Proof: `Timestamp::Now` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	fn on_initialize_start_window() -> Weight;
	/// Storage: `Difficulty::PendingParams` (r:1 w:0)
	/// Proof: `Difficulty::PendingParams` (`max_values`: Some(1), `max_size`: Some(38), added: 533, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::ActiveParams` (r:1 w:0)
	/// Proof: `Difficulty::ActiveParams` (`max_values`: Some(1), `max_size`: Some(34), added: 529, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartBlock` (r:1 w:0)
	/// Proof: `Difficulty::WindowStartBlock` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartMs` (r:1 w:0)
	/// Proof: `Difficulty::WindowStartMs` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Timestamp::Now` (r:1 w:0)
	/// Proof: `Timestamp::Now` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	fn on_initialize_idle() -> Weight;
}

pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Storage: `Difficulty::PendingParams` (r:1 w:0)
	/// Proof: `Difficulty::PendingParams` (`max_values`: Some(1), `max_size`: Some(38), added: 533, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::ActiveParams` (r:1 w:0)
	/// Proof: `Difficulty::ActiveParams` (`max_values`: Some(1), `max_size`: Some(34), added: 529, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartBlock` (r:1 w:1)
	/// Proof: `Difficulty::WindowStartBlock` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `Timestamp::Now` (r:1 w:0)
	/// Proof: `Timestamp::Now` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartMs` (r:1 w:1)
	/// Proof: `Difficulty::WindowStartMs` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::CurrentDifficulty` (r:1 w:1)
	/// Proof: `Difficulty::CurrentDifficulty` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::LastAdjustment` (r:0 w:1)
	/// Proof: `Difficulty::LastAdjustment` (`max_values`: Some(1), `max_size`: Some(36), added: 531, mode: `MaxEncodedLen`)
	fn on_initialize_adjustment() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `240`
		//  Estimated: `1523`
		// Minimum execution time: 11_000_000 picoseconds.
		Weight::from_parts(11_000_000, 0)
			.saturating_add(Weight::from_parts(0, 1523))
			.saturating_add(T::DbWeight::get().reads(6))
			.saturating_add(T::DbWeight::get().writes(4))
	}
	/// Storage: `Difficulty::PendingParams` (r:1 w:1)
	/// Proof: `Difficulty::PendingParams` (`max_values`: Some(1), `max_size`: Some(38), added: 533, mode: `MaxEncodedLen`)
	/// Storage: `Timestamp::Now` (r:1 w:0)
	/// Proof: `Timestamp::Now` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::ActiveParams` (r:0 w:1)
	/// Proof: `Difficulty::ActiveParams` (`max_values`: Some(1), `max_size`: Some(34), added: 529, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartBlock` (r:0 w:1)
	/// Proof: `Difficulty::WindowStartBlock` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartMs` (r:0 w:1)
	/// Proof: `Difficulty::WindowStartMs` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	fn on_initialize_activate_params() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `247`
		//  Estimated: `1523`
		// Minimum execution time: 7_000_000 picoseconds.
		Weight::from_parts(7_000_000, 0)
			.saturating_add(Weight::from_parts(0, 1523))
			.saturating_add(T::DbWeight::get().reads(2))
			.saturating_add(T::DbWeight::get().writes(4))
	}
	/// Storage: `Difficulty::PendingParams` (r:1 w:0)
	/// Proof: `Difficulty::PendingParams` (`max_values`: Some(1), `max_size`: Some(38), added: 533, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::ActiveParams` (r:1 w:0)
	/// Proof: `Difficulty::ActiveParams` (`max_values`: Some(1), `max_size`: Some(34), added: 529, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartBlock` (r:1 w:1)
	/// Proof: `Difficulty::WindowStartBlock` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartMs` (r:1 w:1)
	/// Proof: `Difficulty::WindowStartMs` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Timestamp::Now` (r:1 w:0)
	/// Proof: `Timestamp::Now` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	fn on_initialize_start_window() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `189`
		//  Estimated: `1523`
		// Minimum execution time: 7_000_000 picoseconds.
		Weight::from_parts(8_000_000, 0)
			.saturating_add(Weight::from_parts(0, 1523))
			.saturating_add(T::DbWeight::get().reads(5))
			.saturating_add(T::DbWeight::get().writes(2))
	}
	/// Storage: `Difficulty::PendingParams` (r:1 w:0)
	/// Proof: `Difficulty::PendingParams` (`max_values`: Some(1), `max_size`: Some(38), added: 533, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::ActiveParams` (r:1 w:0)
	/// Proof: `Difficulty::ActiveParams` (`max_values`: Some(1), `max_size`: Some(34), added: 529, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartBlock` (r:1 w:0)
	/// Proof: `Difficulty::WindowStartBlock` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartMs` (r:1 w:0)
	/// Proof: `Difficulty::WindowStartMs` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Timestamp::Now` (r:1 w:0)
	/// Proof: `Timestamp::Now` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	fn on_initialize_idle() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `240`
		//  Estimated: `1523`
		// Minimum execution time: 9_000_000 picoseconds.
		Weight::from_parts(9_000_000, 0)
			.saturating_add(Weight::from_parts(0, 1523))
			.saturating_add(T::DbWeight::get().reads(5))
	}
}

impl WeightInfo for () {
	/// Storage: `Difficulty::PendingParams` (r:1 w:0)
	/// Proof: `Difficulty::PendingParams` (`max_values`: Some(1), `max_size`: Some(38), added: 533, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::ActiveParams` (r:1 w:0)
	/// Proof: `Difficulty::ActiveParams` (`max_values`: Some(1), `max_size`: Some(34), added: 529, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartBlock` (r:1 w:1)
	/// Proof: `Difficulty::WindowStartBlock` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `Timestamp::Now` (r:1 w:0)
	/// Proof: `Timestamp::Now` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartMs` (r:1 w:1)
	/// Proof: `Difficulty::WindowStartMs` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::CurrentDifficulty` (r:1 w:1)
	/// Proof: `Difficulty::CurrentDifficulty` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::LastAdjustment` (r:0 w:1)
	/// Proof: `Difficulty::LastAdjustment` (`max_values`: Some(1), `max_size`: Some(36), added: 531, mode: `MaxEncodedLen`)
	fn on_initialize_adjustment() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `240`
		//  Estimated: `1523`
		// Minimum execution time: 11_000_000 picoseconds.
		Weight::from_parts(11_000_000, 0)
			.saturating_add(Weight::from_parts(0, 1523))
			.saturating_add(RocksDbWeight::get().reads(6))
			.saturating_add(RocksDbWeight::get().writes(4))
	}
	/// Storage: `Difficulty::PendingParams` (r:1 w:1)
	/// Proof: `Difficulty::PendingParams` (`max_values`: Some(1), `max_size`: Some(38), added: 533, mode: `MaxEncodedLen`)
	/// Storage: `Timestamp::Now` (r:1 w:0)
	/// Proof: `Timestamp::Now` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::ActiveParams` (r:0 w:1)
	/// Proof: `Difficulty::ActiveParams` (`max_values`: Some(1), `max_size`: Some(34), added: 529, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartBlock` (r:0 w:1)
	/// Proof: `Difficulty::WindowStartBlock` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartMs` (r:0 w:1)
	/// Proof: `Difficulty::WindowStartMs` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	fn on_initialize_activate_params() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `247`
		//  Estimated: `1523`
		// Minimum execution time: 7_000_000 picoseconds.
		Weight::from_parts(7_000_000, 0)
			.saturating_add(Weight::from_parts(0, 1523))
			.saturating_add(RocksDbWeight::get().reads(2))
			.saturating_add(RocksDbWeight::get().writes(4))
	}
	/// Storage: `Difficulty::PendingParams` (r:1 w:0)
	/// Proof: `Difficulty::PendingParams` (`max_values`: Some(1), `max_size`: Some(38), added: 533, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::ActiveParams` (r:1 w:0)
	/// Proof: `Difficulty::ActiveParams` (`max_values`: Some(1), `max_size`: Some(34), added: 529, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartBlock` (r:1 w:1)
	/// Proof: `Difficulty::WindowStartBlock` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartMs` (r:1 w:1)
	/// Proof: `Difficulty::WindowStartMs` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Timestamp::Now` (r:1 w:0)
	/// Proof: `Timestamp::Now` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	fn on_initialize_start_window() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `189`
		//  Estimated: `1523`
		// Minimum execution time: 7_000_000 picoseconds.
		Weight::from_parts(8_000_000, 0)
			.saturating_add(Weight::from_parts(0, 1523))
			.saturating_add(RocksDbWeight::get().reads(5))
			.saturating_add(RocksDbWeight::get().writes(2))
	}
	/// Storage: `Difficulty::PendingParams` (r:1 w:0)
	/// Proof: `Difficulty::PendingParams` (`max_values`: Some(1), `max_size`: Some(38), added: 533, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::ActiveParams` (r:1 w:0)
	/// Proof: `Difficulty::ActiveParams` (`max_values`: Some(1), `max_size`: Some(34), added: 529, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartBlock` (r:1 w:0)
	/// Proof: `Difficulty::WindowStartBlock` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `Difficulty::WindowStartMs` (r:1 w:0)
	/// Proof: `Difficulty::WindowStartMs` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `Timestamp::Now` (r:1 w:0)
	/// Proof: `Timestamp::Now` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	fn on_initialize_idle() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `240`
		//  Estimated: `1523`
		// Minimum execution time: 9_000_000 picoseconds.
		Weight::from_parts(9_000_000, 0)
			.saturating_add(Weight::from_parts(0, 1523))
			.saturating_add(RocksDbWeight::get().reads(5))
	}
}
