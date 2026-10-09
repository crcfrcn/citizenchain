//! 镇下地址管理模块。
//!
//! 本模块读取随包 `china.sqlite.addresses`,并构造地址变更链上 call data。
//! 不在这里维护第二份地址主数据,也不直接提交链交易。

pub(crate) mod chain_call;
pub(crate) mod handler;
pub(crate) mod model;
pub(crate) mod repo;
pub(crate) mod version;
