//! nodeproposal所属功能与公开接口。

pub mod business;
pub mod commands;
pub mod display;
pub mod query;
pub mod tally;
pub mod types;
pub mod vote;

#[cfg(test)]
#[path = "../tests/integration.rs"]
mod integration_tests;
