//! 区块链交易索引模块。
//!
//! 后台持续扫描链上区块，解析所有余额变动事件，写入 PostgreSQL `tx_records` 表，
//! 并通过 API 暴露给 CitizenApp 查询完整的钱包交易记录。

#[path = "api.rs"]
pub(crate) mod api;
#[path = "db.rs"]
pub(crate) mod db;
#[path = "event_parser.rs"]
pub(crate) mod event_parser;
#[path = "worker.rs"]
pub(crate) mod worker;

pub(crate) use worker::{indexer_worker, bound_institution_projection_worker};
