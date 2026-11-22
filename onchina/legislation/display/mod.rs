//! 组织只读立法大屏的链数据、看板装配和公开接口。

#[path = "chain_read.rs"]
pub(crate) mod chain_read;
/// 免登录只读 HTTP 入口(`GET /api/public/legislation/display/board`)。
#[path = "handler.rs"]
pub(crate) mod handler;
/// 大屏看板 DTO(机构头 + 名册 + 每个活跃提案的进度与席位)。
#[path = "model.rs"]
pub(crate) mod model;
/// 名册 × 活跃提案 × 逐席投票的聚合装配。
#[path = "service.rs"]
pub(crate) mod service;
