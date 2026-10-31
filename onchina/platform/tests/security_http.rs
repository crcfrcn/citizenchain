//! 原所属模块回归，断言与输入保持。
use super::*;

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn preflight_allows_patch_and_security_headers_for_both_origin_policies() {
    use axum::{body::Body, http::Request, routing::patch, Router};
    use tower::ServiceExt;
    for layer in [
        CorsLayer::new().allow_origin(Any),
        CorsLayer::new().allow_origin(vec![HeaderValue::from_static(
            "https://onchina.crcfrcn.com:1988",
        )]),
    ] {
        let app = Router::new()
            .route("/probe", patch(|| async { "ok" }))
            .layer(
                layer
                    .allow_methods(cors_methods())
                    .allow_headers(cors_headers()),
            );
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/probe")
                    .header("origin", "https://onchina.crcfrcn.com:1988")
                    .header("access-control-request-method", "PATCH")
                    .header(
                        "access-control-request-headers",
                        "authorization,content-type,x-passkey-assertion,x-cid-security-grant",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(response.status().is_success());
        assert!(response.headers()["access-control-allow-methods"]
            .to_str()
            .unwrap()
            .split(',')
            .any(|v| v.trim() == "PATCH"));
        let headers = response.headers()["access-control-allow-headers"]
            .to_str()
            .unwrap();
        for header in [
            "authorization",
            "content-type",
            "x-passkey-assertion",
            "x-cid-security-grant",
        ] {
            assert!(headers.split(',').any(|v| v.trim() == header));
        }
    }
    let app = Router::new()
        .route("/probe", patch(|| async { "ok" }))
        .layer(
            CorsLayer::new()
                .allow_origin(vec![HeaderValue::from_static(
                    "https://onchina.crcfrcn.com:1988",
                )])
                .allow_methods(cors_methods())
                .allow_headers(cors_headers()),
        );
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/probe")
                .header("origin", "https://untrusted.invalid")
                .header("access-control-request-method", "PATCH")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(!response
        .headers()
        .contains_key("access-control-allow-origin"));
}

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
