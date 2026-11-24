//! 定义省储行年度发行结算的执行及数据库读写权重。

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use core::marker::PhantomData;
use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};

/// Weight functions for `prbbankissue`.
pub trait WeightInfo {
	/// Storage: `PrbBankIssue::LastSettledYear` (r:1 w:1)
	/// Proof: `PrbBankIssue::LastSettledYear` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `System::Account` (r:43 w:43)
	/// Proof: `System::Account` (`max_values`: None, `max_size`: Some(128), added: 2603, mode: `MaxEncodedLen`)
	/// Storage: `PrbBankIssue::TotalProvincialBankInterestIssued` (r:1 w:1)
	/// Proof: `PrbBankIssue::TotalProvincialBankInterestIssued` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	/// Storage: `PrbBankIssue::LastProvincialBankInterestAudit` (r:0 w:1)
	/// Proof: `PrbBankIssue::LastProvincialBankInterestAudit` (`max_values`: Some(1), `max_size`: Some(24), added: 519, mode: `MaxEncodedLen`)
	fn on_finalize_settlement() -> Weight;
}

pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Storage: `PrbBankIssue::LastSettledYear` (r:1 w:1)
	/// Proof: `PrbBankIssue::LastSettledYear` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `System::Account` (r:43 w:43)
	/// Proof: `System::Account` (`max_values`: None, `max_size`: Some(128), added: 2603, mode: `MaxEncodedLen`)
	/// Storage: `PrbBankIssue::TotalProvincialBankInterestIssued` (r:1 w:1)
	/// Proof: `PrbBankIssue::TotalProvincialBankInterestIssued` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	/// Storage: `PrbBankIssue::LastProvincialBankInterestAudit` (r:0 w:1)
	/// Proof: `PrbBankIssue::LastProvincialBankInterestAudit` (`max_values`: Some(1), `max_size`: Some(24), added: 519, mode: `MaxEncodedLen`)
	fn on_finalize_settlement() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `1619`
		//  Estimated: `112919`
		// Minimum execution time: 568_000_000 picoseconds.
		Weight::from_parts(572_000_000, 0)
			.saturating_add(Weight::from_parts(0, 112919))
			.saturating_add(T::DbWeight::get().reads(45))
			.saturating_add(T::DbWeight::get().writes(46))
	}
}

impl WeightInfo for () {
	/// Storage: `PrbBankIssue::LastSettledYear` (r:1 w:1)
	/// Proof: `PrbBankIssue::LastSettledYear` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `System::Account` (r:43 w:43)
	/// Proof: `System::Account` (`max_values`: None, `max_size`: Some(128), added: 2603, mode: `MaxEncodedLen`)
	/// Storage: `PrbBankIssue::TotalProvincialBankInterestIssued` (r:1 w:1)
	/// Proof: `PrbBankIssue::TotalProvincialBankInterestIssued` (`max_values`: Some(1), `max_size`: Some(16), added: 511, mode: `MaxEncodedLen`)
	/// Storage: `PrbBankIssue::LastProvincialBankInterestAudit` (r:0 w:1)
	/// Proof: `PrbBankIssue::LastProvincialBankInterestAudit` (`max_values`: Some(1), `max_size`: Some(24), added: 519, mode: `MaxEncodedLen`)
	fn on_finalize_settlement() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `1619`
		//  Estimated: `112919`
		// Minimum execution time: 568_000_000 picoseconds.
		Weight::from_parts(572_000_000, 0)
			.saturating_add(Weight::from_parts(0, 112919))
			.saturating_add(RocksDbWeight::get().reads(45))
			.saturating_add(RocksDbWeight::get().writes(46))
	}
}
