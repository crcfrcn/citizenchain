//! 原所属模块回归，断言与输入保持。
use super::*;
#[tokio::test]
#[ignore = "仅供HTTPS进程回归通过精确名称拉起"]
async fn https_child() {
    if std::env::var_os("ONCHINA_HTTPS_CHILD_TEST").is_none() {
        return;
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = PathBuf::from(std::env::var_os("ONCHINA_TEST_TLS_DIRECTORY").unwrap());
    let id = std::env::var("ONCHINA_TEST_MODULE_ID").unwrap();
    let health_id = id.clone();
    let workspace_id = id.clone();
    let route = format!("/api/{id}/module-test/{{id}}");
    let version = std::env::var("ONCHINA_TEST_VERSION").unwrap();
    let health_version = version.clone();
    let slow = std::env::var("ONCHINA_TEST_SLOW_WORKSPACE").unwrap() == "1";
    let app = axum::Router::new()
            .route("/api/health", axum::routing::get(move || {
                let version = health_version.clone();
                let id = health_id.clone();
                async move { Json(serde_json::json!({"code":0,"data":{"status":"UP","module_id":id,"module_version":version}})) }
            }))
            .route(&route, axum::routing::post(move |request: Request| {
                let version = version.clone();
                async move {
                    let (parts, body) = request.into_parts();
                    let body = axum::body::to_bytes(body, 1024).await.unwrap();
                    let response = serde_json::json!({"version":version,"method":parts.method.as_str(),"uri":parts.uri.to_string(),
                        "authorization":parts.headers.get("authorization").and_then(|v|v.to_str().ok()),
                        "passkey":parts.headers.get("x-passkey-assertion").and_then(|v|v.to_str().ok()),
                        "grant":parts.headers.get("x-cid-security-grant").and_then(|v|v.to_str().ok()),
                        "body":String::from_utf8(body.to_vec()).unwrap()});
                    (StatusCode::CREATED, [("x-module-version", version)], Json(response))
                }
            }))
            .route("/internal/workspace", axum::routing::get(move || {
                let id = workspace_id.clone();
                async move {
                if slow { tokio::time::sleep(Duration::from_secs(60)).await; }
                Json(serde_json::json!({"code":0,"data":[id]}))
                }
            }));
    let config = axum_server::tls_rustls::RustlsConfig::from_pem_file(
        directory.join("server.crt"),
        directory.join("server.key"),
    )
    .await
    .unwrap();
    let handle = axum_server::Handle::new();
    let close = handle.clone();
    let parent = crate::parent_shutdown();
    tokio::spawn(async move {
        crate::shutdown_signal(parent).await;
        close.graceful_shutdown(Some(Duration::from_millis(100)));
    });
    // 明确TCP地址类型，避免axum-server同时支持Unix地址时无法推断。
    axum_server::bind_rustls(
        std::env::var("ONCHINA_TEST_BIND")
            .unwrap()
            .parse::<std::net::SocketAddr>()
            .unwrap(),
        config,
    )
    .handle(handle)
    .serve(app.into_make_service())
    .await
    .unwrap();
}

#[tokio::test]
async fn https_workspace_timeout_and_restart_preserve_process_ownership() {
    let mut fixture = super::test_support::https_fixture("example").await;
    let old = fixture.catalog.remote("example").unwrap();
    let old_pid = old.child.lock().unwrap().id();
    assert_eq!(
        fixture
            .catalog
            .workspace_modules("test-cid", &HeaderMap::new())
            .await,
        vec!["example"]
    );
    fixture.restart("1.0.1", true).await;
    assert!(old.child.lock().unwrap().try_wait().unwrap().is_some());
    assert_ne!(
        old_pid,
        fixture
            .catalog
            .remote("example")
            .unwrap()
            .child
            .lock()
            .unwrap()
            .id()
    );
    assert_eq!(fixture.catalog.assets()[0].version, "1.0.1");
    let started = Instant::now();
    assert!(fixture
        .catalog
        .workspace_modules("test-cid", &HeaderMap::new())
        .await
        .is_empty());
    assert!(started.elapsed() < Duration::from_secs(9));
    fixture.catalog.stop_all().unwrap();
    fixture.catalog.stop_all().unwrap();
    assert!(fixture.catalog.assets().is_empty());
    let stopped = fixture.catalog.remote("example");
    assert!(stopped.is_none());
}

#[tokio::test]
async fn slow_workspace_modules_do_not_serialize_or_hide_healthy_modules() {
    let mut first = super::test_support::https_fixture("slowone").await;
    let mut second = super::test_support::https_fixture("slowtwo").await;
    let ready = super::test_support::https_fixture("ready").await;
    first.restart("1.0.1", true).await;
    second.restart("1.0.1", true).await;
    let catalog = ModuleCatalog::new(None);
    for fixture in [&first, &second, &ready] {
        catalog
            .remotes
            .lock()
            .unwrap()
            .push(fixture.catalog.remote(&fixture.id).unwrap());
    }
    let start = Instant::now();
    assert_eq!(
        catalog
            .workspace_modules("test-cid", &HeaderMap::new())
            .await,
        vec!["ready"]
    );
    // 两个5秒超时若串行会超过10秒；允许调度余量，仍须保留健康模块。
    assert!(start.elapsed() < Duration::from_secs(9));
    catalog.stop_all().unwrap();
}

#[tokio::test]
async fn module_https_requires_certificate_trust_and_hostname() {
    let fixture = super::test_support::https_fixture("example").await;
    let untrusted = reqwest::Client::builder()
        .no_proxy()
        .https_only(true)
        .resolve("onchina.crcfrcn.com", fixture.address)
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    assert!(untrusted
        .get(format!(
            "https://onchina.crcfrcn.com:{}/api/health",
            fixture.address.port()
        ))
        .send()
        .await
        .is_err());
    let wrong_host = reqwest::Client::builder()
        .no_proxy()
        .https_only(true)
        .resolve("wrong.invalid", fixture.address)
        .add_root_certificate(reqwest::Certificate::from_pem(fixture.ca.as_bytes()).unwrap())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    assert!(wrong_host
        .get(format!(
            "https://wrong.invalid:{}/api/health",
            fixture.address.port()
        ))
        .send()
        .await
        .is_err());
    fixture.catalog.stop_all().unwrap();
}

#[test]
fn route_conflicts_include_method_and_path() {
    let route = RouteDescription {
        group: "admin".into(),
        path: "/api/example/{id}".into(),
        methods: vec!["GET".into()],
        body_limit: 2 * 1024 * 1024,
    };
    assert!(route_keys(&[route.clone()]).is_ok());
    assert!(route_keys(&[route.clone(), route]).is_err());
}
#[test]
fn implicit_head_and_parameter_names_cannot_bypass_conflicts() {
    let get = RouteDescription {
        group: "admin".into(),
        path: "/api/example/{id}".into(),
        methods: vec!["GET".into()],
        body_limit: 1,
    };
    let head = RouteDescription {
        methods: vec!["HEAD".into()],
        path: "/api/example/{cid_number}".into(),
        ..get.clone()
    };
    assert!(route_keys(&[get.clone(), head]).is_err());
    let invalid = RouteDescription {
        path: "/api/example/{*all}".into(),
        ..get.clone()
    };
    assert!(route_keys(&[invalid]).is_err());
    let too_large = RouteDescription {
        body_limit: crate::documents::MAX_REQUEST_BYTES + 1,
        ..get
    };
    assert!(route_keys(&[too_large]).is_err());
}
#[test]
fn incompatible_metadata_and_core_action_replacement_are_rejected() {
    let mut description = ModuleDescription {
        id: "example".into(),
        version: "1.0.0".into(),
        interface_version: model::MODULE_INTERFACE_VERSION,
        frontend_source: "frontend/Workspace.tsx".into(),
        frontend_exports: vec!["Workspace".into()],
        frontend_workspace: Some("Workspace".into()),
        instance_workspace: false,
        routes: Vec::new(),
        purposes: Vec::new(),
        actions: Vec::new(),
        capabilities: BTreeMap::new(),
        workspaces: BTreeMap::new(),
    };
    assert!(validate_description(&description).is_ok());
    let mut legacy = description.clone();
    legacy.interface_version = 1;
    assert!(
        validate_description(&legacy).is_err(),
        "旧私有CA模块必须在启动之前拒绝"
    );
    description.interface_version += 1;
    assert!(validate_description(&description).is_err());
    description.interface_version -= 1;
    description.frontend_workspace = Some("Missing".into());
    assert!(validate_description(&description).is_err());
    description.frontend_workspace = None;
    description.actions.push("NODE_BINDING_UNBIND".into());
    assert!(validate_description(&description).is_err());
    description.actions = vec!["EXAMPLE_OPERATION".into()];
    assert!(validate_description(&description).is_ok());
    description
        .capabilities
        .insert("FRG".into(), capability::EMPTY);
    assert!(validate_description(&description).is_err());
    description.capabilities.clear();
    description.frontend_source = "../frontend/Workspace.tsx".into();
    assert!(validate_description(&description).is_err());
}
#[test]
fn identifiers_reject_paths() {
    for id in ["registry", "legislation", "private", "judicial"] {
        assert!(valid_id(id));
    }
    for id in ["", "../registry", "/tmp/module", "Registry", "a-b"] {
        assert!(!valid_id(id));
    }
}
