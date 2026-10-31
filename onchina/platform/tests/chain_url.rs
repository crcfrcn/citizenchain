//! 原所属模块回归，断言与输入保持。
use super::{decode_desktop_rpc_certificate, trusted_websocket_config};

#[test]
fn desktop_chain_certificate_is_bound_to_loopback_and_valid_hex() {
    assert_eq!(
        decode_desktop_rpc_certificate("010203", "wss://127.0.0.1:9944/").unwrap(),
        vec![1, 2, 3]
    );
    for url in [
        "wss://example.com:9944/",
        "wss://localhost:9944/",
        "ws://127.0.0.1:9944/",
        "not-a-url",
    ] {
        assert!(decode_desktop_rpc_certificate("010203", url).is_err());
    }
    assert!(decode_desktop_rpc_certificate("xyz", "wss://127.0.0.1:9944/").is_err());
    let generated = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    assert!(trusted_websocket_config(generated.cert.der().to_vec()).is_ok());
    assert!(trusted_websocket_config(vec![1, 2, 3]).is_err());
}
#[tokio::test]
async fn rpc_https_uses_exact_certificate_and_validates_hostname() {
    use std::{
        io::{Read, Write},
        sync::Arc,
        time::{Duration, Instant},
    };
    let params = rcgen::CertificateParams::new(vec!["127.0.0.1".into()]).unwrap();
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = params.self_signed(&key).unwrap();
    let certificate = cert.der().to_vec();
    let server = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![rustls::pki_types::CertificateDer::from(certificate.clone())],
        rustls::pki_types::PrivateKeyDer::Pkcs8(key.serialize_der().into()),
    )
    .unwrap();
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let thread = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(10);
        let config = Arc::new(server);
        let mut accepted = 0;
        while accepted < 3 && Instant::now() < until {
            match listener.accept() {
                Ok((socket, _)) => {
                    accepted += 1;
                    // macOS接受的连接会继承非阻塞模式；同步TLS读写前恢复阻塞，仍保留原超时。
                    socket.set_nonblocking(false).unwrap();
                    socket
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    socket
                        .set_write_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut tls = rustls::StreamOwned::new(
                        rustls::ServerConnection::new(config.clone()).unwrap(),
                        socket,
                    );
                    let mut request = [0; 2048];
                    if tls.read(&mut request).is_ok() {
                        let _ = tls.write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                        );
                        let _ = tls.flush();
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(error) => panic!("test TLS listener failed: {error}"),
            }
        }
    });
    let trusted = super::build_rpc_http_client(Some(certificate.clone())).unwrap();
    let url = format!("https://127.0.0.1:{port}");
    assert!(trusted
        .post(&url)
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .unwrap()
        .status()
        .is_success());
    let other = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let untrusted = super::build_rpc_http_client(Some(other.cert.der().to_vec())).unwrap();
    assert!(untrusted
        .post(&url)
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .is_err());
    assert!(trusted
        .post(format!("https://localhost:{port}"))
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .is_err());
    thread.join().unwrap();
}
