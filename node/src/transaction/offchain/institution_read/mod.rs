//! 清算行机构身份只读子模块。
//!
//!
//! - 清算行结算依赖机构身份事实(机构最小集、账户余额、管理员集合、动态阈值),节点作为全节点直读链上。
//! - 本子模块只读不写:机构创建归 onchina 控制台,节点不构建/提交 propose_create_institution。
//! - `cid.rs` 拉身份注册局端候选与注册凭证;`chain.rs` 读链上机构最小集并派生主/费账户、汇总管理员与阈值。

pub mod chain;
pub mod cid;
pub mod commands;
pub mod types;
