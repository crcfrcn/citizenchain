//! 私权机构多签业务分区。
//!
//! 私权机构多签由 CID 机构账户凭证发起,业务字段绑定 cid_number + account_name,
//! 链上派生地址 `derive_institution_account(cid_number, account_name)`。
//! 本子模块包含机构级类型、治理、关闭和账户表。
//! 投票终态回调统一由 lib.rs 的 `InternalVoteExecutor` 接收。

#[cfg(test)]
pub mod accounts;
pub mod governance;
pub mod maintain;
pub mod role;
pub mod types;

pub use types::*;
