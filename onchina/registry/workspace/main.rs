//! registry独立Cargo包入口，仅链接平台公开库和本包业务。
use axum::{
    http::StatusCode,
    routing::{delete, get, post},
    Json,
};
use chrono::DateTime;
use onchina::*;
use onchina::{authentication, authorization, modules, transactions};
use std::sync::Arc;

pub(crate) use admins::model::*;
pub(crate) use citizens::model::*;
pub(crate) use documents::model::*;

#[path = "../address/mod.rs"]
pub(crate) mod address;

#[path = "../citizens/mod.rs"]
pub(crate) mod citizens;

#[path = "../documents/mod.rs"]
pub(crate) mod documents;

#[path = "../institutions/mod.rs"]
pub(crate) mod institutions;

#[path = "../projection/mod.rs"]
pub(crate) mod projection;

#[path = "mod.rs"]
pub(crate) mod workspace;

#[path = "../admins/mod.rs"]
pub(crate) mod admins;
mod module;
fn local_module() -> Option<Arc<dyn modules::model::ModuleBusiness>> {
    Some(Arc::new(module::Module::default()))
}
fn main() {
    onchina::run_backend(local_module());
}

// 注册局专属数据库操作只由本包扩展trait提供。

use tracing::{info, warn};
