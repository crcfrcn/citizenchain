//! 组织法律提案、表决、签署及链上法律读取。

#[path = "action.rs"]
pub(crate) mod action;
#[path = "propose.rs"]
pub(crate) mod chain_propose;
/// 法律链读——链上 Law/LawVersion SCALE 解码镜像 + → 展示 DTO。
#[path = "read.rs"]
pub(crate) mod chain_read;
#[path = "voting.rs"]
pub(crate) mod chain_vote;
/// 法律案 HTTP DTO + 章节条款 ↔ 链编码器入参转换 + 读模型。
#[path = "model.rs"]
pub(crate) mod model;
/// 法律案宪法路由(层级×是否教育 → houses/executive/legislature 机构码)。
#[path = "routing.rs"]
pub(crate) mod routing;
/// 法律案提案组织(请求 + 路由 + 账户解析 → call-data)+ 写入边界 scope 前置。
#[path = "service.rs"]
pub(crate) mod service;
