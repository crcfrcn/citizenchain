//! 节点核心模块。
//!
//! 收口原生节点 CLI、chain spec、RPC、服务工厂、benchmark 与网络证书能力。

pub(crate) mod benchmarking;
pub(crate) mod chain_spec;
pub(crate) mod cli;
pub(crate) mod command;
pub(crate) mod constitution;
pub(crate) mod grandpa_rotation;
pub(crate) mod node_guard;
pub(crate) mod rpc;
pub(crate) mod service;
pub(crate) mod tls_cert;
