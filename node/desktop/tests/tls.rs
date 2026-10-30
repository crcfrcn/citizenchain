use super::*;

use crate::tls::desktop_rpc_certificate;
use crate::tls::desktop_rpc_tls;
use crate::tls::prepare_desktop_rpc_tls;
use crate::tls::resource_response;
use crate::tls::GMB_ICON;
use crate::tls::INSTALL_PAGE;
use sc_rpc_server::RpcEndpoint;

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
        crate::tls::RpcTls::from_der(
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
    assert!(
        crate::tls::RpcTls::from_paths(Path::new("certificate.pem"), Path::new("key.der")).is_err()
    );
    assert!(crate::tls::RpcTls::from_paths(
        Path::new("/nonexistent/rpc-certificate.pem"),
        Path::new("/nonexistent/rpc-key.der")
    )
    .is_err());
    assert!(crate::tls::RpcTls::from_der(vec![], PrivatePkcs8KeyDer::from(vec![1, 2, 3])).is_err());
    // 隔离测试材料不属于用户密钥，不写入源码或磁盘。
    let first = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let second = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    assert!(crate::tls::RpcTls::from_der(
        vec![first.cert.der().clone()],
        PrivatePkcs8KeyDer::from(second.key_pair.serialize_der())
    )
    .is_err());
    assert!(crate::tls::RpcTls::from_der(
        vec![first.cert.der().clone()],
        PrivatePkcs8KeyDer::from(first.key_pair.serialize_der())
    )
    .is_ok());
}

#[tokio::test]
async fn https_wss_certificate_validation_safe_methods_and_plaintext_rejection() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    // 三平台桌面实际会话证书不依赖运行环境文件；重启节点继续信任同一证书。
    prepare_desktop_rpc_tls().unwrap();
    let certificate = desktop_rpc_certificate().unwrap().to_vec();
    prepare_desktop_rpc_tls().unwrap();
    assert_eq!(desktop_rpc_certificate().unwrap(), certificate.as_slice());
    let tls = desktop_rpc_tls().unwrap();
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
    module
        .register_method("system_health", |_, _, _| serde_json::json!({"peers":0}))
        .unwrap();
    for method in ["system_peers", "system_unstable_networkState"] {
        module
            .register_method(method, move |_, _, extensions| {
                sc_rpc_api::check_if_safe(extensions).map(|_| {
                    if method == "system_peers" {
                        serde_json::json!([])
                    } else {
                        serde_json::json!({"connectedPeers":{}})
                    }
                })
            })
            .unwrap();
    }
    let internal = nodeclient::network::LocalNetworkReader::new();
    internal
        .attach(Arc::new(module.clone()), tokio::runtime::Handle::current())
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
    let mut server = tokio::spawn(tls.run(vec![endpoint], module));
    let client = reqwest::Client::builder()
        .https_only(true)
        .no_proxy()
        .add_root_certificate(reqwest::Certificate::from_der(&certificate).unwrap())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let url = format!("https://127.0.0.1:{}", address.port());
    let request = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]});
    let response = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match client.post(&url).json(&request).send().await {
                Ok(response) => break response.json::<serde_json::Value>().await.unwrap(),
                Err(_) => {
                    if server.is_finished() {
                        panic!("TLS测试服务在就绪前退出：{:?}", (&mut server).await);
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(response["result"], "0x7eb");
    // 桌面正式使用的阻塞客户端也须验证自动生成证书与回环IP。
    let blocking_certificate = certificate.clone();
    let blocking_request = request.clone();
    let blocking_url = format!("https://127.0.0.1:{}", address.port());
    let blocking_response = tokio::task::spawn_blocking(move || {
        // 阻塞客户端的创建、请求和析构都必须位于阻塞线程，不能嵌套异步 runtime。
        let blocking_client = crate::rpc::build_local_rpc_client(&blocking_certificate).unwrap();
        blocking_client
            .post(blocking_url)
            .json(&blocking_request)
            .send()
            .unwrap()
            .json::<serde_json::Value>()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(blocking_response["result"], "0x7eb");
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
        "/frontend/desktop/metamask.mjs",
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
    for method in ["unsafe", "system_peers", "system_unstable_networkState"] {
        let denied: serde_json::Value = client
            .post(&url)
            .json(&serde_json::json!({
                "jsonrpc":"2.0","id":2,"method":method,"params":[],
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(denied["error"]["code"], -32601);
        assert_eq!(
            denied["error"]["message"],
            "RPC call is unsafe to be called externally"
        );
    }
    let local_reader = internal.clone();
    let snapshot = tokio::task::spawn_blocking(move || local_reader.sync_snapshot())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.peers, serde_json::json!([]));
    let localhost_response = client
        .post(format!("https://localhost:{}", address.port()))
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(localhost_response.status().as_u16(), 200);
    let wrong_host = reqwest::Client::builder()
        .https_only(true)
        .no_proxy()
        .add_root_certificate(reqwest::Certificate::from_der(&certificate).unwrap())
        .resolve("wrong-localhost.invalid", address)
        .build()
        .unwrap();
    assert!(wrong_host
        .post(format!(
            "https://wrong-localhost.invalid:{}",
            address.port()
        ))
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
    let plain_result = tokio::time::timeout(Duration::from_secs(2), plain.read(&mut buffer)).await;
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
    roots.add(CertificateDer::from(certificate)).unwrap();
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
    for method in ["system_peers", "system_unstable_networkState"] {
        let payload =
            serde_json::json!({"jsonrpc":"2.0","id":3,"method":method,"params":[]}).to_string();
        assert!(payload.len() < 126);
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
        let length = if length == 126 {
            wss.read_u16().await.unwrap() as usize
        } else {
            length as usize
        };
        assert!(length < 4096);
        let mut response = vec![0; length];
        wss.read_exact(&mut response).await.unwrap();
        let response: serde_json::Value = serde_json::from_slice(&response).unwrap();
        assert_eq!(response["error"]["code"], -32601);
    }
    drop(wss);
    internal.close();
    server.abort();
    let _ = server.await;
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
}
