//! 组织管理员登录模型、二维码认证、会话守卫及签名工具。

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

pub(crate) use guards::{admin_session, bearer_token, validate_current_wallet};
pub use guards::{require_admin_any, require_admin_session};
pub(crate) use handler::{
    admin_auth_check, admin_auth_confirm_node_binding, admin_logout,
    require_admin_session_middleware,
};
pub use model::{AdminAuthContext, NodeInstitutionBinding};
pub(crate) use model::{
    AdminInstitutionCandidate, AdminSession, LoginSignRequest, NodeBindingChallenge,
    QrLoginResultRecord,
};
pub(crate) use onchain_gate::{revoke_stale_admin_sessions_loop, revoke_stale_admin_sessions_once};
pub(crate) use qr_login::{
    admin_auth_qr_complete, admin_auth_qr_result, admin_auth_qr_sign_request,
};
pub(crate) use signature::verify_admin_signature_bytes;
pub use signature::{admin_person_names, parse_account_id_bytes};

// 既有测试目录直接调用原挑战创建函数，生产接口不增加旁路。
#[cfg(test)]
pub(crate) use qr_login::create_login_request;
