//! 清算行(L2)扫码支付清算 pallet 权重。
//!
//!
//! - 本文件先给出**非零保守权重**并统一接入 `T::WeightInfo`,替换早期空 trait
//!   占位,避免清算行核心 Call 只按裸 `DbWeight` 计费。
//! - 数值按当前读写路径与 sr25519 验签 / Currency transfer 的最坏路径手工上调,
//!   不是 `frame-benchmarking` 自动生成产物。
//! - 后续用带 benchmarking runtime api 的专用 WASM 重新跑 benchmark 后,应以
//!   自动生成文件替换本文件,并保留同名 trait 方法。

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use core::marker::PhantomData;
use frame_support::{
    traits::Get,
    weights::{constants::RocksDbWeight, Weight},
};

/// Weight functions for `offchain`.
pub trait WeightInfo {
	fn bind_clearing_bank() -> Weight;
	fn deposit() -> Weight;
	fn withdraw() -> Weight;
	fn switch_bank() -> Weight;
	fn submit_offchain_batch(items: u32) -> Weight;
	fn propose_l2_fee_rate() -> Weight;
	fn set_max_l2_fee_rate() -> Weight;
	fn register_clearing_bank() -> Weight;
	fn update_clearing_bank_endpoint() -> Weight;
	fn unregister_clearing_bank() -> Weight;
}

pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// L3 绑定清算行:CID/机构合法性查询 + UserBank/DepositBalance 初始化。
	fn bind_clearing_bank() -> Weight {
		Weight::from_parts(60_000_000, 0)
			.saturating_add(Weight::from_parts(0, 6_000))
			.saturating_add(T::DbWeight::get().reads(8))
			.saturating_add(T::DbWeight::get().writes(2))
	}

	/// L3 充值:读取绑定关系、偿付快照、转账并更新 DepositBalance/BankTotalDeposits。
	fn deposit() -> Weight {
		Weight::from_parts(75_000_000, 0)
			.saturating_add(Weight::from_parts(0, 7_000))
			.saturating_add(T::DbWeight::get().reads(6))
			.saturating_add(T::DbWeight::get().writes(5))
	}

	/// L3 提现:余额校验、Currency transfer、DepositBalance/BankTotalDeposits 同步。
	fn withdraw() -> Weight {
		Weight::from_parts(75_000_000, 0)
			.saturating_add(Weight::from_parts(0, 7_000))
			.saturating_add(T::DbWeight::get().reads(6))
			.saturating_add(T::DbWeight::get().writes(5))
	}

	/// L3 切换清算行:旧行余额清零校验 + 新行合法性校验 + 双索引改写。
	fn switch_bank() -> Weight {
		Weight::from_parts(70_000_000, 0)
			.saturating_add(Weight::from_parts(0, 7_000))
			.saturating_add(T::DbWeight::get().reads(8))
			.saturating_add(T::DbWeight::get().writes(4))
	}

	/// 清算行批次上链:批次级签名 + 每 item L3 签名、nonce、防重、费率、偿付与转账。
	fn submit_offchain_batch(items: u32) -> Weight {
		Weight::from_parts(120_000_000, 0)
			.saturating_add(Weight::from_parts(0, 12_000))
			.saturating_add(T::DbWeight::get().reads(10))
			.saturating_add(T::DbWeight::get().writes(2))
			.saturating_add(
				Weight::from_parts(90_000_000, 0)
					.saturating_add(Weight::from_parts(0, 8_000))
					.saturating_add(T::DbWeight::get().reads(12))
					.saturating_add(T::DbWeight::get().writes(10))
					.saturating_mul(items.into()),
			)
	}

	/// 清算行管理员提案新费率,写入待生效提案。
	fn propose_l2_fee_rate() -> Weight {
		Weight::from_parts(55_000_000, 0)
			.saturating_add(Weight::from_parts(0, 5_000))
			.saturating_add(T::DbWeight::get().reads(7))
			.saturating_add(T::DbWeight::get().writes(2))
	}

	/// 设置全局费率上限。
	fn set_max_l2_fee_rate() -> Weight {
		Weight::from_parts(20_000_000, 0)
			.saturating_add(Weight::from_parts(0, 1_500))
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}

	/// 清算行节点声明:CID 反查、管理员/资格/PeerId 唯一性校验与双索引写入。
	fn register_clearing_bank() -> Weight {
		Weight::from_parts(95_000_000, 0)
			.saturating_add(Weight::from_parts(0, 9_000))
			.saturating_add(T::DbWeight::get().reads(10))
			.saturating_add(T::DbWeight::get().writes(3))
	}

	/// 更新清算行节点 RPC 端点。
	fn update_clearing_bank_endpoint() -> Weight {
		Weight::from_parts(55_000_000, 0)
			.saturating_add(Weight::from_parts(0, 5_000))
			.saturating_add(T::DbWeight::get().reads(5))
			.saturating_add(T::DbWeight::get().writes(1))
	}

	/// 注销清算行节点声明,删除主索引和 PeerId 反向索引。
	fn unregister_clearing_bank() -> Weight {
		Weight::from_parts(55_000_000, 0)
			.saturating_add(Weight::from_parts(0, 5_000))
			.saturating_add(T::DbWeight::get().reads(5))
			.saturating_add(T::DbWeight::get().writes(2))
	}
}

impl WeightInfo for () {
	fn bind_clearing_bank() -> Weight {
		Weight::from_parts(60_000_000, 0)
			.saturating_add(Weight::from_parts(0, 6_000))
			.saturating_add(RocksDbWeight::get().reads(8))
			.saturating_add(RocksDbWeight::get().writes(2))
	}
	fn deposit() -> Weight {
		Weight::from_parts(75_000_000, 0)
			.saturating_add(Weight::from_parts(0, 7_000))
			.saturating_add(RocksDbWeight::get().reads(6))
			.saturating_add(RocksDbWeight::get().writes(5))
	}
	fn withdraw() -> Weight {
		Weight::from_parts(75_000_000, 0)
			.saturating_add(Weight::from_parts(0, 7_000))
			.saturating_add(RocksDbWeight::get().reads(6))
			.saturating_add(RocksDbWeight::get().writes(5))
	}
	fn switch_bank() -> Weight {
		Weight::from_parts(70_000_000, 0)
			.saturating_add(Weight::from_parts(0, 7_000))
			.saturating_add(RocksDbWeight::get().reads(8))
			.saturating_add(RocksDbWeight::get().writes(4))
	}
	fn submit_offchain_batch(items: u32) -> Weight {
		Weight::from_parts(120_000_000, 0)
			.saturating_add(Weight::from_parts(0, 12_000))
			.saturating_add(RocksDbWeight::get().reads(10))
			.saturating_add(RocksDbWeight::get().writes(2))
			.saturating_add(
				Weight::from_parts(90_000_000, 0)
					.saturating_add(Weight::from_parts(0, 8_000))
					.saturating_add(RocksDbWeight::get().reads(12))
					.saturating_add(RocksDbWeight::get().writes(10))
					.saturating_mul(items.into()),
			)
	}
	fn propose_l2_fee_rate() -> Weight {
		Weight::from_parts(55_000_000, 0)
			.saturating_add(Weight::from_parts(0, 5_000))
			.saturating_add(RocksDbWeight::get().reads(7))
			.saturating_add(RocksDbWeight::get().writes(2))
	}
	fn set_max_l2_fee_rate() -> Weight {
		Weight::from_parts(20_000_000, 0)
			.saturating_add(Weight::from_parts(0, 1_500))
			.saturating_add(RocksDbWeight::get().reads(1))
			.saturating_add(RocksDbWeight::get().writes(1))
	}
	fn register_clearing_bank() -> Weight {
		Weight::from_parts(95_000_000, 0)
			.saturating_add(Weight::from_parts(0, 9_000))
			.saturating_add(RocksDbWeight::get().reads(10))
			.saturating_add(RocksDbWeight::get().writes(3))
	}
	fn update_clearing_bank_endpoint() -> Weight {
		Weight::from_parts(55_000_000, 0)
			.saturating_add(Weight::from_parts(0, 5_000))
			.saturating_add(RocksDbWeight::get().reads(5))
			.saturating_add(RocksDbWeight::get().writes(1))
	}
	fn unregister_clearing_bank() -> Weight {
		Weight::from_parts(55_000_000, 0)
			.saturating_add(Weight::from_parts(0, 5_000))
			.saturating_add(RocksDbWeight::get().reads(5))
			.saturating_add(RocksDbWeight::get().writes(2))
	}
}
