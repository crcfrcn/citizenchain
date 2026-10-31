//! 所属独立软件包的真实声明与业务路由合同。
use super::*;
use onchina::modules::model::ModuleBusiness;

#[test]
fn actual_module_preserves_owned_capabilities_and_workspaces() {
    onchina::modules::test_support::assert_actual_module(&Module::default());
}

#[test]
fn unknown_institution_never_gets_a_business_workspace() {
    assert!(declaration("").is_none());
    assert!(declaration("UNKNOWN").is_none());
    assert!(declaration("PMUL").is_none());
}

#[test]
fn module_routes_match_owner_golden() {
    let expected: serde_json::Value = serde_json::from_str(include_str!("routes.json")).unwrap();
    let description = Module::default().describe();
    let mut actual: Vec<_> = description
        .routes
        .iter()
        .map(|route| {
            (
                route.group.clone(),
                route.path.clone(),
                route
                    .methods
                    .iter()
                    .map(|method| method.to_lowercase())
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let mut expected: Vec<_> = expected
        .as_array()
        .unwrap()
        .iter()
        .map(|route| {
            (
                route["group"].as_str().unwrap().to_string(),
                route["path"].as_str().unwrap().to_string(),
                route["methods"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|method| method.as_str().unwrap().to_string())
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn real_owned_router_preserves_authentication_and_missing_api_status() {
    use tower::ServiceExt;
    let module = std::sync::Arc::new(Module::default());
    let declared = module.describe().routes;
    let router = onchina::testing::router_for(module);
    for route in declared.iter().filter(|route| route.group == "admin") {
        let mut path = route.path.clone();
        while let Some(start) = path.find('{') {
            let end = path[start..].find('}').unwrap() + start;
            path.replace_range(start..=end, "example");
        }
        for method in &route.methods {
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
                axum::http::StatusCode::UNAUTHORIZED,
                "{method} {path}"
            );
        }
    }
    let missing = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/not-installed/operation")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), axum::http::StatusCode::NOT_FOUND);
    let bytes = axum::body::to_bytes(missing.into_body(), 4096)
        .await
        .unwrap();
    // 原内部模块未匹配路由返回空404；公共宿主另核验JSON错误。
    assert!(bytes.is_empty());
}
