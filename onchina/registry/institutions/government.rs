//! 公权机构模块。
//!
//! 前后端统一使用 `gov` 命名。公权机构唯一真源是链上 PublicManage,
//! 本模块只负责链投影缓存、列表/API 展示和链上目录验收;不得从行政区本地生成机构。

#[path = "government_chain_audit.rs"]
pub(crate) mod chain_audit;
#[path = "government_handler.rs"]
pub(crate) mod handler;
#[path = "government_service.rs"]
pub(crate) mod service;
