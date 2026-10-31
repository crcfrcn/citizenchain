//! 公信链、域名、到期边界、原子更新以及真实 HTTPS 下的 ACME/DNS 生命周期回归。
use super::*;
use axum::{extract::State, response::IntoResponse};
use base64::Engine;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
};

#[derive(Default)]
struct MemoryVault(Mutex<HashMap<String, Zeroizing<String>>>);
impl Vault for MemoryVault {
    fn get(&self, name: &str) -> Result<Option<Zeroizing<String>>, String> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(name)
            .map(|v| Zeroizing::new(v.to_string())))
    }
    fn set(&self, name: &str, value: &str) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .insert(name.into(), Zeroizing::new(value.into()));
        Ok(())
    }
    fn delete(&self, name: &str) -> Result<(), String> {
        self.0.lock().unwrap().remove(name);
        Ok(())
    }
}
struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("onchina-https-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn issuer() -> (rcgen::Certificate, KeyPair, Arc<RootCertStore>) {
    let key = KeyPair::generate().unwrap();
    let mut params = CertificateParams::default();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "Isolated HTTPS test issuer");
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.key_usages = vec![
        rcgen::KeyUsagePurpose::KeyCertSign,
        rcgen::KeyUsagePurpose::CrlSign,
    ];
    let ca = params.self_signed(&key).unwrap();
    let mut roots = RootCertStore::empty();
    roots.add(ca.der().clone()).unwrap();
    (ca, key, Arc::new(roots))
}
fn certificate(
    host: &str,
    key: &KeyPair,
    ca: &rcgen::Certificate,
    ca_key: &KeyPair,
    start: i64,
    end: i64,
) -> String {
    let mut params = CertificateParams::new(vec![host.into()]).unwrap();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, host);
    params.not_before = time::OffsetDateTime::from_unix_timestamp(start).unwrap();
    params.not_after = time::OffsetDateTime::from_unix_timestamp(end).unwrap();
    params.key_usages = vec![rcgen::KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    format!(
        "{}{}",
        params.signed_by(key, ca, ca_key).unwrap().pem(),
        ca.pem()
    )
}

#[test]
fn strict_chain_hostname_expiry_and_key_validation() {
    let (ca, ca_key, roots) = issuer();
    let key = KeyPair::generate().unwrap();
    let current = now();
    let chain = certificate(
        crate::PLATFORM_HOST,
        &key,
        &ca,
        &ca_key,
        current - 60,
        current + 86400,
    );
    assert!(validate_material(chain.clone(), &key.serialize_pem(), roots.clone(), current).is_ok());
    assert!(
        validate_material(chain.clone(), &key.serialize_pem(), public_roots(), current).is_err()
    );
    assert!(validate_material(
        chain.clone(),
        &key.serialize_pem(),
        roots.clone(),
        current + 86401
    )
    .is_err());
    assert!(validate_material(
        chain.clone(),
        &key.serialize_pem(),
        roots.clone(),
        current - 61
    )
    .is_err());
    let other = KeyPair::generate().unwrap();
    assert!(validate_material(chain, &other.serialize_pem(), roots.clone(), current).is_err());
    let wrong = certificate(
        "wrong.invalid",
        &key,
        &ca,
        &ca_key,
        current - 60,
        current + 86400,
    );
    assert!(validate_material(wrong, &key.serialize_pem(), roots.clone(), current).is_err());
    assert!(validate_material(String::new(), &key.serialize_pem(), roots, current).is_err());
}

#[test]
fn stable_system_key_and_atomic_public_certificate_update() {
    let vault = MemoryVault::default();
    let dir = TestDir::new();
    let key = server_key(&vault).unwrap();
    assert!(server_key(&vault).unwrap().as_str() == key.as_str());
    let pair = KeyPair::from_pem(&key).unwrap();
    let (ca, ca_key, roots) = issuer();
    let current = now();
    let chain = certificate(
        crate::PLATFORM_HOST,
        &pair,
        &ca,
        &ca_key,
        current - 60,
        current + 86400,
    );
    let material = validate_material(chain.clone(), &key, roots.clone(), current).unwrap();
    assert!(material.renew_at > current && material.renew_at < material.expires_at);
    publish_certificate(&dir.0, &material).unwrap();
    assert_eq!(fs::read_to_string(dir.0.join(SERVER_CERT)).unwrap(), chain);
    assert!(load_material(&dir.0, &vault, roots).is_ok());
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    assert!(!fs::read_to_string(dir.0.join(SERVER_CERT))
        .unwrap()
        .contains("PRIVATE KEY"));
    vault.set(SERVER_KEY, "synthetic-corrupt-key").unwrap();
    assert!(server_key(&vault).is_err());
    assert!(vault.get(SERVER_KEY).unwrap().unwrap().as_str() == "synthetic-corrupt-key");
    assert_eq!(fs::read_to_string(dir.0.join(SERVER_CERT)).unwrap(), chain);
}

#[test]
fn invalid_files_and_authorization_fail_closed() {
    let vault = MemoryVault::default();
    let dir = TestDir::new();
    assert!(load_material(&dir.0, &vault, public_roots()).is_err());
    fs::write(
        dir.0.join(SERVER_CERT),
        vec![b'x'; CERTIFICATE_LIMIT as usize + 1],
    )
    .unwrap();
    assert!(load_material(&dir.0, &vault, public_roots()).is_err());
    #[cfg(unix)]
    {
        fs::remove_file(dir.0.join(SERVER_CERT)).unwrap();
        fs::write(dir.0.join("public.crt"), "not-a-certificate").unwrap();
        std::os::unix::fs::symlink(dir.0.join("public.crt"), dir.0.join(SERVER_CERT)).unwrap();
        assert!(load_material(&dir.0, &vault, public_roots()).is_err());
    }
    for zone_id in ["", "../escape", "z0000000000000000000000000000000"] {
        assert!(DnsAuthorization {
            zone_id: zone_id.into(),
            api_token: "synthetic".into()
        }
        .validate()
        .is_err());
    }
    assert!(DnsAuthorization {
        zone_id: "a".repeat(32),
        api_token: "synthetic\nvalue".into()
    }
    .validate()
    .is_err());
    assert!(DnsAuthorization {
        zone_id: "a".repeat(32),
        api_token: "synthetic".into()
    }
    .validate()
    .is_ok());
}

struct MockAuthority {
    base: String,
    certificate: String,
    key_der: Vec<u8>,
    records: Mutex<Vec<serde_json::Value>>,
    ready: AtomicBool,
    finalized: AtomicBool,
    stall_dns: AtomicBool,
    fail_detail: AtomicBool,
    account_count: AtomicUsize,
    csr_count: AtomicUsize,
    created: tokio::sync::Notify,
}
impl MockAuthority {
    fn order(&self, status: &str) -> serde_json::Value {
        serde_json::json!({"status":status,"authorizations":[format!("{}/authz",self.base)],"finalize":format!("{}/finalize",self.base),
            "certificate":if status == "valid" {Some(format!("{}/certificate",self.base))}else{None}})
    }
}
async fn mock_authority(
    State(state): State<Arc<MockAuthority>>,
    request: axum::extract::Request,
) -> axum::response::Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let bytes = axum::body::to_bytes(request.into_body(), 65536)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
    let mut status = axum::http::StatusCode::OK;
    let mut location = None;
    let value = match path.as_str() {
        "/directory" => {
            serde_json::json!({"newNonce":format!("{}/nonce",state.base),"newAccount":format!("{}/account",state.base),"newOrder":format!("{}/new-order",state.base)})
        }
        "/nonce" => serde_json::json!({}),
        "/account" => {
            state.account_count.fetch_add(1, Ordering::SeqCst);
            status = axum::http::StatusCode::CREATED;
            location = Some(format!("{}/account/1", state.base));
            serde_json::json!({"status":"valid"})
        }
        "/new-order" => {
            state.ready.store(false, Ordering::SeqCst);
            state.finalized.store(false, Ordering::SeqCst);
            status = axum::http::StatusCode::CREATED;
            location = Some(format!("{}/order", state.base));
            state.order("pending")
        }
        "/order" => state.order(if state.finalized.load(Ordering::SeqCst) {
            "valid"
        } else if state.ready.load(Ordering::SeqCst) {
            "ready"
        } else {
            "pending"
        }),
        "/authz" => {
            serde_json::json!({"identifier":{"type":"dns","value":crate::PLATFORM_HOST},"status":"pending","challenges":[{"type":"dns-01","url":format!("{}/challenge",state.base),"token":"synthetic-challenge","status":"pending"}]})
        }
        "/challenge" => {
            state.ready.store(true, Ordering::SeqCst);
            serde_json::json!({"type":"dns-01","url":format!("{}/challenge",state.base),"status":"valid","token":"synthetic-challenge"})
        }
        "/finalize" => {
            let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(body["payload"].as_str().unwrap())
                .unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&payload).unwrap();
            let der = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(payload["csr"].as_str().unwrap())
                .unwrap();
            use x509_parser::prelude::FromDer;
            let (_, csr) =
                x509_parser::certification_request::X509CertificationRequest::from_der(&der)
                    .unwrap();
            assert!(csr.verify_signature().is_ok());
            assert!(csr.certification_request_info.subject_pki.raw == state.key_der.as_slice());
            state.csr_count.fetch_add(1, Ordering::SeqCst);
            state.finalized.store(true, Ordering::SeqCst);
            state.order("valid")
        }
        "/certificate" => return state.certificate.clone().into_response(),
        "/dns-query" => {
            let answers = if state.stall_dns.load(Ordering::SeqCst) {
                Vec::new()
            } else {
                state.records.lock().unwrap().iter().map(|record| serde_json::json!({"type":16,"name":challenge_name(),"data":format!("\"{}\"",record["content"].as_str().unwrap())})).collect()
            };
            serde_json::json!({"Status":0,"Answer":answers})
        }
        _ if path.ends_with("/dns_records") && method == reqwest::Method::POST => {
            let mut record = body;
            record["id"] = serde_json::Value::String("a".repeat(32));
            state.records.lock().unwrap().push(record.clone());
            state.created.notify_one();
            serde_json::json!({"success":true,"result":record})
        }
        _ if path.ends_with("/dns_records") && method == reqwest::Method::GET => {
            serde_json::json!({"success":true,"result":state.records.lock().unwrap().clone()})
        }
        _ if method == reqwest::Method::GET
            && path.ends_with(&"a".repeat(32))
            && state.fail_detail.load(Ordering::SeqCst) =>
        {
            status = axum::http::StatusCode::BAD_REQUEST;
            serde_json::json!({"success":false})
        }
        _ if method == reqwest::Method::GET && path.ends_with(&"a".repeat(32)) => {
            let record = state
                .records
                .lock()
                .unwrap()
                .iter()
                .find(|record| record["id"] == "a".repeat(32))
                .cloned();
            if let Some(record) = record {
                serde_json::json!({"success":true,"result":record})
            } else {
                status = axum::http::StatusCode::NOT_FOUND;
                serde_json::json!({})
            }
        }
        _ if method == reqwest::Method::DELETE => {
            assert!(path.ends_with(&"a".repeat(32)));
            state
                .records
                .lock()
                .unwrap()
                .retain(|record| record["id"] != "a".repeat(32));
            serde_json::json!({"success":true,"result":{}})
        }
        _ => {
            status = axum::http::StatusCode::NOT_FOUND;
            serde_json::json!({})
        }
    };
    let mut response = (status, axum::Json(value)).into_response();
    response.headers_mut().insert(
        "replay-nonce",
        axum::http::HeaderValue::from_static("synthetic-nonce"),
    );
    if let Some(location) = location {
        response
            .headers_mut()
            .insert("location", location.parse().unwrap());
    }
    response
}
struct MockServer {
    state: Arc<MockAuthority>,
    client: reqwest::Client,
    roots: Arc<RootCertStore>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for MockServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl MockServer {
    async fn new(vault: &MemoryVault) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (ca, ca_key, roots) = issuer();
        let key = server_key(vault).unwrap();
        let pair = KeyPair::from_pem(&key).unwrap();
        let current = now();
        let issued = certificate(
            crate::PLATFORM_HOST,
            &pair,
            &ca,
            &ca_key,
            current - 60,
            current + 86400,
        );
        let transport_key = KeyPair::generate().unwrap();
        let transport_certificate = certificate(
            "localhost",
            &transport_key,
            &ca,
            &ca_key,
            current - 60,
            current + 86400,
        );
        let config = RustlsConfig::from_pem(
            transport_certificate.into_bytes(),
            transport_key.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let state = Arc::new(MockAuthority {
            base: format!("https://localhost:{port}"),
            certificate: issued,
            key_der: pair.public_key_der(),
            records: Mutex::new(vec![
                serde_json::json!({"type":"TXT","name":challenge_name(),"content":"unrelated-record","id":"b".repeat(32)}),
            ]),
            ready: AtomicBool::new(false),
            finalized: AtomicBool::new(false),
            stall_dns: AtomicBool::new(false),
            fail_detail: AtomicBool::new(false),
            account_count: AtomicUsize::new(0),
            csr_count: AtomicUsize::new(0),
            created: tokio::sync::Notify::new(),
        });
        let app = axum::Router::new()
            .fallback(mock_authority)
            .with_state(state.clone());
        let task = tokio::spawn(async move {
            axum_server::from_tcp_rustls(listener, config)
                .unwrap()
                .serve(app.into_make_service())
                .await
                .unwrap();
        });
        let client = reqwest::Client::builder()
            .no_proxy()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .tls_certs_only([reqwest::Certificate::from_pem(ca.pem().as_bytes()).unwrap()])
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        Self {
            state,
            client,
            roots,
            task,
        }
    }
    fn provider(&self) -> DnsProvider {
        DnsProvider {
            client: self.client.clone(),
            authorization: DnsAuthorization {
                zone_id: "c".repeat(32),
                api_token: "synthetic-test-authorization".into(),
            },
            api: self.state.base.clone(),
        }
    }
}

#[tokio::test]
async fn acme_https_issue_reuses_account_and_key_and_cleans_only_owned_txt() {
    let vault = MemoryVault::default();
    let server = MockServer::new(&vault).await;
    let provider = server.provider();
    let (stop, cancel) = watch::channel(false);
    let dir = TestDir::new();
    for _ in 0..2 {
        let material = issue_process(
            &vault,
            &provider,
            &format!("{}/directory", server.state.base),
            &format!("{}/dns-query", server.state.base),
            server.roots.clone(),
            cancel.clone(),
        )
        .await
        .unwrap();
        publish_certificate(&dir.0, &material).unwrap();
        assert!(load_material(&dir.0, &vault, server.roots.clone()).is_ok());
        assert!(vault.get(PENDING_DNS).unwrap().is_none());
        assert_eq!(server.state.records.lock().unwrap().len(), 1);
    }
    assert_eq!(server.state.account_count.load(Ordering::SeqCst), 1);
    assert_eq!(server.state.csr_count.load(Ordering::SeqCst), 2);
    drop(stop);
}

#[tokio::test]
async fn cancelling_dns_wait_cleans_challenge_and_preserves_current_certificate() {
    let vault = Arc::new(MemoryVault::default());
    let server = MockServer::new(&vault).await;
    server.state.stall_dns.store(true, Ordering::SeqCst);
    let directory = TestDir::new();
    fs::write(directory.0.join(SERVER_CERT), &server.state.certificate).unwrap();
    let (stop, cancel) = watch::channel(false);
    let provider = server.provider();
    let task_vault = vault.clone();
    let roots = server.roots.clone();
    let base = server.state.base.clone();
    let task = tokio::spawn(async move {
        issue_process(
            task_vault.as_ref(),
            &provider,
            &format!("{base}/directory"),
            &format!("{base}/dns-query"),
            roots,
            cancel,
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(5), server.state.created.notified())
        .await
        .unwrap();
    stop.send(true).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert!(result.is_err());
    assert!(vault.get(PENDING_DNS).unwrap().is_none());
    assert_eq!(server.state.records.lock().unwrap().len(), 1);
    assert_eq!(
        fs::read_to_string(directory.0.join(SERVER_CERT)).unwrap(),
        server.state.certificate
    );
    assert!(load_material(&directory.0, vault.as_ref(), server.roots.clone()).is_ok());
}

#[tokio::test]
async fn missing_authorization_fails_without_writing_disk_or_replacing_keys() {
    let vault = MemoryVault::default();
    let (stop, cancel) = watch::channel(false);
    assert!(issue(&vault, public_roots(), cancel).await.is_err());
    assert!(vault.get(SERVER_KEY).unwrap().is_none());
    drop(stop);
}

#[tokio::test]
async fn cleanup_preserves_owner_edits_and_accepts_already_removed_record() {
    let vault = MemoryVault::default();
    let server = MockServer::new(&vault).await;
    let provider = server.provider();
    let pending = PendingDns {
        zone_id: "c".repeat(32),
        value: "original-challenge".into(),
        record_id: Some("a".repeat(32)),
        created_at: now(),
    };
    server.state.records.lock().unwrap().push(serde_json::json!({"id":"a".repeat(32),"type":"TXT","name":challenge_name(),"content":"owner-edited-value"}));
    save_pending(&vault, &pending).unwrap();
    provider.cleanup(&vault).await.unwrap();
    assert_eq!(server.state.records.lock().unwrap().len(), 2);
    assert!(vault.get(PENDING_DNS).unwrap().is_none());
    server
        .state
        .records
        .lock()
        .unwrap()
        .retain(|record| record["id"] != "a".repeat(32));
    server.state.fail_detail.store(true, Ordering::SeqCst);
    save_pending(&vault, &pending).unwrap();
    provider.cleanup(&vault).await.unwrap();
    assert_eq!(server.state.records.lock().unwrap().len(), 1);
    assert!(vault.get(PENDING_DNS).unwrap().is_none());
}
