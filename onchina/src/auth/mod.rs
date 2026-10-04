/// 敏感动作冷钱包扫码签名工具(PasskeyColdSign 档)。
pub(crate) mod action_sign;
/// 管理员安全动作入口:注册局管理员治理直接 apply,业务写操作签发一次性 grant。
pub(crate) mod actions;
pub(crate) mod catalog;
pub(crate) mod city_registry_admins;
/// 管理员登录认证能力,归入 admins 边界。
pub(crate) mod login;
/// 联邦注册局管理员/市注册局管理员实体、角色和列表 DTO。
pub(crate) mod model;
/// 管理端操作权限分级唯一入口。
pub(crate) mod operation_auth;
/// WebAuthn passkey 鉴权(Passkey / PasskeyColdSign 档 step-up)。
pub(crate) mod passkey;
/// 管理员结构化表读写，唯一持久化入口。
pub(crate) mod repo;
/// 管理员一次性安全授权模型(扫码签名挑战与 grant)。
pub(crate) mod security_model;

pub(crate) use catalog::get_own_institution;
pub(crate) use catalog::list_federal_registry_admins;
pub(crate) use catalog::list_own_institution_admins;
pub(crate) use city_registry_admins::list_city_registry_admins;
