//! 链上中国平台管理员登录认证模块。
//!
//! 对外暴露管理员会话与扫码登录；内部按模型、扫码登录、鉴权守卫、
//! 签名工具拆分,避免认证链路继续集中在单个超大文件。

#[path = "login_guards.rs"]
mod guards;
#[path = "login_handler.rs"]
mod handler;
#[path = "login_model.rs"]
mod model;
#[path = "login_onchain_gate.rs"]
mod onchain_gate;
#[path = "login_qr_login.rs"]
mod qr_login;
#[path = "login_signature.rs"]
mod signature;

const LOGIN_SIGN_REQUEST_TTL_SECONDS: i64 = 90;

pub(crate) use guards::require_admin_any;
pub(crate) use handler::{
    admin_auth_check, admin_auth_confirm_node_binding, admin_logout,
    require_admin_session_middleware,
};
pub(crate) use model::{
    AdminAuthContext, AdminInstitutionCandidate, AdminSession, LoginSignRequest,
    NodeBindingChallenge, NodeInstitutionBinding, QrLoginResultRecord,
};
pub(crate) use onchain_gate::revoke_stale_admin_sessions_loop;
pub(crate) use qr_login::{
    admin_auth_qr_complete, admin_auth_qr_result, admin_auth_qr_sign_request,
};
pub(crate) use signature::verify_admin_signature_bytes;
pub(crate) use signature::{admin_person_names, parse_account_id_bytes};
