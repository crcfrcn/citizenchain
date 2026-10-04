//! 交易模块桌面端聚合目录。
//!
//! 与 runtime/transaction/ 边界对齐，承载链上链下交易、机构多签账户、多签交易相关功能。
//! multisig：多签转账模块，用于机构/个人的多签转账；
//! offchain：链下支付模块，与清算行系统对接的个人链下支付方；
//! onchain：链上支付模块，统一的链上支付；

pub mod multisig;
pub mod offchain;
pub mod onchain;
