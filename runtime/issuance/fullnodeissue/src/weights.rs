//! 定义全节点奖励账户和发行的执行及数据库读写权重。

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{constants::RocksDbWeight, Weight}};
use core::marker::PhantomData;

/// FRAME实测权重接口。
pub trait WeightInfo {
    fn bind_reward_account() -> Weight;
    fn rebind_reward_account() -> Weight;
    fn on_finalize_reward() -> Weight;
}
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Storage: `FullnodeIssue::RewardAccountIdByMiner` (r:1 w:1)
	/// Proof: `FullnodeIssue::RewardAccountIdByMiner` (`max_values`: None, `max_size`: Some(80), added: 2555, mode: `MaxEncodedLen`)
	fn bind_reward_account() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `6`
		//  Estimated: `3545`
		// Minimum execution time: 4_000_000 picoseconds.
		Weight::from_parts(5_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3545))
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}
	/// Storage: `FullnodeIssue::RewardAccountIdByMiner` (r:1 w:1)
	/// Proof: `FullnodeIssue::RewardAccountIdByMiner` (`max_values`: None, `max_size`: Some(80), added: 2555, mode: `MaxEncodedLen`)
	fn rebind_reward_account() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `111`
		//  Estimated: `3545`
		// Minimum execution time: 6_000_000 picoseconds.
		Weight::from_parts(6_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3545))
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}
	/// Storage: `FullnodeIssue::RewardAccountIdByMiner` (r:1 w:0)
	/// Proof: `FullnodeIssue::RewardAccountIdByMiner` (`max_values`: None, `max_size`: Some(80), added: 2555, mode: `MaxEncodedLen`)
	/// Storage: `System::Account` (r:1 w:1)
	/// Proof: `System::Account` (`max_values`: None, `max_size`: Some(128), added: 2603, mode: `MaxEncodedLen`)
	/// Storage: `FullnodeIssue::RewardedBlockCount` (r:1 w:1)
	/// Proof: `FullnodeIssue::RewardedBlockCount` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `FullnodeIssue::TotalFullnodeIssued` (r:1 w:1)
	/// Proof: `FullnodeIssue::TotalFullnodeIssued` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	/// Storage: `FullnodeIssue::LastRewardAudit` (r:0 w:1)
	/// Proof: `FullnodeIssue::LastRewardAudit` (`max_values`: Some(1), `max_size`: Some(84), added: 579, mode: `MaxEncodedLen`)
	/// Storage: `FullnodeIssue::LastAuthoredBlockByMiner` (r:0 w:1)
	/// Proof: `FullnodeIssue::LastAuthoredBlockByMiner` (`max_values`: None, `max_size`: Some(52), added: 2527, mode: `MaxEncodedLen`)
	fn on_finalize_reward() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `267`
		//  Estimated: `3593`
		// Minimum execution time: 17_000_000 picoseconds.
		Weight::from_parts(17_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3593))
			.saturating_add(T::DbWeight::get().reads(4))
			.saturating_add(T::DbWeight::get().writes(5))
	}
}
impl WeightInfo for () {
	/// Storage: `FullnodeIssue::RewardAccountIdByMiner` (r:1 w:1)
	/// Proof: `FullnodeIssue::RewardAccountIdByMiner` (`max_values`: None, `max_size`: Some(80), added: 2555, mode: `MaxEncodedLen`)
	fn bind_reward_account() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `6`
		//  Estimated: `3545`
		// Minimum execution time: 4_000_000 picoseconds.
		Weight::from_parts(5_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3545))
			.saturating_add(RocksDbWeight::get().reads(1))
			.saturating_add(RocksDbWeight::get().writes(1))
	}
	/// Storage: `FullnodeIssue::RewardAccountIdByMiner` (r:1 w:1)
	/// Proof: `FullnodeIssue::RewardAccountIdByMiner` (`max_values`: None, `max_size`: Some(80), added: 2555, mode: `MaxEncodedLen`)
	fn rebind_reward_account() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `111`
		//  Estimated: `3545`
		// Minimum execution time: 6_000_000 picoseconds.
		Weight::from_parts(6_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3545))
			.saturating_add(RocksDbWeight::get().reads(1))
			.saturating_add(RocksDbWeight::get().writes(1))
	}
	/// Storage: `FullnodeIssue::RewardAccountIdByMiner` (r:1 w:0)
	/// Proof: `FullnodeIssue::RewardAccountIdByMiner` (`max_values`: None, `max_size`: Some(80), added: 2555, mode: `MaxEncodedLen`)
	/// Storage: `System::Account` (r:1 w:1)
	/// Proof: `System::Account` (`max_values`: None, `max_size`: Some(128), added: 2603, mode: `MaxEncodedLen`)
	/// Storage: `FullnodeIssue::RewardedBlockCount` (r:1 w:1)
	/// Proof: `FullnodeIssue::RewardedBlockCount` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `FullnodeIssue::TotalFullnodeIssued` (r:1 w:1)
	/// Proof: `FullnodeIssue::TotalFullnodeIssued` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	/// Storage: `FullnodeIssue::LastRewardAudit` (r:0 w:1)
	/// Proof: `FullnodeIssue::LastRewardAudit` (`max_values`: Some(1), `max_size`: Some(84), added: 579, mode: `MaxEncodedLen`)
	/// Storage: `FullnodeIssue::LastAuthoredBlockByMiner` (r:0 w:1)
	/// Proof: `FullnodeIssue::LastAuthoredBlockByMiner` (`max_values`: None, `max_size`: Some(52), added: 2527, mode: `MaxEncodedLen`)
	fn on_finalize_reward() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `267`
		//  Estimated: `3593`
		// Minimum execution time: 17_000_000 picoseconds.
		Weight::from_parts(17_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3593))
			.saturating_add(RocksDbWeight::get().reads(4))
			.saturating_add(RocksDbWeight::get().writes(5))
	}
}
