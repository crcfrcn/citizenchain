//! 导出按机构行政层级派生的可见范围规则。

#[path = "rules.rs"]
pub mod rules;

pub use rules::get_visible_scope;
