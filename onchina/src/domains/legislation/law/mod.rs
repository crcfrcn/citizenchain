//! 法律案(本轮实现):章节条款提案 + 院内/两院表决 + 签署。
//!
//! Phase 1 增量交付裸 SCALE call-data 编码器——
//! - `chain_propose`:`propose_enact/amend/repeal_law`(pallet 25);
//! - `chain_vote`：`cast_representative_vote` 等表决/签署（pallet 26）。
//! 复用 `core::institution_call` 和 `core::chain_submit` 的「构造 call data → CitizenWallet 一次签名
//! → OnChina 回扫响应二维码并统一提交」通道，
//! 逐字节交叉校验链端 SCALE。HTTP DTO(model)、`service` 组织提案数据、链读(`chain_read`)、
//! 冷签 prepare/commit 随 Phase 1B 后续增量落地。

/// 立法链签名准备——组织 ChainCall → 统一短期签名会话。
pub(crate) mod action;
pub(crate) mod chain_propose;
/// 法律链读——链上 Law/LawVersion SCALE 解码镜像 + → 展示 DTO。
pub(crate) mod chain_read;
pub(crate) mod chain_vote;
/// 法律案 HTTP DTO + 章节条款 ↔ 链编码器入参转换 + 读模型。
pub(crate) mod model;
/// 法律案宪法路由(层级×是否教育 → houses/executive/legislature 机构码)。
pub(crate) mod routing;
/// 法律案提案组织(请求 + 路由 + 账户解析 → call-data)+ 写入边界 scope 前置。
pub(crate) mod service;
