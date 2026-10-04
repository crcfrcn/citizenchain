//! OnChina 后端低层加密工具集。
//!
//! 本目录承载与具体业务无关的公钥规范化等低层加密辅助,放在业务模块
//! (admins / institutions / citizens) 之下,避免业务模块互相依赖,业务模块不得重复实现。

pub(crate) mod pubkey;
