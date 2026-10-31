//! 固定局域网入口的公信 HTTPS：标准 ACME、DNS-01、系统密钥存储和证书热加载。
//! 私钥、账户和办证授权只存系统密钥库；磁盘只保存公开证书链。客户端无需导入 CA。

use axum_server::tls_rustls::RustlsConfig;
use bytes::Bytes;
use http_body_util::BodyExt;
use instant_acme::{
    Account, AuthorizationStatus, ChallengeType, Identifier, NewAccount, NewOrder, OrderStatus,
    RetryPolicy,
};
use rcgen::{CertificateParams, KeyPair};
use rustls::{
    client::danger::ServerCertVerifier,
    pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer, ServerName, UnixTime},
    RootCertStore, ServerConfig,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::watch;
use zeroize::{Zeroize, Zeroizing};

const KEYRING_SERVICE: &str = crate::PLATFORM_HOST;
const AUTHORIZATION: &str = "dns-authorization";
const ACCOUNT: &str = "acme-account";
const SERVER_KEY: &str = "server-key";
const PENDING_DNS: &str = "pending-dns";
const SERVER_CERT: &str = "onchina-server.crt";
const ACME_DIRECTORY: &str = "https://acme-v02.api.letsencrypt.org/directory";
const CLOUDFLARE_API: &str = "https://api.cloudflare.com/client/v4";
const DNS_QUERY: &str = "https://cloudflare-dns.com/dns-query";
const CERTIFICATE_LIMIT: u64 = 64 * 1024;
const RESPONSE_LIMIT: usize = 1024 * 1024;
const RENEW_INTERVAL: Duration = Duration::from_secs(6 * 3600);

// 抽象仅隔离系统密钥存储，生产始终使用系统实现；测试不接触用户的真实密钥。
trait Vault: Send + Sync {
    fn get(&self, name: &str) -> Result<Option<Zeroizing<String>>, String>;
    fn set(&self, name: &str, value: &str) -> Result<(), String>;
    fn delete(&self, name: &str) -> Result<(), String>;
}
struct SystemVault;
impl Vault for SystemVault {
    fn get(&self, name: &str) -> Result<Option<Zeroizing<String>>, String> {
        let entry =
            keyring::Entry::new(KEYRING_SERVICE, name).map_err(|_| "无法访问链上中国系统密钥库")?;
        match entry.get_password() {
            Ok(value) => Ok(Some(Zeroizing::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err("读取链上中国系统密钥库失败".into()),
        }
    }
    fn set(&self, name: &str, value: &str) -> Result<(), String> {
        keyring::Entry::new(KEYRING_SERVICE, name)
            .map_err(|_| "无法访问链上中国系统密钥库")?
            .set_password(value)
            .map_err(|_| "保存链上中国系统密钥失败".into())
    }
    fn delete(&self, name: &str) -> Result<(), String> {
        let entry =
            keyring::Entry::new(KEYRING_SERVICE, name).map_err(|_| "无法访问链上中国系统密钥库")?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("清理链上中国系统密钥条目失败".into()),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DnsAuthorization {
    zone_id: String,
    api_token: String,
}
impl Drop for DnsAuthorization {
    fn drop(&mut self) {
        self.api_token.zeroize();
    }
}
impl DnsAuthorization {
    fn validate(&self) -> Result<(), String> {
        if !valid_record_id(&self.zone_id)
            || self.api_token.is_empty()
            || self.api_token.len() > 1024
            || !self.api_token.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err("Cloudflare 区域 ID 或办证授权格式无效".into());
        }
        Ok(())
    }
}
fn valid_record_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// 域名所有者在主机执行一次；输入隐藏，不经参数、环境变量、日志或配置文件交付机密。
pub(crate) fn authorize() -> Result<(), String> {
    let zone_id =
        rpassword::prompt_password("Cloudflare 区域 ID：").map_err(|_| "读取区域 ID 失败")?;
    let api_token = rpassword::prompt_password("仅限该区域 DNS 编辑的办证授权：")
        .map_err(|_| "读取办证授权失败")?;
    let authorization = DnsAuthorization {
        zone_id: zone_id.trim().into(),
        api_token,
    };
    authorization.validate()?;
    let encoded =
        Zeroizing::new(serde_json::to_string(&authorization).map_err(|_| "编码办证授权失败")?);
    SystemVault.set(AUTHORIZATION, &encoded)?;
    println!("办证授权已保存至本机系统密钥库。启动链上中国后自动申请及续期证书。");
    Ok(())
}

/// 公开证书目录与节点数据同根；独立执行也不写应用安装目录。
pub(crate) fn tls_dir() -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os("ONCHINA_TLS_DIR").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(value));
    }
    #[cfg(target_os = "macos")]
    let base = PathBuf::from(std::env::var_os("HOME").ok_or("用户目录不可用")?)
        .join("Library/Application Support");
    #[cfg(target_os = "windows")]
    let base = PathBuf::from(std::env::var_os("APPDATA").ok_or("用户应用数据目录不可用")?);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let base = match std::env::var_os("XDG_DATA_HOME") {
        Some(value) => PathBuf::from(value),
        None => {
            PathBuf::from(std::env::var_os("HOME").ok_or("用户目录不可用")?).join(".local/share")
        }
    };
    Ok(base.join("gmb/onchina-tls"))
}
fn public_roots() -> Arc<RootCertStore> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    Arc::new(roots)
}

struct Material {
    certificate: String,
    config: Arc<ServerConfig>,
    renew_at: i64,
    expires_at: i64,
}
fn validate_material(
    certificate: String,
    key: &str,
    roots: Arc<RootCertStore>,
    now: i64,
) -> Result<Material, String> {
    let certificates = CertificateDer::pem_slice_iter(certificate.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "HTTPS 证书链格式无效")?;
    let leaf = certificates.first().ok_or("HTTPS 证书链为空")?;
    let name = ServerName::try_from(crate::PLATFORM_HOST).map_err(|_| "HTTPS 域名声明无效")?;
    let verifier = rustls::client::WebPkiServerVerifier::builder_with_provider(
        roots,
        Arc::new(rustls::crypto::ring::default_provider()),
    )
    .build()
    .map_err(|_| "创建公信证书验证器失败")?;
    let timestamp = u64::try_from(now).map_err(|_| "系统时间无效")?;
    verifier
        .verify_server_cert(
            leaf,
            &certificates[1..],
            &name,
            &[],
            UnixTime::since_unix_epoch(Duration::from_secs(timestamp)),
        )
        .map_err(|_| "HTTPS 证书未通过公信链、域名或有效期验证")?;
    let (_, parsed) =
        x509_parser::parse_x509_certificate(leaf.as_ref()).map_err(|_| "解析 HTTPS 证书失败")?;
    let start = parsed.validity().not_before.timestamp();
    let end = parsed.validity().not_after.timestamp();
    let renew_at = end - ((end - start) / 3).max(3600);
    let key = PrivateKeyDer::from_pem_slice(key.as_bytes())
        .map_err(|_| "系统密钥库中的 HTTPS 私钥无效")?;
    let config =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|_| "HTTPS 协议配置失败")?
            .with_no_client_auth()
            .with_single_cert(certificates, key)
            .map_err(|_| "HTTPS 证书与系统私钥不匹配")?;
    Ok(Material {
        certificate,
        config: Arc::new(config),
        renew_at,
        expires_at: end,
    })
}
fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}
fn load_material(
    dir: &Path,
    vault: &dyn Vault,
    roots: Arc<RootCertStore>,
) -> Result<Material, String> {
    let path = dir.join(SERVER_CERT);
    let metadata = fs::symlink_metadata(&path).map_err(|_| "公信 HTTPS 证书尚未准备")?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > CERTIFICATE_LIMIT
    {
        return Err("公信 HTTPS 证书文件无效".into());
    }
    let certificate = fs::read_to_string(&path).map_err(|_| "读取公信 HTTPS 证书失败")?;
    let key = vault
        .get(SERVER_KEY)?
        .ok_or("系统密钥库中缺少 HTTPS 私钥")?;
    validate_material(certificate, &key, roots, now())
}
fn server_key(vault: &dyn Vault) -> Result<Zeroizing<String>, String> {
    if let Some(key) = vault.get(SERVER_KEY)? {
        KeyPair::from_pem(&key).map_err(|_| "系统密钥库中的 HTTPS 私钥损坏，拒绝覆盖")?;
        return Ok(key);
    }
    // 同一主机续期复用系统中的密钥，公开证书链单文件原子切换，不存在证书/私钥半更新。
    let key = Zeroizing::new(
        KeyPair::generate()
            .map_err(|_| "生成 HTTPS 私钥失败")?
            .serialize_pem(),
    );
    vault.set(SERVER_KEY, &key)?;
    Ok(key)
}
fn publish_certificate(dir: &Path, material: &Material) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|_| "创建公开证书目录失败")?;
    let staged = dir.join(format!(".certificate-{}.next", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)
            .map_err(|_| "创建待更新公开证书失败")?;
        file.write_all(material.certificate.as_bytes())
            .map_err(|_| "写入公开证书失败")?;
        file.sync_all().map_err(|_| "同步公开证书失败")?;
        fs::rename(&staged, dir.join(SERVER_CERT)).map_err(|_| "原子更新公开证书失败")
    })();
    if result.is_err() {
        let _ = fs::remove_file(staged);
    }
    result.map_err(str::to_string)
}

fn https_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "创建办证 HTTPS 客户端失败".into())
}
async fn response_bytes(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "读取办证 HTTPS 响应失败")?
    {
        if bytes.len() + chunk.len() > RESPONSE_LIMIT {
            return Err("办证 HTTPS 响应过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
// 仅做标准库的 HTTP 适配，协议签名、订单、nonce 和挑战状态均由 instant-acme 实现。
struct AcmeTransport(reqwest::Client);
impl instant_acme::HttpClient for AcmeTransport {
    fn request(
        &self,
        request: axum::http::Request<instant_acme::BodyWrapper<Bytes>>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<instant_acme::BytesResponse, instant_acme::Error>,
                > + Send,
        >,
    > {
        let client = self.0.clone();
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let body = body
                .collect()
                .await
                .unwrap_or_else(|never| match never {})
                .to_bytes();
            let response = client
                .request(parts.method, parts.uri.to_string())
                .headers(parts.headers)
                .body(body)
                .send()
                .await
                .map_err(|_| {
                    instant_acme::Error::Other(std::io::Error::other("ACME HTTPS 请求失败").into())
                })?;
            let status = response.status();
            let headers = response.headers().clone();
            let bytes = response_bytes(response).await.map_err(|_| {
                instant_acme::Error::Other(std::io::Error::other("ACME 响应无效").into())
            })?;
            let mut response =
                axum::http::Response::new(http_body_util::Full::new(Bytes::from(bytes)));
            *response.status_mut() = status;
            *response.headers_mut() = headers;
            Ok(response.into())
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingDns {
    zone_id: String,
    value: String,
    record_id: Option<String>,
    created_at: i64,
}
impl Drop for PendingDns {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}
struct DnsProvider {
    client: reqwest::Client,
    authorization: DnsAuthorization,
    api: String,
}
impl DnsProvider {
    async fn request(
        &self,
        method: reqwest::Method,
        zone: &str,
        suffix: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        if !valid_record_id(zone) {
            return Err("DNS 验证区域 ID 无效".into());
        }
        let url = format!("{}/zones/{zone}/dns_records{suffix}", self.api);
        let mut request = self
            .client
            .request(method, url)
            .bearer_auth(&self.authorization.api_token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|_| "Cloudflare DNS HTTPS 请求失败")?;
        if response.status() == reqwest::StatusCode::NOT_FOUND && suffix.starts_with('/') {
            return Ok(serde_json::Value::Null);
        }
        if !response.status().is_success() {
            return Err(format!(
                "Cloudflare DNS 授权或请求失败（HTTP {}）",
                response.status().as_u16()
            ));
        }
        let value: serde_json::Value = serde_json::from_slice(&response_bytes(response).await?)
            .map_err(|_| "Cloudflare DNS 响应无效")?;
        if value["success"] != true {
            return Err("Cloudflare DNS 操作未成功".into());
        }
        value
            .get("result")
            .cloned()
            .ok_or_else(|| "Cloudflare DNS 响应缺少结果".into())
    }
    async fn create(&self, pending: &PendingDns) -> Result<String, String> {
        let result = self
            .request(
                reqwest::Method::POST,
                &pending.zone_id,
                "",
                Some(serde_json::json!({
                    "type":"TXT", "name":challenge_name(), "content":pending.value, "ttl":60
                })),
            )
            .await?;
        let id = result["id"]
            .as_str()
            .filter(|id| valid_record_id(id))
            .ok_or("DNS 验证记录标识无效")?;
        Ok(id.into())
    }
    async fn cleanup(&self, vault: &dyn Vault) -> Result<(), String> {
        let Some(encoded) = vault.get(PENDING_DNS)? else {
            return Ok(());
        };
        let pending: PendingDns =
            serde_json::from_str(&encoded).map_err(|_| "系统中的 DNS 清理记录无效")?;
        // 已知ID直接读取本次记录，避免列表分页遗漏；管理员修改过的记录不再由本任务删除。
        if let Some(id) = &pending.record_id {
            if !valid_record_id(id) {
                return Err("DNS 验证记录标识无效".into());
            }
            if let Ok(record) = self
                .request(
                    reqwest::Method::GET,
                    &pending.zone_id,
                    &format!("/{id}"),
                    None,
                )
                .await
            {
                if record.is_null() {
                    return vault.delete(PENDING_DNS);
                }
                if record["id"].as_str() != Some(id.as_str()) {
                    return Err("DNS 验证记录标识不匹配".into());
                }
                if record["type"] == "TXT"
                    && record["name"] == challenge_name()
                    && record["content"].as_str() == Some(pending.value.as_str())
                {
                    self.request(
                        reqwest::Method::DELETE,
                        &pending.zone_id,
                        &format!("/{id}"),
                        None,
                    )
                    .await?;
                }
                return vault.delete(PENDING_DNS);
            }
            // 详情请求错误时，用有界完整列表确认真实状态，不把HTTP错误当作记录已删除。
        }
        // 创建响应中断时只能按精确名称、类型和值找回；分页不完整时不清除系统中的待办。
        let mut found = false;
        for page in 1..=10 {
            let records = self
                .request(
                    reqwest::Method::GET,
                    &pending.zone_id,
                    &format!(
                        "?type=TXT&name={}&per_page=100&page={page}",
                        challenge_name()
                    ),
                    None,
                )
                .await?;
            let records = records.as_array().ok_or("DNS 验证记录列表无效")?;
            for record in records {
                if record["type"] == "TXT"
                    && record["name"] == challenge_name()
                    && record["content"].as_str() == Some(pending.value.as_str())
                {
                    let id = record["id"]
                        .as_str()
                        .filter(|id| valid_record_id(id))
                        .ok_or("DNS 验证记录标识无效")?;
                    self.request(
                        reqwest::Method::DELETE,
                        &pending.zone_id,
                        &format!("/{id}"),
                        None,
                    )
                    .await?;
                    found = true;
                }
            }
            if found || records.len() < 100 {
                break;
            }
            if page == 10 {
                return Err("DNS 验证记录分页未完整读取，已保留清理待办".into());
            }
        }
        if pending.record_id.is_none() && !found && now() - pending.created_at < 120 {
            // 创建请求中断时不能假定服务器未写入；保留密钥库记录供下次准确清理。
            return Err("DNS 验证记录创建结果未确认，下次办证将继续清理".into());
        }
        vault.delete(PENDING_DNS)
    }
}
fn challenge_name() -> String {
    format!("_acme-challenge.{}", crate::PLATFORM_HOST)
}
fn save_pending(vault: &dyn Vault, pending: &PendingDns) -> Result<(), String> {
    let encoded =
        Zeroizing::new(serde_json::to_string(pending).map_err(|_| "编码 DNS 验证状态失败")?);
    vault.set(PENDING_DNS, &encoded)
}
async fn propagated(
    client: &reqwest::Client,
    resolver: &str,
    expected: &str,
) -> Result<(), String> {
    let name = challenge_name();
    let start = tokio::time::Instant::now();
    while start.elapsed() < Duration::from_secs(180) {
        let mut query = reqwest::Url::parse(resolver).map_err(|_| "DNS 验证查询地址无效")?;
        query
            .query_pairs_mut()
            .append_pair("name", &name)
            .append_pair("type", "TXT");
        let response = client
            .get(query)
            .header("accept", "application/dns-json")
            .send()
            .await;
        if let Ok(response) = response {
            if response.status().is_success() {
                if let Ok(bytes) = response_bytes(response).await {
                    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                        if value["Status"] == 0
                            && value["Answer"].as_array().is_some_and(|answers| {
                                answers.iter().any(|answer| {
                                    answer["type"] == 16
                                        && answer["name"].as_str().is_some_and(|value| {
                                            value.trim_end_matches('.') == name
                                        })
                                        && answer["data"].as_str().is_some_and(|value| {
                                            value.trim_matches('"') == expected
                                        })
                                })
                            })
                        {
                            return Ok(());
                        }
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    Err("DNS 办证验证记录尚未生效，请检查域名授权或网络连接".into())
}
async fn account(
    vault: &dyn Vault,
    client: reqwest::Client,
    directory: &str,
) -> Result<Account, String> {
    let builder = Account::builder_with_http(Box::new(AcmeTransport(client)));
    if let Some(encoded) = vault.get(ACCOUNT)? {
        let credentials =
            serde_json::from_str(&encoded).map_err(|_| "系统密钥库中的 ACME 账户无效")?;
        return builder
            .from_credentials(credentials)
            .await
            .map_err(|_| "恢复 ACME 账户失败".into());
    }
    let (account, credentials) = builder
        .create(
            &NewAccount {
                contact: &[],
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            directory.into(),
            None,
        )
        .await
        .map_err(|_| "创建 ACME 账户失败")?;
    let encoded =
        Zeroizing::new(serde_json::to_string(&credentials).map_err(|_| "编码 ACME 账户失败")?);
    vault.set(ACCOUNT, &encoded)?;
    Ok(account)
}
async fn issue_order(
    vault: &dyn Vault,
    provider: &DnsProvider,
    directory: &str,
    resolver: &str,
    roots: Arc<RootCertStore>,
) -> Result<Material, String> {
    provider.cleanup(vault).await?;
    let key = server_key(vault)?;
    let account = account(vault, provider.client.clone(), directory).await?;
    let identifiers = [Identifier::Dns(crate::PLATFORM_HOST.into())];
    let mut order = account
        .new_order(&NewOrder::new(&identifiers))
        .await
        .map_err(|_| "申请 ACME 证书订单失败")?;
    let mut authorizations = order.authorizations();
    while let Some(result) = authorizations.next().await {
        let mut authorization = result.map_err(|_| "读取 ACME 域名验证状态失败")?;
        match authorization.status {
            AuthorizationStatus::Valid => continue,
            AuthorizationStatus::Pending => {}
            _ => return Err("ACME 域名验证状态无效".into()),
        }
        let mut challenge = authorization
            .challenge(ChallengeType::Dns01)
            .ok_or("证书颁发机构未提供 DNS-01 验证")?;
        let mut pending = PendingDns {
            zone_id: provider.authorization.zone_id.clone(),
            value: challenge.key_authorization().dns_value(),
            record_id: None,
            created_at: now(),
        };
        save_pending(vault, &pending)?;
        pending.record_id = Some(provider.create(&pending).await?);
        save_pending(vault, &pending)?;
        propagated(&provider.client, resolver, &pending.value).await?;
        challenge
            .set_ready()
            .await
            .map_err(|_| "提交 ACME 域名验证失败")?;
    }
    let retry = RetryPolicy::new().timeout(Duration::from_secs(180));
    if order
        .poll_ready(&retry)
        .await
        .map_err(|_| "ACME 域名验证未通过")?
        != OrderStatus::Ready
    {
        return Err("ACME 证书订单尚未就绪".into());
    }
    let pair = KeyPair::from_pem(&key).map_err(|_| "HTTPS 系统私钥无效")?;
    let mut parameters = CertificateParams::new(vec![crate::PLATFORM_HOST.into()])
        .map_err(|_| "HTTPS 域名声明无效")?;
    parameters.distinguished_name = rcgen::DistinguishedName::new();
    let request = parameters
        .serialize_request(&pair)
        .map_err(|_| "生成证书签名请求失败")?;
    order
        .finalize_csr(request.der())
        .await
        .map_err(|_| "提交 ACME 证书请求失败")?;
    let certificate = order
        .poll_certificate(&retry)
        .await
        .map_err(|_| "获取 ACME 公信证书失败")?;
    validate_material(certificate, &key, roots, now())
}

pub(crate) async fn cancelled(receiver: &mut watch::Receiver<bool>) {
    loop {
        if *receiver.borrow() {
            return;
        }
        if receiver.changed().await.is_err() {
            return;
        }
    }
}
async fn issue(
    vault: &dyn Vault,
    roots: Arc<RootCertStore>,
    cancel: watch::Receiver<bool>,
) -> Result<Material, String> {
    let encoded = vault
        .get(AUTHORIZATION)?
        .ok_or("尚未配置域名办证授权，请由域名所有者在主机执行 onchina authorize-tls 一次")?;
    let authorization: DnsAuthorization =
        serde_json::from_str(&encoded).map_err(|_| "系统密钥库中的办证授权无效")?;
    authorization.validate()?;
    let provider = DnsProvider {
        client: https_client()?,
        authorization,
        api: CLOUDFLARE_API.into(),
    };
    issue_process(vault, &provider, ACME_DIRECTORY, DNS_QUERY, roots, cancel).await
}
async fn issue_process(
    vault: &dyn Vault,
    provider: &DnsProvider,
    directory: &str,
    resolver: &str,
    roots: Arc<RootCertStore>,
    mut cancel: watch::Receiver<bool>,
) -> Result<Material, String> {
    let result = tokio::select! {
        biased;
        _ = cancelled(&mut cancel) => Err("证书准备已取消".into()),
        result = tokio::time::timeout(Duration::from_secs(600), issue_order(vault, provider, directory, resolver, roots)) => {
            result.unwrap_or_else(|_| Err("公信证书申请超时".into()))
        }
    };
    // 失败和取消也走同一清理路径；中断或清理失败的记录保存在系统密钥库供下次继续。
    let cleanup = tokio::time::timeout(Duration::from_secs(30), provider.cleanup(vault))
        .await
        .unwrap_or_else(|_| Err("DNS 验证记录清理超时，已保留待办".into()));
    match (result, cleanup) {
        (Ok(material), Ok(())) => Ok(material),
        (Err(error), _) => Err(error),
        (Ok(material), Err(error)) => {
            tracing::warn!(%error, "证书已签发，DNS 验证清理保留待办，将于下次处理");
            Ok(material)
        }
    }
}

/// 有效证书直接使用；宿主在缺少有效公信证书时签发，模块只加载同一份证书。
pub(crate) async fn prepare(
    host: bool,
    cancel: watch::Receiver<bool>,
) -> Result<RustlsConfig, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tls_dir()?;
    let material = match load_material(&dir, &SystemVault, public_roots()) {
        Ok(material) => material,
        Err(error) if !host => return Err(error),
        Err(_) => {
            let material = issue(&SystemVault, public_roots(), cancel).await?;
            publish_certificate(&dir, &material)?;
            material
        }
    };
    Ok(RustlsConfig::from_config(material.config))
}

/// 宿主后台续期，宿主和模块都轮询公开证书更新并热加载；失败不覆盖当前有效证书。
pub(crate) async fn maintain(
    config: RustlsConfig,
    host: bool,
    mut cancel: watch::Receiver<bool>,
) -> Result<(), String> {
    let dir = tls_dir()?;
    let mut loaded = load_material(&dir, &SystemVault, public_roots())?.certificate;
    let mut next_attempt = tokio::time::Instant::now();
    loop {
        if *cancel.borrow() {
            return Ok(());
        }
        let current = load_material(&dir, &SystemVault, public_roots())?;
        if current.certificate != loaded {
            config.reload_from_config(current.config.clone());
            loaded = current.certificate.clone();
        }
        if host && now() >= current.renew_at && tokio::time::Instant::now() >= next_attempt {
            match issue(&SystemVault, public_roots(), cancel.clone()).await {
                Ok(material) => match publish_certificate(&dir, &material) {
                    Ok(()) => {
                        config.reload_from_config(material.config);
                        loaded = material.certificate;
                        tracing::info!("公信 HTTPS 证书续期并热加载完成");
                    }
                    Err(error) => tracing::warn!(%error, "新证书未能保存，保留现有有效证书"),
                },
                Err(error) => {
                    if *cancel.borrow() {
                        return Ok(());
                    }
                    if now() >= current.expires_at {
                        return Err("公信 HTTPS 证书已过期，续期失败".into());
                    }
                    tracing::warn!(%error, "公信 HTTPS 续期失败，保留现有有效证书，稍后重试");
                }
            }
            next_attempt = tokio::time::Instant::now() + RENEW_INTERVAL;
        }
        tokio::select! {
            _ = cancelled(&mut cancel) => return Ok(()),
            _ = tokio::time::sleep(Duration::from_secs(60)) => {},
        }
    }
}

#[cfg(test)]
#[path = "../tests/tls.rs"]
mod tests;
