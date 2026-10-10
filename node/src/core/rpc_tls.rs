//! 原生与 Ethereum RPC 的唯一 TLS 监听器。证书缺失或无效时拒绝启动。

use futures::future::try_join_all;
use jsonrpsee::{
    server::{serve_with_graceful_shutdown, stop_channel, ServerHandle},
    RpcModule,
};
use rustls::pki_types::{pem::PemObject, CertificateDer, PrivatePkcs8KeyDer};
use sc_rpc_api::DenyUnsafe;
use sc_rpc_server::{MiddlewareLayer, RpcEndpoint, RpcServiceBuilder};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::Semaphore, task::JoinSet};
use tokio_rustls::TlsAcceptor;
use tower::Service;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

type RpcError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// HTTP 层错误使用有所有权的具体类型，跨 TLS 任务时不传播动态错误的生命周期。
#[derive(Debug)]
struct HttpServiceError(RpcError);

impl std::fmt::Display for HttpServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.0.as_ref(), formatter)
    }
}

impl std::error::Error for HttpServiceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref())
    }
}

/// 已装载的 RPC TLS 配置；私钥不进入日志、RPC 或持久化链状态。
pub(crate) struct RpcTls {
    acceptor: TlsAcceptor,
    page: Arc<[u8]>,
}

const INSTALL_PAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/metamask.html"));
const GMB_ICON: &[u8] = include_bytes!("../../../icons/gmb.png");

/// 精确路径分流；WebSocket 升级继续交给 RPC，不能被普通页面 GET 截获。
fn resource_response<B>(
    request: &jsonrpsee::server::HttpRequest<B>,
    page: &[u8],
) -> Option<jsonrpsee::server::HttpResponse> {
    let method = request.method().as_str();
    let root = request.uri().path() == "/" && request.uri().query().is_none();
    let upgrade = request
        .headers()
        .get("upgrade")
        .is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"websocket"));
    if root && (method == "POST" || method == "OPTIONS" || method == "GET" && upgrade) {
        return None;
    }
    let resource = match (request.uri().path(), request.uri().query()) {
        ("/", None) => Some((page, "text/html; charset=utf-8")),
        ("/icons/gmb.png", None) => Some((GMB_ICON, "image/png")),
        _ => None,
    };
    let status = if resource.is_none() {
        404
    } else if matches!(method, "GET" | "HEAD") {
        200
    } else {
        405
    };
    let mut response = jsonrpsee::server::HttpResponse::builder()
        .status(status)
        .header("cache-control", "no-store")
        .header("x-content-type-options", "nosniff");
    let body = if status == 200 {
        let (bytes, content_type) = resource.expect("已验证的固定资源");
        response = response
            .header("content-type", content_type)
            .header("content-length", bytes.len());
        if method == "HEAD" {
            jsonrpsee::server::HttpBody::empty()
        } else {
            jsonrpsee::server::HttpBody::from(bytes.to_vec())
        }
    } else {
        if status == 405 {
            response = response.header("allow", "GET, HEAD");
        }
        jsonrpsee::server::HttpBody::empty()
    };
    Some(response.body(body).expect("固定资源响应"))
}

impl RpcTls {
    /// CLI 与桌面进程共用同一配置入口，禁止自动生成或复用 P2P 自签证书。
    pub(crate) fn load() -> Result<Self, RpcError> {
        let certificate = std::env::var_os("CITIZENCHAIN_RPC_TLS_CERTIFICATE")
            .ok_or("缺少 CITIZENCHAIN_RPC_TLS_CERTIFICATE")?;
        let private_key = std::env::var_os("CITIZENCHAIN_RPC_TLS_PRIVATE_KEY")
            .ok_or("缺少 CITIZENCHAIN_RPC_TLS_PRIVATE_KEY")?;
        Self::from_paths(Path::new(&certificate), Path::new(&private_key))
    }

    fn from_paths(certificate: &Path, private_key: &Path) -> Result<Self, RpcError> {
        if !certificate.is_absolute() || !private_key.is_absolute() {
            return Err("RPC TLS 证书和私钥必须使用绝对路径".into());
        }
        let chain = CertificateDer::pem_file_iter(certificate)?.collect::<Result<Vec<_>, _>>()?;
        if chain.is_empty() {
            return Err("RPC TLS 证书链为空".into());
        }
        let key = PrivatePkcs8KeyDer::from(std::fs::read(private_key)?);
        Self::from_der(chain, key)
    }

    pub(super) fn from_der(
        chain: Vec<CertificateDer<'static>>,
        key: PrivatePkcs8KeyDer<'static>,
    ) -> Result<Self, RpcError> {
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(chain, key.into())?;
        Ok(Self {
            acceptor: TlsAcceptor::from(Arc::new(config)),
            page: Arc::from(INSTALL_PAGE),
        })
    }

    /// 隔离视图只供测试会话注入，正式进程没有页面替换环境变量或磁盘读取入口。
    #[cfg(test)]
    pub(super) fn with_test_page(mut self, page: Vec<u8>) -> Result<Self, RpcError> {
        let html = std::str::from_utf8(&page)?;
        if page.len() > 128 * 1024
            || !html.starts_with("<!doctype html>")
            || !html.ends_with("</html>\n")
        {
            return Err("隔离接入页必须是有界的完整 HTML".into());
        }
        self.page = Arc::from(page);
        Ok(self)
    }

    /// 使用既有端点和限额运行 TLS；所有监听、连接和订阅随此 Future 取消而关闭。
    pub(crate) async fn run(
        self,
        endpoints: Vec<RpcEndpoint>,
        mut module: RpcModule<()>,
    ) -> Result<(), RpcError> {
        module.extensions_mut().insert(DenyUnsafe::Yes);
        module.remove_method("rpc_methods");
        let mut available = module.method_names().map(str::to_owned).collect::<Vec<_>>();
        available.push("rpc_methods".into());
        available.sort();
        module.register_method(
            "rpc_methods",
            move |_, _, _| serde_json::json!({"methods":available}),
        )?;
        let mut listeners = Vec::new();
        for endpoint in endpoints {
            if !endpoint.listen_addr.ip().is_loopback() {
                return Err("RPC TLS 仅允许回环监听".into());
            }
            if endpoint.rate_limit_trust_proxy_headers {
                return Err("RPC TLS 限流只使用真实连接地址，禁止信任代理头".into());
            }
            let listener = match TcpListener::bind(endpoint.listen_addr).await {
                Ok(listener) => listener,
                Err(_) if endpoint.is_optional => continue,
                Err(error) => return Err(error.into()),
            };
            listeners.push((listener, endpoint));
        }
        if listeners.is_empty() {
            return Err("RPC TLS 没有成功监听的端点".into());
        }
        let (stop, handle) = stop_channel();
        let _shutdown = Shutdown(handle);
        let mut tasks = Vec::new();
        for (listener, endpoint) in listeners {
            let stop = stop.clone();
            let acceptor = self.acceptor.clone();
            let page = self.page.clone();
            let module = module.clone();
            tasks.push(async move {
                let cors = match endpoint.cors.clone() {
                    Some(origins) => CorsLayer::new()
                        .allow_headers(Any)
                        .allow_methods(Any)
                        .allow_origin(AllowOrigin::predicate(move |origin, _| {
                            origins
                                .iter()
                                .any(|allowed| allowed.as_bytes() == origin.as_bytes())
                        })),
                    None => CorsLayer::permissive(),
                };
                let address = listener.local_addr()?;
                let host_filter = jsonrpsee::server::middleware::http::HostFilterLayer::new([
                    format!("localhost:{}", address.port()),
                    format!("127.0.0.1:{}", address.port()),
                    format!("[::1]:{}", address.port()),
                ])?;
                let builder = jsonrpsee::server::Server::builder()
                    .max_request_body_size(endpoint.max_payload_in_mb.saturating_mul(1024 * 1024))
                    .max_response_body_size(endpoint.max_payload_out_mb.saturating_mul(1024 * 1024))
                    .max_connections(endpoint.max_connections)
                    .max_subscriptions_per_connection(endpoint.max_subscriptions_per_connection)
                    .set_message_buffer_capacity(endpoint.max_buffer_capacity_per_connection)
                    .set_batch_request_config(endpoint.batch_config)
                    .set_id_provider(sc_rpc_server::RandomStringIdProvider::new(16))
                    .to_service_builder();
                let connections = Arc::new(Semaphore::new(endpoint.max_connections as usize));
                // 同一端点的额度跨连接共享，不能通过反复握手重置限流计数。
                let rate_middleware = endpoint
                    .rate_limit
                    .map(|limit| MiddlewareLayer::new().with_rate_limit_per_minute(limit));
                let mut tasks = JoinSet::new();
                loop {
                    let (socket, remote) = tokio::select! {
                        accepted = listener.accept() => accepted?,
                        _ = stop.clone().shutdown() => return Ok::<(), RpcError>(()),
                        Some(_) = tasks.join_next(), if !tasks.is_empty() => continue,
                    };
                    let Ok(permit) = connections.clone().try_acquire_owned() else {
                        continue;
                    };
                    let acceptor = acceptor.clone();
                    let builder = builder.clone();
                    let module = module.clone();
                    let stop = stop.clone();
                    let middleware = if endpoint
                        .rate_limit_whitelisted_ips
                        .iter()
                        .any(|range| range.contains(remote.ip()))
                    {
                        None
                    } else {
                        rate_middleware.clone()
                    };
                    let allowed_origins = endpoint.cors.clone();
                    let http_layers = tower::ServiceBuilder::new()
                        .layer(host_filter.clone())
                        .layer(cors.clone());
                    let page = page.clone();
                    tasks.spawn(async move {
                        let _permit = permit;
                        let Ok(Ok(tls)) =
                            tokio::time::timeout(Duration::from_secs(10), acceptor.accept(socket))
                                .await
                        else {
                            return;
                        };
                        let builder = builder
                            .set_rpc_middleware(RpcServiceBuilder::new().option_layer(middleware));
                        let connection_stop = stop.clone();
                        let service = tower::service_fn(
                            move |mut request: jsonrpsee::server::HttpRequest<_>| {
                                request.extensions_mut().insert(DenyUnsafe::Yes);
                                let mut rpc = builder.clone().build(module.clone(), stop.clone());
                                let page = page.clone();
                                // Host/CORS 中间件覆盖静态交付与 RPC，不能因分流绕过校验。
                                let router = tower::service_fn(move |request| {
                                    let response = resource_response(&request, &page);
                                    let pending = if response.is_none() {
                                        Some(rpc.call(request))
                                    } else {
                                        None
                                    };
                                    async move {
                                        match response {
                                            Some(response) => Ok::<_, RpcError>(response),
                                            None => pending.expect("RPC 分支").await,
                                        }
                                    }
                                });
                                let mut service = http_layers.service(router);
                                let rejected_origin =
                                    match (&allowed_origins, request.headers().get("origin")) {
                                        (Some(origins), Some(origin)) => !origins
                                            .iter()
                                            .any(|allowed| allowed.as_bytes() == origin.as_bytes()),
                                        _ => false,
                                    };
                                async move {
                                    // 浏览器 WSS 不执行 HTTP CORS 检查，握手必须显式核对 Origin。
                                    if rejected_origin {
                                        return Ok::<_, HttpServiceError>(
                                            jsonrpsee::server::HttpResponse::builder()
                                                .status(403)
                                                .body(jsonrpsee::server::HttpBody::empty())
                                                .expect("固定 HTTP 响应"),
                                        );
                                    }
                                    service.call(request).await.map_err(HttpServiceError)
                                }
                            },
                        );
                        // 固定每连接服务的 Future 与错误类型，TLS 任务只持有独立的所有权。
                        let service = tower::util::BoxCloneService::<
                            _,
                            jsonrpsee::server::HttpResponse,
                            HttpServiceError,
                        >::new(service);
                        let _ =
                            serve_with_graceful_shutdown(tls, service, connection_stop.shutdown())
                                .await;
                    });
                }
            });
        }
        try_join_all(tasks).await?;
        Err("RPC TLS 监听任务已退出".into())
    }
}

/// 停止句柄随节点任务释放，通知仍在执行的 HTTP/WSS 会话退出。
struct Shutdown(ServerHandle);
impl Drop for Shutdown {
    fn drop(&mut self) {
        let _ = self.0.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_routes_preserve_rpc_upgrade_and_reject_invalid_test_pages() {
        for (method, upgrade) in [("POST", false), ("OPTIONS", false), ("GET", true)] {
            let mut request = jsonrpsee::server::HttpRequest::builder()
                .method(method)
                .uri("/");
            if upgrade {
                request = request.header("upgrade", "websocket");
            }
            assert!(resource_response(&request.body(()).unwrap(), INSTALL_PAGE).is_none());
        }
        assert!(INSTALL_PAGE.starts_with(b"<!doctype html>"));
        assert!(INSTALL_PAGE.ends_with(b"</html>\n"));
        assert_eq!(&GMB_ICON[..8], b"\x89PNG\r\n\x1a\n");
        let credentials = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let make = || {
            RpcTls::from_der(
                vec![credentials.cert.der().clone()],
                PrivatePkcs8KeyDer::from(credentials.key_pair.serialize_der()),
            )
            .unwrap()
        };
        for page in [
            Vec::new(),
            vec![0xff],
            vec![b' '; 128 * 1024 + 1],
            b"<!doctype html>partial".to_vec(),
        ] {
            assert!(make().with_test_page(page).is_err());
        }
        let page = b"<!doctype html><html>isolated</html>\n".to_vec();
        assert_eq!(
            make().with_test_page(page.clone()).unwrap().page.as_ref(),
            page
        );
        assert_eq!(make().page.as_ref(), INSTALL_PAGE);
    }

    #[test]
    fn missing_relative_invalid_and_mismatched_credentials_are_rejected() {
        assert!(RpcTls::from_paths(Path::new("certificate.pem"), Path::new("key.der")).is_err());
        assert!(RpcTls::from_paths(
            Path::new("/nonexistent/rpc-certificate.pem"),
            Path::new("/nonexistent/rpc-key.der")
        )
        .is_err());
        assert!(RpcTls::from_der(vec![], PrivatePkcs8KeyDer::from(vec![1, 2, 3])).is_err());
        // 隔离测试材料不属于用户密钥，不写入源码或磁盘。
        let first = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let second = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        assert!(RpcTls::from_der(
            vec![first.cert.der().clone()],
            PrivatePkcs8KeyDer::from(second.key_pair.serialize_der())
        )
        .is_err());
        assert!(RpcTls::from_der(
            vec![first.cert.der().clone()],
            PrivatePkcs8KeyDer::from(first.key_pair.serialize_der())
        )
        .is_ok());
    }

    #[tokio::test]
    async fn https_wss_certificate_validation_safe_methods_and_plaintext_rejection() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        // 测试客户端显式信任隔离证书，仍执行完整 TLS 与 localhost 主机名验证。
        let credentials = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let certificate = credentials.cert.der().clone();
        let tls = RpcTls::from_der(
            vec![certificate.clone()],
            PrivatePkcs8KeyDer::from(credentials.key_pair.serialize_der()),
        )
        .unwrap();
        let reserved = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = reserved.local_addr().unwrap();
        drop(reserved);
        let mut module = RpcModule::new(());
        module.extensions_mut().insert(DenyUnsafe::No);
        module
            .register_method("eth_chainId", |_, _, _| "0x7eb")
            .unwrap();
        module
            .register_method("unsafe", |_, _, extensions| {
                sc_rpc_api::check_if_safe(extensions).map(|_| true)
            })
            .unwrap();
        let endpoint = RpcEndpoint {
            listen_addr: address,
            batch_config: jsonrpsee::server::BatchRequestConfig::Unlimited,
            max_connections: 10,
            max_payload_in_mb: 1,
            max_payload_out_mb: 1,
            max_subscriptions_per_connection: 5,
            max_buffer_capacity_per_connection: 16,
            rate_limit: None,
            rate_limit_trust_proxy_headers: false,
            rate_limit_whitelisted_ips: vec![],
            cors: Some(vec!["https://localhost".into()]),
            rpc_methods: sc_rpc_server::RpcMethods::Safe,
            is_optional: false,
            retry_random_port: false,
        };
        let server = tokio::spawn(tls.run(vec![endpoint], module));
        let client = reqwest::Client::builder()
            .https_only(true)
            .no_proxy()
            .add_root_certificate(reqwest::Certificate::from_der(certificate.as_ref()).unwrap())
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let url = format!("https://localhost:{}", address.port());
        let request =
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]});
        let response = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match client.post(&url).json(&request).send().await {
                    Ok(response) => break response.json::<serde_json::Value>().await.unwrap(),
                    Err(_) => {
                        assert!(!server.is_finished());
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(response["result"], "0x7eb");
        // 同一真实 TLS 端点交付编译期资源；GET/HEAD 不得改变 POST 与 WSS 合同。
        for (path, bytes, content_type) in [
            ("/", INSTALL_PAGE, "text/html; charset=utf-8"),
            ("/icons/gmb.png", GMB_ICON, "image/png"),
        ] {
            let resource = client
                .get(format!("{url}{path}"))
                .header("Origin", "https://localhost")
                .send()
                .await
                .unwrap();
            assert_eq!(resource.status().as_u16(), 200);
            assert_eq!(resource.headers()["content-type"], content_type);
            assert_eq!(
                resource.headers()["content-length"],
                bytes.len().to_string()
            );
            assert_eq!(resource.headers()["cache-control"], "no-store");
            assert_eq!(
                resource.headers()["access-control-allow-origin"],
                "https://localhost"
            );
            assert_eq!(resource.bytes().await.unwrap().as_ref(), bytes);
            let head = client.head(format!("{url}{path}")).send().await.unwrap();
            assert_eq!(head.status().as_u16(), 200);
            assert_eq!(head.headers()["content-length"], bytes.len().to_string());
            assert!(head.bytes().await.unwrap().is_empty());
            for (name, value) in [
                ("Origin", "https://untrusted.invalid"),
                ("Host", "untrusted.invalid"),
            ] {
                let rejected = client
                    .get(format!("{url}{path}"))
                    .header(name, value)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(rejected.status().as_u16(), 403);
            }
        }
        for path in [
            "/missing",
            "/icons/",
            "/icons/gmb.png?source=other",
            "/?rpc=other",
            "/frontend/metamask.mjs",
        ] {
            for method in [reqwest::Method::GET, reqwest::Method::HEAD] {
                let response = client
                    .request(method, format!("{url}{path}"))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(response.status().as_u16(), 404, "{path}");
                assert!(response.bytes().await.unwrap().is_empty());
            }
        }
        for (path, method) in [
            ("/", reqwest::Method::PUT),
            ("/icons/gmb.png", reqwest::Method::POST),
        ] {
            let response = client
                .request(method, format!("{url}{path}"))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), 405);
            assert_eq!(response.headers()["allow"], "GET, HEAD");
            assert!(response.bytes().await.unwrap().is_empty());
        }
        let forbidden = client
            .post(&url)
            .header("Origin", "https://untrusted.invalid")
            .json(&request)
            .send()
            .await
            .unwrap();
        assert_eq!(forbidden.status().as_u16(), 403);
        let preflight = client
            .request(reqwest::Method::OPTIONS, &url)
            .header("Origin", "https://localhost")
            .header("Access-Control-Request-Method", "POST")
            .header("Access-Control-Request-Headers", "content-type")
            .send()
            .await
            .unwrap();
        assert_eq!(
            preflight.headers()["access-control-allow-origin"],
            "https://localhost"
        );
        assert!(preflight
            .headers()
            .contains_key("access-control-allow-methods"));
        let denied: serde_json::Value = client
            .post(&url)
            .json(&serde_json::json!({
                "jsonrpc":"2.0","id":2,"method":"unsafe","params":[],
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(denied.get("error").is_some());
        assert!(client
            .post(format!("https://127.0.0.1:{}", address.port()))
            .json(&request)
            .send()
            .await
            .is_err());
        let mut plain = tokio::net::TcpStream::connect(address).await.unwrap();
        plain
            .write_all(b"POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\n\r\n{}")
            .await
            .unwrap();
        let mut buffer = [0; 128];
        let plain_result =
            tokio::time::timeout(Duration::from_secs(2), plain.read(&mut buffer)).await;
        match plain_result {
            Ok(Ok(0)) | Ok(Err(_)) => {}
            Ok(Ok(length)) => {
                // TLS 拒绝明文可先发送致命告警；只接受完整告警及断连，不能接受 HTTP/JSON。
                let mut alert = [0u8; 7];
                assert!(length <= alert.len(), "明文连接收到非 TLS 告警响应");
                alert[..length].copy_from_slice(&buffer[..length]);
                tokio::time::timeout(
                    Duration::from_secs(2),
                    plain.read_exact(&mut alert[length..]),
                )
                .await
                .unwrap()
                .unwrap();
                assert_eq!(&alert[..6], &[0x15, 0x03, 0x03, 0x00, 0x02, 0x02]);
                let closed =
                    tokio::time::timeout(Duration::from_secs(2), plain.read(&mut buffer)).await;
                assert!(
                    matches!(closed, Ok(Ok(0)) | Ok(Err(_))),
                    "TLS 致命告警后必须断连"
                );
            }
            Err(_) => panic!("明文连接必须及时拒绝"),
        }

        let mut roots = rustls::RootCertStore::empty();
        roots.add(certificate).unwrap();
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
        let mut forbidden_wss = connector
            .connect(
                rustls::pki_types::ServerName::try_from("localhost").unwrap(),
                tokio::net::TcpStream::connect(address).await.unwrap(),
            )
            .await
            .unwrap();
        forbidden_wss.write_all(format!("GET / HTTP/1.1\r\nHost: localhost:{}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nOrigin: https://untrusted.invalid\r\n\r\n", address.port()).as_bytes()).await.unwrap();
        let mut forbidden_header = Vec::new();
        while !forbidden_header.ends_with(b"\r\n\r\n") {
            forbidden_header.push(forbidden_wss.read_u8().await.unwrap());
            assert!(forbidden_header.len() < 16_384);
        }
        assert!(String::from_utf8(forbidden_header)
            .unwrap()
            .starts_with("HTTP/1.1 403"));
        drop(forbidden_wss);
        let mut wss = connector
            .connect(
                rustls::pki_types::ServerName::try_from("localhost").unwrap(),
                tokio::net::TcpStream::connect(address).await.unwrap(),
            )
            .await
            .unwrap();
        wss.write_all(format!("GET / HTTP/1.1\r\nHost: localhost:{}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nOrigin: https://localhost\r\n\r\n", address.port()).as_bytes()).await.unwrap();
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
            header.push(wss.read_u8().await.unwrap());
            assert!(header.len() < 16_384);
        }
        assert!(String::from_utf8(header)
            .unwrap()
            .starts_with("HTTP/1.1 101"));
        let payload = request.to_string();
        assert!(payload.len() < 126);
        let mask = [1u8, 2, 3, 4];
        let mut frame = vec![0x81, 0x80 | payload.len() as u8];
        frame.extend_from_slice(&mask);
        frame.extend(
            payload
                .bytes()
                .enumerate()
                .map(|(index, byte)| byte ^ mask[index % 4]),
        );
        wss.write_all(&frame).await.unwrap();
        assert_eq!(wss.read_u8().await.unwrap(), 0x81);
        let length = wss.read_u8().await.unwrap();
        assert!(length < 126);
        let mut payload = vec![0; length as usize];
        wss.read_exact(&mut payload).await.unwrap();
        let response: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(response["result"], "0x7eb");
        drop(wss);
        server.abort();
        let _ = server.await;
        assert!(tokio::net::TcpStream::connect(address).await.is_err());
    }
}
