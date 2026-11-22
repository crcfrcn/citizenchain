//! 保留选举pallet的Runtime位置，当前不提供业务调用、存储或事件。

#![cfg_attr(not(feature = "std"), no_std)]

// Cargo库入口与FRAME定义分离，唯一公开出口保持原形状。
mod election;
pub use election::pallet;
pub use pallet::*;

#[cfg(test)]
#[path = "../tests/mod.rs"]
mod tests;
