//! 机构账户模块。
//!
//! 默认账户、制度账户和新增账户名称都归这里;链交互文件仍遵守
//! `chain_` 前缀规则,不新建独立链目录。

pub(crate) mod derive;
pub(crate) mod handler;
