//! 链上中国公共库：宿主和独立业务通过本库公开接口复用唯一公共实现。
//! CID、省市解析、权限与业务规则保持原实现。
use axum::{
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use base64::Engine as _;
use chrono::{DateTime, Utc};
use postgres::config::Host;
use serde::Serialize;
use std::{net::SocketAddr, sync::Arc};
use tracing::{info, warn};
use uuid::Uuid;

#[path = "../audit/handler.rs"]
pub mod audit;
#[path = "../authentication/mod.rs"]
pub mod authentication;
#[path = "../client/mod.rs"]
mod citizenapp;
#[path = "../codes/mod.rs"]
pub mod codes;

#[path = "../audit/runtime.rs"]
pub mod audit_runtime;
/// 公民 CID finalized 六读闭环快照；注册局办理、查询和投影共用。
#[path = "../chain/identity.rs"]
pub mod chain_identity;
/// 跨业务复用的链上凭证签名、SCALE payload 与 genesis hash 对齐工具。
#[path = "../chain/runtime.rs"]
pub mod chain_runtime;
#[path = "../transactions/submit.rs"]
pub mod chain_submit;
/// 链 RPC URL 统一读取入口,业务模块不得直接读环境变量。
#[path = "../chain/url.rs"]
pub mod chain_url;
/// PostgreSQL 连接池和当前结构化 schema 初始化。
#[path = "../database/connection.rs"]
pub mod database;
/// 内嵌私有 PostgreSQL 生命周期(onchina 自管;Card 05 零依赖部署)。
#[path = "../database/embedded.rs"]
pub(crate) mod database_embedded;
/// 机构治理裸 SCALE call data 编码器（OnChina 唯一真源）。
#[path = "../institutions/chain_call.rs"]
pub mod institutions_call;
/// QR_V1 协议和链上中国平台签名二维码构造。
#[path = "../qr/mod.rs"]
pub mod qr;
/// HTTP API 通用响应、分页和健康检查输出模型。
#[path = "../communication/response.rs"]
pub mod response;
#[path = "../security/http.rs"]
pub(crate) mod security_http;
/// onchina 内网 API 机构私有 CA TLS(Card 05;rcgen CA 签发 + rustls)。
#[path = "../security/tls.rs"]
pub(crate) mod tls;

#[path = "../authorization/mod.rs"]
pub mod authorization;
#[path = "../security/crypto.rs"]
pub mod crypto;
#[path = "../discovery/mdns.rs"]
mod discovery;
#[path = "../documents/mod.rs"]
pub mod documents;
#[path = "../indexer/mod.rs"]
pub mod indexer;
#[path = "../institutions/mod.rs"]
pub mod institutions;
#[path = "mod.rs"]
pub mod modules;
#[path = "../transactions/mod.rs"]
pub mod transactions;
#[path = "../workspace/mod.rs"]
pub mod workspace;

use crate::codes::model::*;
pub use crate::database::Db;
pub use crate::response::*;
pub use crate::security_http::*;
pub use authentication::login::{parse_account_id_bytes, require_admin_any};
pub use authentication::model::*;
pub use institutions::model::*;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    rate_limiter: Arc<LocalRateLimiter>,
    pub modules: Arc<modules::ModuleCatalog>,
}
include!("../communication/models.rs");
include!("../database/records.rs");
include!("../database/maintenance.rs");
include!("../security/server.rs");
include!("../communication/error.rs");
include!("../communication/routes.rs");

// 公共库测试选择宿主；业务包在所属测试构造实际模块。
#[cfg(test)]
use testing::local_module;

/// 仅开发测试使用，正式平台和模块不编入测试入口。
#[cfg(any(test, feature = "test-support"))]
#[path = "../tests/support.rs"]
pub mod testing;
