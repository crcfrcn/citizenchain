//! 平台通用链签会话和业务提交入口；保持现有单进程调用方式。
#[path = "handler.rs"]
pub mod handler;
#[path = "session.rs"]
pub mod session;
