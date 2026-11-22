//! 组织平台会员价格的最终链读取、基金会身份校验和调价提案准备。

#[path = "call.rs"]
pub(crate) mod chain_call;
#[path = "handler.rs"]
pub(crate) mod handler;

/// 统一链冷签会话用途；提交仍走 `onchina::chain_submit` 唯一路径。
pub(crate) const PURPOSE_PLATFORM_PRICE_PROPOSAL: &str = "PLATFORM_PRICE_PROPOSAL";
