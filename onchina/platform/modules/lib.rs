//! 链上中国公共库：宿主和独立业务通过本库公开接口复用唯一公共实现。
//! CID、省市解析、权限与业务规则保持原实现。
use axum::{
    http::StatusCode,
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
/// 公信 HTTPS 证书申请、续期和热加载。
#[path = "../security/tls.rs"]
pub(crate) mod tls;

#[path = "../authorization/mod.rs"]
pub mod authorization;
#[path = "../security/crypto.rs"]
pub mod crypto;
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

/// 固定部署唯一真源；宿主通过 --describe 向节点公开，不由调用方复制配置。
pub const PLATFORM_HOST: &str = "onchina.crcfrcn.com";
/// 本产品只监听用户一次配置的固定局域网主机地址。
pub const PLATFORM_ADDRESS: SocketAddr = SocketAddr::new(
    std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 20, 27)),
    1988,
);
/// 浏览器、Passkey 与调用方共用的完整 HTTPS 入口。
pub fn platform_url() -> String {
    format!("https://{}:{}/", PLATFORM_HOST, PLATFORM_ADDRESS.port())
}

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
