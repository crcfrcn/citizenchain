//! 定义公民认证奖励的执行及数据库读写权重。

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use core::marker::PhantomData;
use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};

/// Weight functions for `citizenissue`.
pub trait WeightInfo {
	/// Storage: `CitizenIssue::IdentityRewardClaimed` (r:1 w:1)
	/// Proof: `CitizenIssue::IdentityRewardClaimed` (`max_values`: None, `max_size`: Some(49), added: 2524, mode: `MaxEncodedLen`)
	/// Storage: `CitizenIssue::PendingIdentityRewardClaimed` (r:1 w:1)
	/// Proof: `CitizenIssue::PendingIdentityRewardClaimed` (`max_values`: None, `max_size`: Some(49), added: 2524, mode: `MaxEncodedLen`)
	/// Storage: `CitizenIssue::AccountRewarded` (r:1 w:1)
	/// Proof: `CitizenIssue::AccountRewarded` (`max_values`: None, `max_size`: Some(48), added: 2523, mode: `MaxEncodedLen`)
	/// Storage: `CitizenIssue::PendingAccountRewarded` (r:1 w:1)
	/// Proof: `CitizenIssue::PendingAccountRewarded` (`max_values`: None, `max_size`: Some(48), added: 2523, mode: `MaxEncodedLen`)
	/// Storage: `CitizenIssue::RewardedCount` (r:1 w:1)
	/// Proof: `CitizenIssue::RewardedCount` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `CitizenIssue::PendingRewardCount` (r:1 w:1)
	/// Proof: `CitizenIssue::PendingRewardCount` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `System::Account` (r:1 w:1)
	/// Proof: `System::Account` (`max_values`: None, `max_size`: Some(128), added: 2603, mode: `MaxEncodedLen`)
	/// Storage: `CitizenIssue::PendingRewards` (r:0 w:1)
	/// Proof: `CitizenIssue::PendingRewards` (`max_values`: None, `max_size`: Some(77), added: 2552, mode: `MaxEncodedLen`)
	fn on_voting_identity_registered() -> Weight;
}

pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Storage: `CitizenIssue::IdentityRewardClaimed` (r:1 w:1)
	/// Proof: `CitizenIssue::IdentityRewardClaimed` (`max_values`: None, `max_size`: Some(49), added: 2524, mode: `MaxEncodedLen`)
	/// Storage: `CitizenIssue::PendingIdentityRewardClaimed` (r:1 w:1)
	/// Proof: `CitizenIssue::PendingIdentityRewardClaimed` (`max_values`: None, `max_size`: Some(49), added: 2524, mode: `MaxEncodedLen`)
	/// Storage: `CitizenIssue::AccountRewarded` (r:1 w:1)
	/// Proof: `CitizenIssue::AccountRewarded` (`max_values`: None, `max_size`: Some(48), added: 2523, mode: `MaxEncodedLen`)
	/// Storage: `CitizenIssue::PendingAccountRewarded` (r:1 w:1)
	/// Proof: `CitizenIssue::PendingAccountRewarded` (`max_values`: None, `max_size`: Some(48), added: 2523, mode: `MaxEncodedLen`)
	/// Storage: `CitizenIssue::RewardedCount` (r:1 w:1)
	/// Proof: `CitizenIssue::RewardedCount` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `CitizenIssue::PendingRewardCount` (r:1 w:1)
	/// Proof: `CitizenIssue::PendingRewardCount` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `System::Account` (r:1 w:1)
	/// Proof: `System::Account` (`max_values`: None, `max_size`: Some(128), added: 2603, mode: `MaxEncodedLen`)
	/// Storage: `CitizenIssue::PendingRewards` (r:0 w:1)
	/// Proof: `CitizenIssue::PendingRewards` (`max_values`: None, `max_size`: Some(77), added: 2552, mode: `MaxEncodedLen`)
	fn on_voting_identity_registered() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `76`
		//  Estimated: `3593`
		// Minimum execution time: 25_000_000 picoseconds.
		Weight::from_parts(26_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3593))
			.saturating_add(T::DbWeight::get().reads(7))
			.saturating_add(T::DbWeight::get().writes(8))
	}
}

impl WeightInfo for () {
	/// Storage: `CitizenIssue::IdentityRewardClaimed` (r:1 w:1)
	/// Proof: `CitizenIssue::IdentityRewardClaimed` (`max_values`: None, `max_size`: Some(49), added: 2524, mode: `MaxEncodedLen`)
	/// Storage: `CitizenIssue::PendingIdentityRewardClaimed` (r:1 w:1)
	/// Proof: `CitizenIssue::PendingIdentityRewardClaimed` (`max_values`: None, `max_size`: Some(49), added: 2524, mode: `MaxEncodedLen`)
	/// Storage: `CitizenIssue::AccountRewarded` (r:1 w:1)
	/// Proof: `CitizenIssue::AccountRewarded` (`max_values`: None, `max_size`: Some(48), added: 2523, mode: `MaxEncodedLen`)
	/// Storage: `CitizenIssue::PendingAccountRewarded` (r:1 w:1)
	/// Proof: `CitizenIssue::PendingAccountRewarded` (`max_values`: None, `max_size`: Some(48), added: 2523, mode: `MaxEncodedLen`)
	/// Storage: `CitizenIssue::RewardedCount` (r:1 w:1)
	/// Proof: `CitizenIssue::RewardedCount` (`max_values`: Some(1), `max_size`: Some(8), added: 503, mode: `MaxEncodedLen`)
	/// Storage: `CitizenIssue::PendingRewardCount` (r:1 w:1)
	/// Proof: `CitizenIssue::PendingRewardCount` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `System::Account` (r:1 w:1)
	/// Proof: `System::Account` (`max_values`: None, `max_size`: Some(128), added: 2603, mode: `MaxEncodedLen`)
	/// Storage: `CitizenIssue::PendingRewards` (r:0 w:1)
	/// Proof: `CitizenIssue::PendingRewards` (`max_values`: None, `max_size`: Some(77), added: 2552, mode: `MaxEncodedLen`)
	fn on_voting_identity_registered() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `76`
		//  Estimated: `3593`
		// Minimum execution time: 25_000_000 picoseconds.
		Weight::from_parts(26_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3593))
			.saturating_add(RocksDbWeight::get().reads(7))
			.saturating_add(RocksDbWeight::get().writes(8))
	}
}
