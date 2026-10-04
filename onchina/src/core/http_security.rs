use axum::{
    extract::{ConnectInfo, Request, State},
    http::{header::HeaderName, HeaderMap, HeaderValue, Method, StatusCode},
    middleware,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use std::{
    net::{IpAddr, SocketAddr},
    sync::OnceLock,
};
use tower_http::cors::{Any, CorsLayer};

use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};

use crate::*;

type Blake2b256 = Blake2b<U32>;

static TRUSTED_PROXY_IPS: OnceLock<Vec<IpAddr>> = OnceLock::new();
const RATE_LIMIT_WINDOW_MS: i64 = 60_000;

/// 进程内滑动窗口限流器。
///
/// 去中心化后 OnChina 是每节点本地服务,限流落本地内存即可,无需外部 Redis。
/// 按 actor 哈希分桶,窗口内记录命中时间戳,超过 `limit_per_min` 即拒绝。
#[derive(Default)]
pub(crate) struct LocalRateLimiter {
    buckets: dashmap::DashMap<String, std::collections::VecDeque<i64>>,
}

impl LocalRateLimiter {
    /// 新建空限流器。
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// 尝试为某 actor 占用一个时间片;窗口内已达上限时返回 false。
    pub(crate) fn try_acquire(&self, actor_hash: &str, limit_per_min: usize, now_ms: i64) -> bool {
        if limit_per_min == 0 {
            return false;
        }
        let cutoff = now_ms - RATE_LIMIT_WINDOW_MS;
        let mut slots = self.buckets.entry(actor_hash.to_string()).or_default();
        // 清掉滑出窗口的旧时间戳。
        while let Some(&front) = slots.front() {
            if front <= cutoff {
                slots.pop_front();
            } else {
                break;
            }
        }
        if slots.len() >= limit_per_min {
            return false;
        }
        slots.push_back(now_ms);
        true
    }
}

pub(crate) async fn global_rate_limit_middleware(
    State(state): State<AppState>,
    request: Request,
    next: middleware::Next,
) -> Response {
    let now = Utc::now();
    let limit_per_min = std::env::var("ONCHINA_RATE_LIMIT_PER_MIN")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(120);
    let actor = actor_ip_from_request(&request)
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let now_ms = now.timestamp_millis();
    let actor_hash = hex::encode(Blake2b256::digest(actor.as_bytes()));
    if !state
        .rate_limiter
        .try_acquire(actor_hash.as_str(), limit_per_min, now_ms)
    {
        return api_error(StatusCode::TOO_MANY_REQUESTS, 1029, "rate limit exceeded");
    }

    next.run(request).await
}

/// 带内容哈希的前端产物目录前缀。必须带尾斜杠:写成 `/assets` 会让 `/assetsfoo`
/// 之类路径误命中永久缓存档。
const HASHED_ASSET_PREFIX: &str = "/assets/";

/// 内容哈希产物:内容变则文件名变,永久缓存零风险。
const CACHE_IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// 非哈希资源:允许缓存但每次必须校验。配 `ServeDir` 已发的 `Last-Modified`
/// 走条件请求,未变即 304、响应体 0 字节;不用 `no-store` 白传全量。
const CACHE_REVALIDATE: &str = "no-cache";

/// 静态资源缓存策略单源。
///
/// `index.html` 是内容哈希的唯一索引,一旦被缓存整套 vite 哈希失效机制就废掉:
/// 客户端永远拿不到新的 `assets/index-<hash>.js` 引用。`ServeDir` 本身不发
/// `Cache-Control`,浏览器按 RFC 9111 走启发式缓存(新鲜期 ≈ `(Date − Last-Modified) × 10%`),
/// 因此必须显式声明。
///
/// 兜底档是 `no-cache` 而非永久缓存,这是刻意的 fail-safe:将来 dist 里出现
/// `favicon.ico`、`robots.txt` 等**不带内容哈希**的资源时默认保守,只有明确带哈希的
/// 路径才升级。反向配置(默认永久 + 白名单校验)漏一个就永久钉死客户端,代价不对称。
pub(crate) fn static_cache_control(path: &str) -> &'static str {
    if path.starts_with(HASHED_ASSET_PREFIX) {
        CACHE_IMMUTABLE
    } else {
        CACHE_REVALIDATE
    }
}

/// 给静态资源响应打 `Cache-Control`。只包前端静态服务,不碰 API 路由
/// (静态与 API 的缓存策略分开管)。
pub(crate) async fn static_cache_headers(request: Request, next: middleware::Next) -> Response {
    let policy = static_cache_control(request.uri().path());
    let mut response = next.run(request).await;
    // insert 而非 append:避免出现两个 Cache-Control。
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static(policy),
    );
    response
}

pub(crate) fn required_env(key: &str) -> String {
    match std::env::var(key) {
        Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => panic!("{key} is required and must be non-empty"),
    }
}

pub(crate) fn optional_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub(crate) fn build_cors_layer() -> CorsLayer {
    let env_mode = optional_env("ONCHINA_ENV")
        .or_else(|| optional_env("ENV"))
        .unwrap_or_else(|| "dev".to_string())
        .to_ascii_lowercase();
    let is_prod = env_mode == "prod" || env_mode == "production";
    let allow_any_in_prod = env_flag_enabled("ONCHINA_ALLOW_CORS_ANY_IN_PROD");
    let allow_all = std::env::var("ONCHINA_CORS_ALLOWED_ORIGINS")
        .ok()
        .map(|v| v.trim().to_string())
        .is_some_and(|v| v == "*");
    if allow_all {
        if is_prod && !allow_any_in_prod {
            panic!("ONCHINA_CORS_ALLOWED_ORIGINS='*' is forbidden in production");
        }
        return CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(vec![
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers(vec![
                HeaderName::from_static("authorization"),
                HeaderName::from_static("content-type"),
                HeaderName::from_static("x-request-id"),
                HeaderName::from_static("x-chain-token"),
                HeaderName::from_static("x-chain-request-id"),
                HeaderName::from_static("x-chain-nonce"),
                HeaderName::from_static("x-chain-timestamp"),
                HeaderName::from_static("x-chain-signature"),
            ]);
    }

    let configured = std::env::var("ONCHINA_CORS_ALLOWED_ORIGINS")
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .filter(|v| *v != "*")
                .filter_map(|v| HeaderValue::from_str(v).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let origins = if configured.is_empty() {
        vec![
            HeaderValue::from_static("https://onchina.local:8964"),
            HeaderValue::from_static("https://127.0.0.1:5179"),
            HeaderValue::from_static("https://localhost:5179"),
            HeaderValue::from_static("https://127.0.0.1:5173"),
            HeaderValue::from_static("https://localhost:5173"),
        ]
    } else {
        configured
    };
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(vec![
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(vec![
            HeaderName::from_static("authorization"),
            HeaderName::from_static("content-type"),
            HeaderName::from_static("x-request-id"),
            HeaderName::from_static("x-chain-token"),
            HeaderName::from_static("x-chain-request-id"),
            HeaderName::from_static("x-chain-nonce"),
            HeaderName::from_static("x-chain-timestamp"),
            HeaderName::from_static("x-chain-signature"),
            HeaderName::from_static("x-cid-security-grant"),
            HeaderName::from_static("x-passkey-assertion"),
        ])
}

pub(crate) async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = state
        .db
        .with_client(|conn| {
            conn.query_one("SELECT 1", &[])
                .map(|_| ())
                .map_err(|e| format!("health query failed: {e}"))
        })
        .is_ok();
    Json(ApiResponse {
        code: 0,
        message: "ok".to_string(),
        data: HealthData {
            service: "onchina",
            status: if db_ok { "UP" } else { "DEGRADED" },
            checked_at: Utc::now().timestamp(),
        },
    })
}

// chain pull 端点(multisig_info / joint_vote / vote_eligibility)的安全模型是
// "返回签名凭证只对请求者 account_id 有效",不需要请求侧 HMAC。

pub(crate) fn env_flag_enabled(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|v| {
            let value = v.trim();
            value.eq_ignore_ascii_case("1")
                || value.eq_ignore_ascii_case("true")
                || value.eq_ignore_ascii_case("yes")
                || value.eq_ignore_ascii_case("on")
        })
        .unwrap_or(false)
}

pub(crate) fn parse_csv_env_set(key: &str) -> Vec<String> {
    std::env::var(key)
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(|v| v.to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default()
}

fn trusted_proxy_ips() -> &'static [IpAddr] {
    TRUSTED_PROXY_IPS
        .get_or_init(|| {
            parse_csv_env_set("CID_TRUST_PROXY_IPS")
                .into_iter()
                .filter_map(|raw| raw.parse::<IpAddr>().ok())
                .collect::<Vec<_>>()
        })
        .as_slice()
}

fn peer_ip_from_request(request: &Request) -> Option<IpAddr> {
    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip())
}

fn actor_ip_from_request(request: &Request) -> Option<String> {
    let trusted_ips = trusted_proxy_ips();
    let peer_ip = peer_ip_from_request(request);
    if let Some(peer) = peer_ip {
        if trusted_ips.contains(&peer) {
            return actor_ip_from_headers(request.headers()).or_else(|| Some(peer.to_string()));
        }
        return Some(peer.to_string());
    }
    actor_ip_from_headers(request.headers())
}

pub(crate) fn chain_header_value(headers: &HeaderMap, key: &str) -> Option<String> {
    headers
        .get(key)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub(crate) fn actor_ip_from_headers(headers: &HeaderMap) -> Option<String> {
    let forwarded = chain_header_value(headers, "x-forwarded-for");
    if let Some(ff) = forwarded {
        return ff
            .split(',')
            .map(|v| v.trim())
            .find(|candidate| candidate.parse::<IpAddr>().is_ok())
            .map(|v| v.to_string());
    }
    chain_header_value(headers, "x-real-ip").filter(|candidate| candidate.parse::<IpAddr>().is_ok())
}

pub(crate) fn request_id_from_headers(headers: &HeaderMap) -> Option<String> {
    chain_header_value(headers, "x-chain-request-id")
        .or_else(|| chain_header_value(headers, "x-request-id"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashed_assets_get_immutable_cache() {
        // vite 内容哈希产物:内容变则文件名变,永久缓存零风险。
        for path in [
            "/assets/index-BH-jNQdT.js",
            "/assets/index-BdOndhxL.css",
            "/assets/nested/chunk-abc123.js",
        ] {
            assert_eq!(static_cache_control(path), CACHE_IMMUTABLE, "path={path}");
        }
    }

    #[test]
    fn index_and_spa_routes_must_revalidate() {
        // index.html 是哈希索引,被缓存则整套失效机制作废;SPA 回退路由同样返回 index.html。
        for path in [
            "/",
            "/index.html",
            "/citizens",
            "/admins/institutions",
            "/legislation/proposals/42",
        ] {
            assert_eq!(static_cache_control(path), CACHE_REVALIDATE, "path={path}");
        }
    }

    #[test]
    fn assets_prefix_requires_trailing_slash() {
        // 前缀判断必须带尾斜杠,否则这些路径会误命中永久缓存档。
        for path in [
            "/assets",
            "/assetsfoo",
            "/assets-old/index.js",
            "/x/assets/a.js",
        ] {
            assert_eq!(static_cache_control(path), CACHE_REVALIDATE, "path={path}");
        }
    }

    #[test]
    fn unhashed_future_resources_default_to_revalidate() {
        // 兜底档必须是 no-cache:非哈希资源误配永久缓存会永久钉死客户端。
        for path in [
            "/favicon.ico",
            "/robots.txt",
            "/logo.svg",
            "/manifest.webmanifest",
        ] {
            assert_eq!(static_cache_control(path), CACHE_REVALIDATE, "path={path}");
        }
    }
}
