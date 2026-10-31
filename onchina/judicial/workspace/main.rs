//! judicial独立Cargo包入口，仅链接平台公开库和本包业务。
use onchina::modules;
use std::sync::Arc;

mod module;
#[path = "mod.rs"]
pub(crate) mod workspace;
fn local_module() -> Option<Arc<dyn modules::model::ModuleBusiness>> {
    Some(Arc::new(module::Module::default()))
}
fn main() {
    onchina::run_backend(local_module());
}
