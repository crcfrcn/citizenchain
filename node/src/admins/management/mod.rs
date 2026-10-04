// 管理员管理桌面端后端模块。
//
// 本目录只承载机构管理员账户、entity 岗位任职读取和管理员账户激活。
// 机构管理员变化只能由治理业务结果驱动，Node 不构造管理员集合变更调用。

pub mod account_id;
pub mod activation;
pub mod codec;
pub mod commands;
pub mod storage;
pub mod types;
