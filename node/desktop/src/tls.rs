use futures::future::try_join_all;
use jsonrpsee::server::serve_with_graceful_shutdown;
use jsonrpsee::server::stop_channel;
use jsonrpsee::server::ServerHandle;
use jsonrpsee::RpcModule;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::PrivatePkcs8KeyDer;
use sc_rpc_api::DenyUnsafe;
use sc_rpc_server::MiddlewareLayer;
use sc_rpc_server::RpcEndpoint;
use sc_rpc_server::RpcServiceBuilder;
use std::path::Path;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_rustls::TlsAcceptor;
use tower::Service;
use tower_http::cors::AllowOrigin;
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;

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
#[derive(Clone)]
pub struct RpcTls {
    acceptor: TlsAcceptor,
    page: Arc<[u8]>,
}

pub const INSTALL_PAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/metamask.html"));

const GMB_ICON: &[u8] = include_bytes!("../../../icons/gmb.png");

/// 桌面三个平台在一次 App 会话中共用同一份本机证书；节点手动重启不轮换信任锚。
static DESKTOP_RPC_TLS: OnceLock<DesktopRpcTls> = OnceLock::new();

struct DesktopRpcTls {
    server: RpcTls,
    certificate: Vec<u8>,
}

/// 只在桌面启动路径调用；私钥留在进程内存，公开证书由桌面入口受控交付，不复用 P2P 或生产回源证书。
pub fn prepare_desktop_rpc_tls() -> Result<(), String> {
    if DESKTOP_RPC_TLS.get().is_some() {
        return Ok(());
    }
    let mut params = rcgen::CertificateParams::new(vec!["localhost".into()])
        .map_err(|error| format!("生成本机 RPC 证书参数失败: {error}"))?;
    params
        .subject_alt_names
        .push(rcgen::SanType::IpAddress(std::net::IpAddr::V4(
            std::net::Ipv4Addr::LOCALHOST,
        )));
    let key =
        rcgen::KeyPair::generate().map_err(|error| format!("生成本机 RPC 密钥失败: {error}"))?;
    let cert = params
        .self_signed(&key)
        .map_err(|error| format!("生成本机 RPC 证书失败: {error}"))?;
    let certificate = cert.der().to_vec();
    let server = crate::tls::RpcTls::from_der(
        vec![cert.der().clone()],
        PrivatePkcs8KeyDer::from(key.serialize_der()),
    )
    .map_err(|error| format!("装载本机 RPC 证书失败: {error}"))?;
    let _ = DESKTOP_RPC_TLS.set(DesktopRpcTls {
        server,
        certificate,
    });
    Ok(())
}

/// 桌面节点服务与本机客户端使用同一会话身份；未准备时拒绝启动。
pub fn desktop_rpc_tls() -> Result<RpcTls, String> {
    DESKTOP_RPC_TLS
        .get()
        .map(|identity| identity.server.clone())
        .ok_or_else(|| "桌面 RPC 证书尚未准备".into())
}

pub fn desktop_rpc_certificate() -> Option<&'static [u8]> {
    DESKTOP_RPC_TLS
        .get()
        .map(|identity| identity.certificate.as_slice())
}

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
    /// 无界面 CLI 只读取部署方的证书路径，缺失时拒绝启动。
    pub fn load() -> Result<Self, RpcError> {
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

    pub fn from_der(
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

    /// 使用既有端点和限额运行 TLS；所有监听、连接和订阅随此 Future 取消而关闭。
    pub async fn run(
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
#[path = "../tests/tls.rs"]
mod tests;

#[cfg(any(test, feature = "test-support"))]
#[path = "../tests/support.rs"]
mod testing;
