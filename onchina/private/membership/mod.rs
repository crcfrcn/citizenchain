//! 平台会员价格治理域。
//!
//! 本域只读取 finalized `SquarePost` 真源、校验公民链基金会准确 CID，并构造现有
//! `propose_set_platform_price` 冷签交易。投票资格、计票与执行仍完全归统一投票引擎。

#[path = "call.rs"]
pub(crate) mod chain_call;
#[path = "handler.rs"]
pub(crate) mod handler;

/// 统一链冷签会话用途；提交仍走 `onchina::chain_submit` 唯一路径。
pub(crate) const PURPOSE_PLATFORM_PRICE_PROPOSAL: &str = "PLATFORM_PRICE_PROPOSAL";
