//! 公民选举模块，用于公民选举公职人员。
//!
//! 当前仅在 Runtime 中占位，不提供交易入口、业务存储或事件。
//! 后续逐步实现选举业务规则，并接入 electionvote 投票引擎。

#![cfg_attr(not(feature = "std"), no_std)]

// Cargo库入口与FRAME定义分离，唯一公开出口保持原形状。
mod election;
pub use election::pallet;
pub use pallet::*;

#[cfg(test)]
#[path = "../tests/mod.rs"]
mod tests;
