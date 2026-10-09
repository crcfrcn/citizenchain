// 管理员管理桌面端后端模块。
//
// 本目录只承载机构管理员账户、entity 岗位任职读取和管理员账户激活。
// 机构管理员变化只能由治理业务结果驱动，Node 不构造管理员集合变更调用。

#[path = "management_account_id.rs"]
pub mod account_id;
#[path = "management_activation.rs"]
pub mod activation;
#[path = "management_codec.rs"]
pub mod codec;
#[path = "management_commands.rs"]
pub mod commands;
#[path = "management_storage.rs"]
pub mod storage;
#[path = "management_types.rs"]
pub mod types;
