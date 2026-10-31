//! 模块测试辅助，仅由cfg(test)或test-support加载。
/// 每个业务金标只有其所属tests中的一份，公共测试按需组合。
pub fn fixture_cases() -> Vec<serde_json::Value> {
    let mut values = serde_json::from_str::<serde_json::Value>(include_str!("layout.json"))
        .unwrap()["capabilityCases"]
        .as_array()
        .unwrap()
        .clone();
    for text in [
        include_str!("../../registry/tests/capabilities.json"),
        include_str!("../../legislation/tests/capabilities.json"),
        include_str!("../../private/tests/capabilities.json"),
        include_str!("../../judicial/tests/capabilities.json"),
    ] {
        values.extend(serde_json::from_str::<Vec<serde_json::Value>>(text).unwrap());
    }
    values
}
use super::*;
pub fn fixture_description() -> ModuleDescription {
    let fixture: serde_json::Value = serde_json::json!({"capabilityCases": fixture_cases()});
    let mut capabilities = BTreeMap::new();
    let mut workspaces = BTreeMap::new();
    for case in fixture["capabilityCases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["owner"].is_string())
    {
        let mut value = serde_json::to_value(capability::EMPTY).unwrap();
        for flag in case["flags"].as_array().unwrap() {
            value[flag.as_str().unwrap()] = serde_json::Value::Bool(true);
        }
        let code = case["code"].as_str().unwrap().to_string();
        capabilities.insert(code.clone(), serde_json::from_value(value).unwrap());
        if !case["kind"].is_null() {
            workspaces.insert(
                code,
                WorkspaceDeclaration {
                    kind: serde_json::from_value(case["kind"].clone()).unwrap(),
                    sections: serde_json::from_value(case["sections"].clone()).unwrap(),
                },
            );
        }
    }
    ModuleDescription {
        id: "fixture".into(),
        version: "1.0.0".into(),
        interface_version: model::MODULE_INTERFACE_VERSION,
        frontend_source: "frontend/Fixture.tsx".into(),
        frontend_exports: vec!["Fixture".into()],
        frontend_workspace: None,
        instance_workspace: false,
        routes: Vec::new(),
        purposes: Vec::new(),
        actions: Vec::new(),
        capabilities,
        workspaces,
    }
}
struct FixtureModule;
#[async_trait::async_trait]
impl ModuleBusiness for FixtureModule {
    fn describe(&self) -> ModuleDescription {
        fixture_description()
    }
    fn routes(&self) -> Vec<ModuleRoute> {
        Vec::new()
    }
}
pub fn fixture_catalog() -> ModuleCatalog {
    ModuleCatalog::new(Some(Arc::new(FixtureModule)))
}

// 真实HTTPS/父管道测试夹具：临时证书和输入只归本用例，不连接数据库或链。
pub struct HttpsFixture {
    pub catalog: Arc<ModuleCatalog>,
    pub address: std::net::SocketAddr,
    pub ca: String,
    directory: PathBuf,
    pub id: String,
}
impl HttpsFixture {
    pub async fn restart(&mut self, version: &str, slow: bool) {
        self.catalog.stop_all().unwrap();
        // 直接复用当前测试程序；每次子进程仍有独立输入、父管道和退出回收。
        // 标准stderr直接交给测试日志，不隐藏子进程的启动诊断。
        let mut child = ModuleProcess(
            Command::new(std::env::current_exe().unwrap())
                .args(["--ignored", "--exact", "modules::tests::https_child"])
                .env("ONCHINA_HTTPS_CHILD_TEST", "1")
                .env("ONCHINA_PARENT_PIPE", "1")
                .env("ONCHINA_TEST_MODULE_ID", &self.id)
                .env("ONCHINA_TEST_BIND", self.address.to_string())
                .env("ONCHINA_TEST_VERSION", version)
                .env("ONCHINA_TEST_TLS_DIRECTORY", &self.directory)
                .env("ONCHINA_TEST_SLOW_WORKSPACE", if slow { "1" } else { "0" })
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let client = reqwest::Client::builder()
            .no_proxy()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .add_root_certificate(reqwest::Certificate::from_pem(self.ca.as_bytes()).unwrap())
            .resolve("onchina.local", self.address)
            .connect_timeout(Duration::from_secs(1))
            .build()
            .unwrap();
        let endpoint = format!("https://onchina.local:{}", self.address.port());
        let start = Instant::now();
        loop {
            assert!(
                child.try_wait().unwrap().is_none(),
                "HTTPS test child exited"
            );
            // 只报告本夹具健康请求的失败；原重试、超时、证书与主机名校验不变。
            if let Ok(response) = client
                .get(format!("{endpoint}/api/health"))
                .timeout(Duration::from_millis(250))
                .send()
                .await
                .inspect_err(|error| eprintln!("HTTPS test health request failed: {error:?}"))
            {
                let status = response.status();
                if let Ok(value) = response
                    .json::<serde_json::Value>()
                    .await
                    .inspect_err(|error| {
                        eprintln!("HTTPS test health JSON failed ({status}): {error:?}")
                    })
                {
                    if value
                        .pointer("/data/module_version")
                        .and_then(|v| v.as_str())
                        == Some(version)
                    {
                        break;
                    }
                    eprintln!("HTTPS test health version mismatch ({status}): expected {version}, received {:?}", value.pointer("/data/module_version").and_then(|v| v.as_str()));
                }
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "HTTPS test child readiness"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let route = RouteDescription {
            group: "public".into(),
            path: format!("/api/{}/module-test/{{id}}", self.id),
            methods: vec!["POST".into()],
            body_limit: 1024,
        };
        let description = ModuleDescription {
            id: self.id.clone(),
            version: version.into(),
            interface_version: model::MODULE_INTERFACE_VERSION,
            frontend_source: "frontend/Workspace.tsx".into(),
            frontend_exports: vec!["Workspace".into()],
            frontend_workspace: None,
            instance_workspace: true,
            routes: vec![route],
            purposes: Vec::new(),
            actions: Vec::new(),
            capabilities: BTreeMap::new(),
            workspaces: BTreeMap::new(),
        };
        validate_description(&description).unwrap();
        let frontend = self.directory.join("frontend");
        std::fs::create_dir_all(&frontend).unwrap();
        let marker = format!(
            "/*onchina:{}*/\n",
            serde_json::json!({"id":&self.id,"version":version,"interfaceVersion":model::MODULE_INTERFACE_VERSION})
        );
        for name in ["module.js", "module.css"] {
            std::fs::write(frontend.join(name), &marker).unwrap();
        }
        verify_frontend(&frontend, &description).unwrap();
        self.catalog = Arc::new(ModuleCatalog::new(None));
        self.catalog
            .remotes
            .lock()
            .unwrap()
            .push(Arc::new(RemoteModule {
                description,
                client,
                endpoint,
                frontend,
                child: Mutex::new(child),
            }));
    }
}
impl Drop for HttpsFixture {
    fn drop(&mut self) {
        let result = self.catalog.stop_all().and_then(|_| {
            std::fs::remove_dir_all(&self.directory).map_err(|error| error.to_string())
        });
        if !std::thread::panicking() {
            result.expect("test process and temporary directory cleanup");
        }
    }
}
pub async fn https_fixture(id: &str) -> HttpsFixture {
    assert!(valid_id(id));
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory =
        std::env::temp_dir().join(format!("onchina-module-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    // 目录一创建就交给守卫，证书生成或启动失败同样清理自己的现场。
    let mut fixture = HttpsFixture {
        catalog: Arc::new(ModuleCatalog::new(None)),
        address: (Ipv4Addr::LOCALHOST, 0).into(),
        ca: String::new(),
        directory,
        id: id.into(),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fixture.directory, std::fs::Permissions::from_mode(0o700))
            .unwrap();
    }
    let ca_key = rcgen::KeyPair::generate().unwrap();
    let mut ca_params = rcgen::CertificateParams::default();
    // 沿用rcgen官方CA签发示例，名称与密钥标识明确区分颁发者和服务器。
    ca_params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "OnChina test CA");
    ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        rcgen::KeyUsagePurpose::DigitalSignature,
        rcgen::KeyUsagePurpose::KeyCertSign,
        rcgen::KeyUsagePurpose::CrlSign,
    ];
    let ca = ca_params.self_signed(&ca_key).unwrap();
    let key = rcgen::KeyPair::generate().unwrap();
    let mut params = rcgen::CertificateParams::new(vec!["onchina.local".into()]).unwrap();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "onchina.local");
    params.use_authority_key_identifier_extension = true;
    params.key_usages = vec![rcgen::KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    let certificate = params.signed_by(&key, &ca, &ca_key).unwrap();
    std::fs::write(fixture.directory.join("server.crt"), certificate.pem()).unwrap();
    std::fs::write(fixture.directory.join("server.key"), key.serialize_pem()).unwrap();
    let socket = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    fixture.address = socket.local_addr().unwrap();
    drop(socket);
    fixture.ca = ca.pem();
    fixture.restart("1.0.0", false).await;
    fixture
}

pub fn assert_actual_module(module: &dyn ModuleBusiness) {
    let description = module.describe();
    validate_description(&description).unwrap();
    assert_eq!(
        description.routes,
        module
            .routes()
            .into_iter()
            .map(|route| route.description)
            .collect::<Vec<_>>()
    );
    let fixture: serde_json::Value = serde_json::json!({"capabilityCases": fixture_cases()});
    let expected = fixture_description();
    for case in fixture["capabilityCases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["owner"].as_str() == Some(description.id.as_str()))
    {
        let code = case["code"].as_str().unwrap();
        assert_eq!(
            serde_json::to_value(description.capabilities.get(code).unwrap()).unwrap(),
            serde_json::to_value(expected.capabilities[code]).unwrap(),
            "{code}"
        );
        let workspace = description.workspaces.get(code).unwrap();
        assert_eq!(serde_json::to_value(workspace.kind).unwrap(), case["kind"]);
        assert_eq!(
            serde_json::to_value(&workspace.sections).unwrap(),
            case["sections"],
            "{code} sections"
        );
    }
}
