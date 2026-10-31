//! 原所属模块回归，断言与输入保持。
use super::*;
use tower::ServiceExt;

#[test]
fn actual_module_preserves_capabilities_and_route_declarations() {
    if let Some(module) = local_module() {
        modules::test_support::assert_actual_module(module.as_ref());
    } else {
        assert_eq!(modules::ModuleCatalog::new(None).assets().len(), 0);
    }
}

#[tokio::test]
async fn real_router_builds_and_protects_parameterized_business_routes() {
    let state = AppState {
        db: Db::empty_for_route_test(),
        rate_limiter: Arc::new(LocalRateLimiter::new()),
        modules: Arc::new(modules::ModuleCatalog::new(local_module())),
    };
    let declared = state
        .modules
        .local
        .as_ref()
        .map(|m| m.describe().routes)
        .unwrap_or_else(|| {
            platform_routes()
                .into_iter()
                .map(|route| route.description)
                .collect()
        });
    let router = build_routes(state);
    let mut requests = vec![("POST".to_string(), "/api/admin/chain/submit".to_string())];
    for route in declared.into_iter().filter(|r| r.group == "admin") {
        let mut path = route.path.clone();
        while let Some(start) = path.find('{') {
            let end = path[start..].find('}').unwrap() + start;
            path.replace_range(start..=end, "example");
        }
        for method in route.methods {
            requests.push((method, path.clone()));
        }
    }
    for (method, path) in requests {
        let response = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method(method.as_str())
                    .uri(&path)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path}"
        );
    }
    let missing = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/not-installed/operation")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    let missing_body = axum::body::to_bytes(missing.into_body(), 4096)
        .await
        .unwrap();
    let missing_json: serde_json::Value = serde_json::from_slice(&missing_body).unwrap();
    assert_eq!(missing_json["code"], 1004);
    assert!(missing_json["message"]
        .as_str()
        .unwrap()
        .contains("unavailable"));
    let response = router
        .oneshot(
            axum::http::Request::builder()
                .method("DELETE")
                .uri("/api/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}
#[tokio::test]
async fn host_forwards_over_real_https_and_restart_replaces_only_module_software() {
    let mut fixture = modules::test_support::https_fixture("example").await;
    let host = |catalog| {
        build_routes(AppState {
            db: Db::empty_for_route_test(),
            rate_limiter: Arc::new(LocalRateLimiter::new()),
            modules: catalog,
        })
    };
    let router = host(fixture.catalog.clone());
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/api/example/module-test/one?value=kept")
        .header("authorization", "Bearer fixture-session")
        .header("x-passkey-assertion", "fixture-proof")
        .header("x-cid-security-grant", "fixture-grant")
        .body(axum::body::Body::from("original-body"))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response.headers()["x-module-version"], "1.0.0");
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["method"], "POST");
    assert_eq!(body["uri"], "/api/example/module-test/one?value=kept");
    assert_eq!(body["body"], "original-body");
    assert_eq!(body["authorization"], "Bearer fixture-session");
    assert_eq!(body["passkey"], "fixture-proof");
    assert_eq!(body["grant"], "fixture-grant");
    let too_large = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/example/module-test/one")
                .body(axum::body::Body::from(vec![0u8; 1025]))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(too_large.status(), StatusCode::PAYLOAD_TOO_LARGE);
    fixture.catalog.stop_all().unwrap();
    let stopped = router
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/example/module-test/one")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stopped.status(), StatusCode::SERVICE_UNAVAILABLE);
    fixture.restart("1.0.1", false).await;
    let restarted = host(fixture.catalog.clone());
    let response = restarted
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/example/module-test/two")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers()["x-module-version"], "1.0.1");
    let asset = restarted
        .oneshot(
            axum::http::Request::builder()
                .uri("/modules/example/module.js")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(asset.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(asset.into_body(), 4096).await.unwrap();
    let header = std::str::from_utf8(&bytes).unwrap().lines().next().unwrap();
    assert!(header.contains("1.0.1"));
    fixture.catalog.stop_all().unwrap();
    let unplugged = host(Arc::new(modules::ModuleCatalog::new(None)));
    let missing = unplugged
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/example/module-test/one")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert!(missing.headers()["content-type"]
        .to_str()
        .unwrap()
        .contains("application/json"));
}
