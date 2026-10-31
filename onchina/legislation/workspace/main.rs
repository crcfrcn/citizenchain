//! legislation独立Cargo包入口，仅链接平台公开库和本包业务。
use axum::routing::{get, post};
use onchina::*;
use onchina::{authentication, modules, transactions};
use std::sync::Arc;

/// 预算案（类>款>项>目）字段 schema；未来由预算业务模块接入代表机构表决。
#[path = "../budget/mod.rs"]
pub(crate) mod budget;

#[path = "../proposals/category.rs"]
pub(crate) mod category;
/// 提案进度只读投影（Proposal + 代表/法律元数据 + tally）。
#[path = "../proposals/chain_read_proposal.rs"]
pub(crate) mod chain_read_proposal;
/// 大屏只读看板(Phase 3)——本节点机构名册 × 活跃提案 × 逐席投票聚合,免登录只读。
#[path = "../display/mod.rs"]
pub(crate) mod display;
/// 立法与表决 HTTP handler(/api/legislation/* 发起/表决/读法律/读提案)。
#[path = "../proposals/handler.rs"]
pub(crate) mod handler;
/// 法律案——章节条款提案 + 院内/两院表决 + 签署的链交互编码器(Phase 1)。
#[path = "../laws/mod.rs"]
pub(crate) mod law;

#[path = "../proposals/model.rs"]
pub(crate) mod model;
/// 任免案（人事任免职书）字段 schema；未来由任免业务模块接入代表机构表决。
#[path = "../personnel/mod.rs"]
pub(crate) mod personnel;

mod module;
#[path = "mod.rs"]
pub(crate) mod workspace;
fn local_module() -> Option<Arc<dyn modules::model::ModuleBusiness>> {
    Some(Arc::new(module::Module::default()))
}
fn main() {
    onchina::run_backend(local_module());
}
