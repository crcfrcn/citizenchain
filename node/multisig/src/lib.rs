//! 组织多签转账提案、冷签命令及公开详情。

pub mod account;
pub mod commands;
pub mod proposal;
pub mod signing;
pub mod types;

#[cfg(test)]
#[path = "../tests/integration.rs"]
mod integration_tests;
