//! 组织链交易后台索引、结构化记录及公民端查询接口。

#[path = "api.rs"]
pub mod api;
#[path = "db.rs"]
pub mod db;
#[path = "event_parser.rs"]
pub mod event_parser;
#[path = "worker.rs"]
pub mod worker;

pub(crate) use worker::{bound_institution_projection_worker, indexer_worker};
