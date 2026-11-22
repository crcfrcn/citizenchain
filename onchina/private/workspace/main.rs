//! 启动独立私权机构服务并连接平台公共接口。

use axum::routing::{get, post};
use onchina::*;
use onchina::{authentication, modules, transactions};
use std::sync::Arc;

#[path = "../membership/mod.rs"]
pub(crate) mod membership;

mod module;
fn local_module() -> Option<Arc<dyn modules::model::ModuleBusiness>> {
    Some(Arc::new(module::Module::default()))
}
fn main() {
    onchina::run_backend(local_module());
}
