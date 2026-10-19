//! 镇下地址管理模块。
//!
//! 本模块读取随包 `china.sqlite.addresses`,并构造地址变更链上 call data。
//! 不在这里维护第二份地址主数据,也不直接提交链交易。

#[path = "chain_call.rs"]
pub(crate) mod chain_call;
#[path = "handler.rs"]
pub(crate) mod handler;
#[path = "model.rs"]
pub(crate) mod model;
#[path = "repo.rs"]
pub(crate) mod repo;
#[path = "version.rs"]
pub(crate) mod version;
