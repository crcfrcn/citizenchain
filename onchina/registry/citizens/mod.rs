//! 公民身份业务模块。
//!
//! 公民电子护照档案、状态扫码、链上身份推送等业务统一收口在此目录。
//! 公民模型和公民查询 handler 归属本目录。

/// 注册局直接录入公民并直接发护照入口。
#[path = "admin_entry.rs"]
pub(crate) mod admin_entry;
/// 公民钱包签名与 identity 上链二维码准备入口。
#[path = "chain_identity.rs"]
pub(crate) mod chain_identity;
/// 公民模块联合投票本地人数查询接口。
#[path = "chain_joint_vote.rs"]
pub(crate) mod chain_joint_vote;
/// 投票资格提示查询接口。
#[path = "chain_vote.rs"]
pub(crate) mod chain_vote;
#[path = "handler.rs"]
pub(crate) mod handler;
#[path = "model.rs"]
pub(crate) mod model;
#[path = "occupy.rs"]
pub(crate) mod occupy;
/// OnChina 自持的护照号与护照有效期生成逻辑。
#[path = "../passports/number.rs"]
pub(crate) mod passport_no;
