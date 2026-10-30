use crate::publication::check_rpc_certificate_acl;
use crate::publication::check_rpc_certificate_file;
use crate::publication::formal_rpc_certificate_profile;
use crate::publication::publish_rpc_certificate;
use crate::publication::publish_rpc_certificate_using;
use crate::publication::RPC_CERTIFICATE_FILE;
use std::fs;
use std::os::unix::fs::symlink;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        // 夹具归调用方交付的临时目录；本轮TMPDIR为独占target/test现场。
        let path = std::env::temp_dir().join(format!(
            "rpc-public-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir_all(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path.canonicalize().unwrap())
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn public_certificate_profile_does_not_publish_development_identity() {
    for profile in ["prod", "production", "release"] {
        assert_eq!(
            formal_rpc_certificate_profile(Some(profile), true).unwrap(),
            true
        );
    }
    for profile in ["dev", "development", "debug"] {
        assert_eq!(
            formal_rpc_certificate_profile(Some(profile), false).unwrap(),
            false
        );
    }
    assert!(!formal_rpc_certificate_profile(None, true).unwrap());
    assert!(formal_rpc_certificate_profile(None, false).unwrap());
    assert!(formal_rpc_certificate_profile(Some("unknown"), false).is_err());
}

#[test]
fn public_certificate_is_atomic_restricted_and_replaced_without_exporting_private_material() {
    let directory = crate::publication::certificate_tests::Directory::new();
    let first = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let second = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let path = directory.0.join(RPC_CERTIFICATE_FILE);
    publish_rpc_certificate(&directory.0, first.cert.der()).unwrap();
    check_rpc_certificate_file(&fs::symlink_metadata(&path).unwrap()).unwrap();
    assert_eq!(
        hex::decode(fs::read_to_string(&path).unwrap()).unwrap(),
        first.cert.der().as_ref()
    );
    publish_rpc_certificate(&directory.0, second.cert.der()).unwrap();
    assert_eq!(
        hex::decode(fs::read_to_string(&path).unwrap()).unwrap(),
        second.cert.der().as_ref()
    );
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
}

#[test]
fn public_certificate_rejects_links_types_permissions_and_invalid_size() {
    let directory = crate::publication::certificate_tests::Directory::new();
    let credentials = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let cert = credentials.cert.der();
    let path = directory.0.join(RPC_CERTIFICATE_FILE);
    let other = directory.0.join("other");
    fs::write(&other, b"unchanged").unwrap();
    fs::set_permissions(&other, fs::Permissions::from_mode(0o600)).unwrap();
    symlink(&other, &path).unwrap();
    assert!(publish_rpc_certificate(&directory.0, cert).is_err());
    assert_eq!(fs::read(&other).unwrap(), b"unchanged");
    fs::remove_file(&path).unwrap();
    fs::hard_link(&other, &path).unwrap();
    assert!(publish_rpc_certificate(&directory.0, cert).is_err());
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(publish_rpc_certificate(&directory.0, cert).is_err());
    fs::remove_dir(&path).unwrap();
    publish_rpc_certificate(&directory.0, cert).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(publish_rpc_certificate(&directory.0, cert).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(publish_rpc_certificate(&directory.0, cert).is_err());
    fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(publish_rpc_certificate(&directory.0, &[]).is_err());
    assert!(publish_rpc_certificate(&directory.0, &vec![0; 32 * 1024 + 1]).is_err());
    assert!(publish_rpc_certificate(&directory.0.join("missing"), cert).is_err());
    let linked = directory.0.join("linked");
    symlink(&directory.0, &linked).unwrap();
    assert!(publish_rpc_certificate(&linked, cert).is_err());
}

#[test]
fn public_certificate_acl_is_closed_and_ancestor_write_grants_are_rejected() {
    assert!(check_rpc_certificate_acl("directory\n", true).is_ok());
    assert!(
        check_rpc_certificate_acl("directory\n 0: group:everyone deny delete\n", false).is_ok()
    );
    assert!(
        check_rpc_certificate_acl("directory\n 0: user:fixture allow read,search\n", false).is_ok()
    );
    for (output, private) in [
        ("", false),
        ("directory\nunknown\n", false),
        ("directory\n 0: user:fixture allow write\n", false),
        ("directory\n 0: user:fixture deny name allow write\n", false),
        ("directory\n 0: group:everyone deny delete\n", true),
    ] {
        assert!(check_rpc_certificate_acl(output, private).is_err());
    }
}

#[test]
fn public_certificate_write_failure_is_not_reported_as_delivery() {
    let directory = crate::publication::certificate_tests::Directory::new();
    let credentials = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let error =
        publish_rpc_certificate_using(&directory.0, credentials.cert.der(), |path, text| {
            assert_eq!(path, directory.0.join(RPC_CERTIFICATE_FILE));
            assert_eq!(text, hex::encode(credentials.cert.der()));
            Err("fixture write failure".into())
        })
        .unwrap_err();
    assert_eq!(error, "fixture write failure");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
}

#[test]
fn public_certificate_real_node_https_accepts_only_current_valid_loopback_identity() {
    use rustls::pki_types::PrivatePkcs8KeyDer;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::Arc,
        time::{Duration, Instant},
    };
    // 只使用产品已交付的Node；测试证书私钥在本Rust测试进程内生成、使用、销毁。
    let node = std::env::var_os("NODE").expect("公开证书TLS回归缺少产品已交付的NODE");
    let entry = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts/runtime.mjs");
    let code = r#"
import {pathToFileURL} from 'node:url';
const [entry,directory,endpoint,expected] = process.argv.slice(2);
const {readRuntimeCertificate,rpcRequest} = await import(pathToFileURL(entry));
const certificate = readRuntimeCertificate(directory);
try {
  const value = await rpcRequest(endpoint,'chain_getFinalizedHead',[],{certificate});
  if (expected !== 'accepted' || value !== '0x'+'11'.repeat(32)) throw Error('TLS正例结果不符');
  process.stdout.write('accepted');
} catch (error) {
  if (expected === 'accepted' || !['DEPTH_ZERO_SELF_SIGNED_CERT','CERT_HAS_EXPIRED','ERR_TLS_CERT_ALTNAME_INVALID'].includes(error.code)) throw error;
  process.stdout.write('rejected');
}
"#;
    for scenario in ["accepted", "wrong", "hostname", "expired", "old"] {
        let directory = crate::publication::certificate_tests::Directory::new();
        let mut params = rcgen::CertificateParams::new(vec![(if scenario == "hostname" {
            "localhost"
        } else {
            "127.0.0.1"
        })
        .into()])
        .unwrap();
        if scenario == "expired" {
            params.not_before = rcgen::date_time_ymd(2010, 1, 1);
            params.not_after = rcgen::date_time_ymd(2020, 1, 1);
        }
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = params.self_signed(&key).unwrap();
        let other = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        if scenario == "old" {
            publish_rpc_certificate(&directory.0, cert.der()).unwrap();
        }
        publish_rpc_certificate(
            &directory.0,
            if matches!(scenario, "wrong" | "old") {
                other.cert.der()
            } else {
                cert.der()
            },
        )
        .unwrap();
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.der().clone()],
            PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
        )
        .unwrap();
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let config = Arc::new(config);
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(15);
            while Instant::now() < deadline {
                match listener.accept() {
                    Ok((socket, _)) => {
                        // macOS接受的socket可能继承监听器非阻塞状态；本夹具使用带超时的阻塞rustls流。
                        socket.set_nonblocking(false).unwrap();
                        socket
                            .set_read_timeout(Some(Duration::from_secs(3)))
                            .unwrap();
                        socket
                            .set_write_timeout(Some(Duration::from_secs(3)))
                            .unwrap();
                        let connection = rustls::ServerConnection::new(config.clone()).unwrap();
                        let mut tls = rustls::StreamOwned::new(connection, socket);
                        let mut request = Vec::new();
                        let mut bytes = [0; 1024];
                        loop {
                            match tls.read(&mut bytes) {
                                Ok(0) => return Some("请求正文未完成即EOF".to_string()),
                                Err(error) => return Some(format!("读取TLS请求失败: {error}")),
                                Ok(size) => request.extend_from_slice(&bytes[..size]),
                            }
                            if request.len() > 8192 {
                                return Some("请求超过夹具边界".into());
                            }
                            // 正式HTTPS客户端可能分记录发送头和正文；消费完整POST，避免未读数据导致TCP RST。
                            if let Some(end) =
                                request.windows(4).position(|part| part == b"\r\n\r\n")
                            {
                                let headers = String::from_utf8_lossy(&request[..end]);
                                let length = headers
                                    .lines()
                                    .filter_map(|line| line.split_once(':'))
                                    .find(|(name, _)| name.eq_ignore_ascii_case("content-length"));
                                if let Some((_, length)) = length {
                                    let Ok(length) = length.trim().parse::<usize>() else {
                                        return Some("请求长度无效".into());
                                    };
                                    if length > 8192 {
                                        return Some("请求正文超过夹具边界".into());
                                    }
                                    if request.len() >= end + 4 + length {
                                        break;
                                    }
                                } else if headers.lines().any(|line| {
                                    line.eq_ignore_ascii_case("transfer-encoding: chunked")
                                }) {
                                    let body = &request[end + 4..];
                                    if body == b"0\r\n\r\n" || body.ends_with(b"\r\n0\r\n\r\n") {
                                        break;
                                    }
                                } else {
                                    return Some("POST请求缺少长度或分块边界".into());
                                }
                            }
                        }
                        let body = format!(
                            "{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"0x{}\"}}",
                            "11".repeat(32)
                        );
                        let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                        if let Err(error) = tls.write_all(response.as_bytes()) {
                            return Some(format!("写TLS响应失败: {error}"));
                        }
                        tls.conn.send_close_notify();
                        return tls
                            .flush()
                            .err()
                            .map(|error| format!("关闭TLS响应失败: {error}"));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(20))
                    }
                    Err(error) => return Some(format!("接收TLS连接失败: {error}")),
                }
            }
            Some("等待TLS请求超时".into())
        });
        let expected = if scenario == "accepted" {
            "accepted"
        } else {
            "rejected"
        };
        let output = std::process::Command::new(&node)
            .args(["--input-type=module", "-e", code])
            .arg("rpc-certificate-fixture")
            .arg(&entry)
            .arg(&directory.0)
            .arg(format!("https://127.0.0.1:{port}"))
            .arg(expected)
            .env_remove("NODE_OPTIONS")
            .env_remove("NODE_PATH")
            .env_remove("NODE_TEST_CONTEXT")
            .output()
            .unwrap();
        let server_error = server.join().unwrap();
        // 子进程只接收公开证书与临时端点；保留其错误帮助区分TLS失败和夹具衔接失败。
        assert!(
            output.status.success(),
            "公开证书真实TLS场景失败: {scenario}: {}; server: {server_error:?}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
    }
}
