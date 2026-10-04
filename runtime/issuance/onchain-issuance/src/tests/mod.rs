//! 单元测试入口(框架阶段占位)。
//!
//! 沿用 `project_pallet_tests_restructured_2026_05_07` 样板:
//!
//! - `mod.rs`        — 本文件,挂载 mock runtime + 共用 fixtures
//! - `cases.rs`      — 业务路径测试(issue/mint/burn/close/transfer)
//! - `monitor.rs`    — NRC 监管 5 动作测试
//! - `blacklist.rs`  — 字符串黑名单 hit/miss 测试
//!
//! 当前框架阶段只挂占位文件,实装在后续任务卡 A/B 完成。

mod blacklist;
mod cases;
mod monitor;
