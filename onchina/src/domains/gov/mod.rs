//! 公权机构模块。
//!
//! 前后端统一使用 `gov` 命名。公权机构唯一真源是链上 PublicManage,
//! 本模块只负责链投影缓存、列表/API 展示和链上目录验收;不得从行政区本地生成机构。

pub(crate) mod chain_audit;
pub(crate) mod handler;
pub(crate) mod service;
