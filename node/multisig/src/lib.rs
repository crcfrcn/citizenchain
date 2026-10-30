//! nodemultisig所属功能与公开接口。

pub mod account;
pub mod commands;
pub mod proposal;
pub mod signing;
pub mod types;

#[cfg(test)]
#[path = "../tests/integration.rs"]
mod integration_tests;
