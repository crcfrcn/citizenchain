//! 组织公民档案、电子护照、CID办理和链上身份推送。

#[path = "admin_entry.rs"]
pub(crate) mod admin_entry;
/// 公民钱包签名与 identity 上链二维码准备入口。
#[path = "identity.rs"]
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
#[path = "passport.rs"]
pub(crate) mod passport_no;

/// 公民业务的自动授权、内部岗位确定与签名前复查。
pub(crate) mod authorization;
