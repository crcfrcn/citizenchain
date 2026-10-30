//! 投票引擎对外 trait 门面。
//!
//! 具体职责按引擎入口、资格提供者、业务回调、超时终结和清理拆分；
//! 本文件统一 re-export，保持 runtime 与业务 pallet 的既有引用路径不变。

mod callbacks;
mod cleanup;
mod engines;
mod finalizers;
mod providers;

pub use callbacks::*;
pub use cleanup::*;
pub use engines::*;
pub use finalizers::*;
pub use providers::*;
