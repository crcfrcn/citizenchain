//! 节点核心服务与应用装配。

pub mod application;
#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarks;
pub mod bootnodes;
pub mod chain;
pub mod cli;
pub mod command;
pub mod ethereum;
pub mod initialization;
pub mod lifecycle;
pub mod platform;
pub mod rpc;
pub mod runner;
pub mod service;

#[cfg(test)]
#[path = "tests/e2e.rs"]
mod e2e_tests;
