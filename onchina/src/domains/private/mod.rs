//! 私权机构模块。
//!
//! 私权机构继续保留 `private` 一级边界,内部按个体经营、合伙企业、
//! 股权公司、股份公司、公益组织、注册协会拆分。身份 ID 格式不变,私权类型只决定
//! `主体属性 + T2 机构码` 的目标组合。

pub(crate) mod association;
pub(crate) mod common;
pub(crate) mod company;
pub(crate) mod corporation;
pub(crate) mod participants;
pub(crate) mod partnership;
pub(crate) mod sole;
pub(crate) mod welfare;
