//! private独立Cargo包入口，仅链接平台公开库和本包业务。
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
