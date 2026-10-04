//! 清算行 offchain 结算引擎子模块。
//!
//!
//! - 本目录只承载清算行节点运行后的结算链路:监听、打包、签名、提交。
//! - 机构身份只读(CID 查询、机构详情)在 `offchain::institution_read`,
//!   注册清算行节点留在 `offchain`;本目录只保留结算前管理员解锁与批次上链。

pub mod admin_unlock;
pub(crate) mod bootstrap;
pub(crate) mod commands;
pub mod keystore;
pub mod listener;
pub mod packer;
pub mod reserve;
pub mod signer;
pub mod submitter;
