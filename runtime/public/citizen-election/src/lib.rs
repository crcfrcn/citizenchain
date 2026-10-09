//! 公民选举模块，用于公民选举公职人员。
//!
//! 当前仅在 Runtime 中占位，不提供交易入口、业务存储或事件。
//! 后续逐步实现选举业务规则，并接入 election-vote 投票引擎。

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    #[pallet::config]
    pub trait Config: frame_system::Config {}

    #[pallet::pallet]
    pub struct Pallet<T>(_);
}
