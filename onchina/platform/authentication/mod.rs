/// 敏感动作冷钱包扫码签名工具(PasskeyColdSign 档)。
#[path = "action_sign.rs"]
pub mod action_sign;
/// 统一管理员安全动作入口；注册局办理调用所属模块，原有凭证规则保持不变。
#[path = "actions.rs"]
pub mod actions;
/// 平台管理员登录认证能力。
#[path = "login.rs"]
pub mod login;
/// 平台登录管理员元数据；注册局名册 DTO 归注册局。
#[path = "model.rs"]
pub mod model;
/// 管理端操作权限分级唯一入口。
#[path = "operation_auth.rs"]
pub mod operation_auth;
/// WebAuthn passkey 鉴权(Passkey / PasskeyColdSign 档 step-up)。
#[path = "passkey.rs"]
pub mod passkey;
/// 管理员结构化表读写，唯一持久化入口。
#[path = "repo.rs"]
pub mod repo;
/// 管理员一次性安全授权模型(扫码签名挑战与 grant)。
#[path = "security_model.rs"]
pub mod security_model;
