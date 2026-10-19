//! 私权机构模块。
//!
//! 私权机构继续保留 `private` 一级边界,内部按个体经营、合伙企业、
//! 股权公司、股份公司、公益组织、注册协会拆分。身份 ID 格式不变,私权类型只决定
//! `主体属性 + T2 机构码` 的目标组合。

#[path = "private_association.rs"]
pub(crate) mod association;
#[path = "private_common.rs"]
pub(crate) mod common;
#[path = "private_company.rs"]
pub(crate) mod company;
#[path = "private_corporation.rs"]
pub(crate) mod corporation;
#[path = "private_participants.rs"]
pub(crate) mod participants;
#[path = "private_partnership.rs"]
pub(crate) mod partnership;
#[path = "private_sole.rs"]
pub(crate) mod sole;
#[path = "private_welfare.rs"]
pub(crate) mod welfare;
