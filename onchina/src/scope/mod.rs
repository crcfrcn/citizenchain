//! 机构按行政层级的可见范围过滤
//!
//! 此模块提供范围规则派生;作用域过滤已下沉到 SQL 层(`*_in_scope` 查询按 `VisibleScope`
//! 直接约束 WHERE),不再"取全量再 Rust 过滤"。业务 handler 只需:
//! 1. `let ctx = require_admin_any(...)?;`
//! 2. `let scope = scope::get_visible_scope(&ctx);` 传给 SQL 层收窄查询。
//!
//! 五档范围(按机构 admin_level 派生):
//! - 全国(NATIONAL,部委等)        → 不限省/市/镇
//! - 省级(PROVINCE / 联邦注册局)   → 本省,所有市
//! - 市级(CITY)                    → 本市,所有镇
//! - 镇级(TOWN)                    → 本镇
//! - 自机构(私权法人/非法人,无层级)→ 暂沿用本市范围
//!
//! 详细规则见 `rules.rs` 的 `VisibleScope`。
//!
//! 本目录只保留范围规则；HTTP handler、账户与公钥工具归属对应业务模块。

pub mod rules;

pub use rules::get_visible_scope;
