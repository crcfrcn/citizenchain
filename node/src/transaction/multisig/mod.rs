//! 多签转账桌面端模块。
//!
//! 该模块只承载 `MultisigTransfer` pallet 对应的签名请求和提交命令；
//! 治理、清算行和管理员管理不在这里实现。

pub mod account_id;
pub mod commands;
pub mod proposal;
pub mod signing;
