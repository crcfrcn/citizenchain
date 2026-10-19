// 宿主及各模块复用同一装配源；逻辑命名空间保持，CID与省市解析源码不改。
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

#[path = "audit/handler.rs"]
mod audit;
#[path = "authentication/mod.rs"]
mod authentication;
#[path = "codes/mod.rs"]
mod cid;
#[path = "client/mod.rs"]
mod citizenapp;
#[path = "."]
mod core {
/// 公民 CID finalized 六读闭环快照；注册局办理、查询和投影共用。
#[path = "chain/citizen_identity.rs"]
pub(crate) mod chain_citizen_identity;
/// 跨业务复用的链上凭证签名、SCALE payload 与 genesis hash 对齐工具。
#[path = "chain/runtime.rs"]
pub(crate) mod chain_runtime;
#[path = "transactions/submit.rs"]
pub(crate) mod chain_submit;
/// 链 RPC URL 统一读取入口,业务模块不得直接读环境变量。
#[path = "chain/url.rs"]
pub(crate) mod chain_url;
/// PostgreSQL 连接池和当前结构化 schema 初始化。
#[path = "database/connection.rs"]
pub(crate) mod db;
/// 内嵌私有 PostgreSQL 生命周期(onchina 自管;Card 05 零依赖部署)。
#[path = "database/embedded.rs"]
pub(crate) mod embedded_pg;
#[path = "security/http.rs"]
pub(crate) mod http_security;
/// 机构治理裸 SCALE call data 编码器（OnChina 唯一真源）。
#[path = "institutions/chain_call.rs"]
pub(crate) mod institution_call;
/// QR_V1 协议和链上中国平台签名二维码构造。
#[path = "qr/mod.rs"]
pub(crate) mod qr;
/// HTTP API 通用响应、分页和健康检查输出模型。
#[path = "communication/response.rs"]
pub(crate) mod response;
#[path = "audit/runtime.rs"]
pub(crate) mod runtime_ops;
/// onchina 内网 API 机构私有 CA TLS(Card 05;rcgen CA 签发 + rustls)。
#[path = "security/tls.rs"]
pub(crate) mod tls;

}
#[path = "security/crypto.rs"]
mod crypto;
#[path = "documents/mod.rs"]
mod documents;
#[path = "indexer/mod.rs"]
mod indexer;
#[path = "transactions/mod.rs"]
mod transactions;
#[path = "institutions/mod.rs"]
mod institutions;
#[path = "modules/mod.rs"]
mod modules;
#[path = "discovery/mdns.rs"]
mod discovery;
#[path = "authorization/mod.rs"]
mod authorization;
#[path = "workspace/mod.rs"]
mod workspace;

pub(crate) use crate::core::db::Db;
pub(crate) use crate::core::http_security::*;
pub(crate) use crate::core::response::*;
pub(crate) use authentication::login::{parse_account_id_bytes, require_admin_any};
pub(crate) use authentication::model::*;
pub(crate) use institutions::model::*;
pub(crate) use cid::model::*;


#[derive(Clone)]
struct AppState {
    db: Db,
    rate_limiter: Arc<LocalRateLimiter>,
    modules: Arc<modules::ModuleCatalog>,
}
include!("communication/models.rs");
include!("database/records.rs");
include!("database/maintenance.rs");
include!("security/server.rs");
include!("communication/error.rs");
include!("communication/routes.rs");
