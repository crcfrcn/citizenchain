//! 区块链交易索引模块。
//!
//! 后台持续扫描链上区块，解析所有余额变动事件，写入 PostgreSQL `tx_records` 表，
//! 并通过 API 暴露给 CitizenApp 查询完整的钱包交易记录。

#[path = "api.rs"]
pub mod api;
#[path = "db.rs"]
pub mod db;
#[path = "event_parser.rs"]
pub mod event_parser;
#[path = "worker.rs"]
pub mod worker;

pub(crate) use worker::{bound_institution_projection_worker, indexer_worker};
