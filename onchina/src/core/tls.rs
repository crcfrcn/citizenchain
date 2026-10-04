//! onchina 内网 API 机构私有 CA TLS(Card 05)。
//!
//! 每个机构节点只在证书和私钥同时不存在的首次启动生成私有根 CA，再用该 CA
//! 签发 `onchina.local` 服务证书。升级、重启和配置变化不得覆盖根 CA；根材料异常
//! 时必须失败关闭。员工电脑只下载并信任 CA 公钥证书，CA 私钥永不通过 HTTP 暴露。

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use axum_server::tls_rustls::RustlsConfig;
use base64::Engine as _;
use rcgen::{
    date_time_ymd, BasicConstraints, Certificate, CertificateParams, DnType,
    ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose,
};
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};
use x509_parser::{extensions::GeneralName, parse_x509_certificate};

const ONCHINA_TLS_HOST: &str = "onchina.local";
const ORG_CA_CERT_FILE: &str = "onchina-org-root-ca.crt";
const ORG_CA_KEY_FILE: &str = "onchina-org-root-ca.key";
const SERVER_CERT_FILE: &str = "onchina-server.crt";
const SERVER_KEY_FILE: &str = "onchina-server.key";
const HOST_MARKER_FILE: &str = "onchina-cert-host.txt";
const PROFILE_MARKER_FILE: &str = "onchina-cert-profile.txt";
const TLS_PROFILE_ID: &str = "onchina-ca2036-server397d";
const ORG_CA_COMMON_NAME: &str = "OnChina Organization Root CA";
const SERVER_COMMON_NAME: &str = "onchina.local";
const ORG_CA_VALID_UNTIL: &str = "2036-01-01T00:00:00Z";
const SERVER_VALID_DAYS: i64 = 397;
const SERVER_RENEW_BEFORE_DAYS: i64 = 30;

#[derive(Clone, Debug)]
pub(crate) struct CaCertificateInfo {
    pub(crate) filename: &'static str,
    pub(crate) sha256: String,
    pub(crate) subject: &'static str,
    pub(crate) valid_until: String,
}

/// 默认启用 HTTPS；显式关闭会由服务入口拒绝，禁止启动明文监听。
pub(crate) fn is_enabled() -> bool {
    std::env::var("ONCHINA_ENABLE_TLS")
        .ok()
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes"
        })
        .unwrap_or(true)
}

/// 证书持久化目录:`ONCHINA_TLS_DIR`(node 传 `base_path/onchina-tls`);兜底 exe 同目录 `tls`。
pub(crate) fn tls_dir() -> PathBuf {
    if let Some(dir) = std::env::var("ONCHINA_TLS_DIR")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        return PathBuf::from(dir);
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("tls")
}

fn ca_cert_path(dir: &Path) -> PathBuf {
    dir.join(ORG_CA_CERT_FILE)
}

fn ca_key_path(dir: &Path) -> PathBuf {
    dir.join(ORG_CA_KEY_FILE)
}

fn server_cert_path(dir: &Path) -> PathBuf {
    dir.join(SERVER_CERT_FILE)
}

fn server_key_path(dir: &Path) -> PathBuf {
    dir.join(SERVER_KEY_FILE)
}

fn host_marker_path(dir: &Path) -> PathBuf {
    dir.join(HOST_MARKER_FILE)
}

fn profile_marker_path(dir: &Path) -> PathBuf {
    dir.join(PROFILE_MARKER_FILE)
}

fn org_ca_params() -> CertificateParams {
    let mut params = CertificateParams::default();
    params.not_before = date_time_ymd(2026, 1, 1);
    params.not_after = date_time_ymd(2036, 1, 1);
    params
        .distinguished_name
        .push(DnType::OrganizationName, "OnChina");
    params
        .distinguished_name
        .push(DnType::CommonName, ORG_CA_COMMON_NAME);
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    params
}

fn server_params(now: OffsetDateTime) -> Result<CertificateParams, String> {
    let mut params = CertificateParams::new(vec![ONCHINA_TLS_HOST.to_string()])
        .map_err(|e| format!("build onchina server cert params failed: {e}"))?;
    params.not_before = now - Duration::days(1);
    params.not_after = now + Duration::days(SERVER_VALID_DAYS);
    params
        .distinguished_name
        .push(DnType::OrganizationName, "OnChina");
    params
        .distinguished_name
        .push(DnType::CommonName, SERVER_COMMON_NAME);
    params.is_ca = IsCa::ExplicitNoCa;
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    Ok(params)
}

fn write_new_file(path: &Path, content: &str, secret: bool) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    if secret {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|e| format!("create {} without overwrite failed: {e}", path.display()))?;
    file.write_all(content.as_bytes())
        .map_err(|e| format!("write {} failed: {e}", path.display()))?;
    file.sync_all()
        .map_err(|e| format!("sync {} failed: {e}", path.display()))?;
    if secret {
        restrict_secret_file(path)?;
    }
    Ok(())
}

fn replace_file_atomically(path: &Path, content: &str, secret: bool) -> Result<(), String> {
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("invalid tls filename: {}", path.display()))?;
    let staged = path.with_file_name(format!(".{filename}.{}.next", std::process::id()));
    write_new_file(&staged, content, secret)?;
    if let Err(error) = fs::rename(&staged, path) {
        let _ = fs::remove_file(&staged);
        return Err(format!(
            "atomically replace {} failed: {error}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn restrict_secret_file(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .map_err(|e| format!("read {} metadata failed: {e}", path.display()))?
        .permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms)
        .map_err(|e| format!("restrict {} permissions failed: {e}", path.display()))
}

#[cfg(not(unix))]
fn restrict_secret_file(_path: &std::path::Path) -> Result<(), String> {
    Ok(())
}

fn cert_host_matches(dir: &Path) -> bool {
    fs::read_to_string(host_marker_path(dir))
        .ok()
        .is_some_and(|value| value.trim() == ONCHINA_TLS_HOST)
}

fn cert_profile_matches(dir: &Path) -> bool {
    fs::read_to_string(profile_marker_path(dir))
        .ok()
        .is_some_and(|value| value.trim() == TLS_PROFILE_ID)
}

fn material_pair_state(cert_path: &Path, key_path: &Path, label: &str) -> Result<bool, String> {
    match (cert_path.is_file(), key_path.is_file()) {
        (false, false) => Ok(false),
        (true, true) => Ok(true),
        (true, false) => Err(format!(
            "{label} private key is missing while certificate exists; refusing automatic replacement"
        )),
        (false, true) => Err(format!(
            "{label} certificate is missing while private key exists; refusing automatic replacement"
        )),
    }
}

fn load_key_pair(path: &Path, label: &str) -> Result<KeyPair, String> {
    let pem =
        fs::read_to_string(path).map_err(|e| format!("read {label} private key failed: {e}"))?;
    KeyPair::from_pem(pem.as_str()).map_err(|e| format!("parse {label} private key failed: {e}"))
}

fn certificate_common_name(cert: &x509_parser::certificate::X509Certificate<'_>) -> Option<String> {
    cert.subject()
        .iter_common_name()
        .next()
        .and_then(|value| value.as_str().ok())
        .map(str::to_owned)
}

fn write_cert_markers(dir: &Path) -> Result<(), String> {
    fs::write(host_marker_path(dir), ONCHINA_TLS_HOST)
        .map_err(|e| format!("write tls host marker failed: {e}"))?;
    fs::write(profile_marker_path(dir), TLS_PROFILE_ID)
        .map_err(|e| format!("write tls profile marker failed: {e}"))?;
    Ok(())
}

fn generate_ca_material(dir: &Path) -> Result<(Certificate, KeyPair), String> {
    let ca_key = KeyPair::generate().map_err(|e| format!("generate org CA key failed: {e}"))?;
    let ca_cert = org_ca_params()
        .self_signed(&ca_key)
        .map_err(|e| format!("generate org CA cert failed: {e}"))?;
    // 根 CA 只允许首次创建；create_new 可阻止任何隐式覆盖。
    write_new_file(&ca_key_path(dir), ca_key.serialize_pem().as_str(), true)?;
    if let Err(error) = write_new_file(&ca_cert_path(dir), ca_cert.pem().as_str(), false) {
        return Err(format!(
            "write new organization CA certificate failed after private key creation: {error}"
        ));
    }
    Ok((ca_cert, ca_key))
}

fn load_existing_ca(dir: &Path, now: OffsetDateTime) -> Result<(Certificate, KeyPair), String> {
    let pem = fs::read_to_string(ca_cert_path(dir))
        .map_err(|e| format!("read organization CA certificate failed: {e}"))?;
    let der = certificate_pem_to_der(pem.as_str())?;
    let (remainder, parsed) = parse_x509_certificate(der.as_slice())
        .map_err(|e| format!("parse organization CA certificate failed: {e}"))?;
    if !remainder.is_empty() {
        return Err("organization CA certificate contains trailing data".to_string());
    }
    if !parsed.is_ca() {
        return Err("organization CA certificate is not a CA".to_string());
    }
    let key_usage = parsed
        .key_usage()
        .map_err(|e| format!("parse organization CA key usage failed: {e}"))?
        .ok_or_else(|| "organization CA certificate has no key usage".to_string())?;
    if !(key_usage.value.key_cert_sign() && key_usage.value.crl_sign()) {
        return Err("organization CA certificate lacks CA signing usages".to_string());
    }
    if certificate_common_name(&parsed).as_deref() != Some(ORG_CA_COMMON_NAME) {
        return Err("organization CA certificate common name is invalid".to_string());
    }
    if parsed.subject() != parsed.issuer() {
        return Err("organization CA certificate is not self-issued".to_string());
    }
    let now_timestamp = now.unix_timestamp();
    if now_timestamp < parsed.validity().not_before.timestamp()
        || now_timestamp > parsed.validity().not_after.timestamp()
    {
        return Err("organization CA certificate is outside its validity period".to_string());
    }
    parsed
        .verify_signature(None)
        .map_err(|e| format!("verify organization CA self-signature failed: {e}"))?;

    let ca_key = load_key_pair(&ca_key_path(dir), "organization CA")?;
    if parsed.public_key().raw != ca_key.public_key_der().as_slice() {
        return Err("organization CA certificate and private key do not match".to_string());
    }
    let params = CertificateParams::from_ca_cert_pem(pem.as_str())
        .map_err(|e| format!("load existing organization CA parameters failed: {e}"))?;
    let ca_cert = params
        .self_signed(&ca_key)
        .map_err(|e| format!("prepare existing organization CA issuer failed: {e}"))?;
    Ok((ca_cert, ca_key))
}

fn load_or_initialize_ca(
    dir: &Path,
    now: OffsetDateTime,
) -> Result<(Certificate, KeyPair, bool), String> {
    if material_pair_state(&ca_cert_path(dir), &ca_key_path(dir), "organization CA")? {
        let (certificate, key) = load_existing_ca(dir, now)?;
        return Ok((certificate, key, false));
    }
    generate_ca_material(dir).map(|(certificate, key)| (certificate, key, true))
}

fn generate_server_material(
    dir: &Path,
    ca_cert: &Certificate,
    ca_key: &KeyPair,
    now: OffsetDateTime,
) -> Result<(), String> {
    let server_key =
        KeyPair::generate().map_err(|e| format!("generate onchina server key failed: {e}"))?;
    let server_cert = server_params(now)?
        .signed_by(&server_key, ca_cert, ca_key)
        .map_err(|e| format!("sign onchina server cert failed: {e}"))?;
    replace_file_atomically(
        &server_key_path(dir),
        server_key.serialize_pem().as_str(),
        true,
    )?;
    replace_file_atomically(&server_cert_path(dir), server_cert.pem().as_str(), false)?;
    Ok(())
}

fn server_certificate_is_reusable(
    dir: &Path,
    ca_pem: &str,
    now: OffsetDateTime,
) -> Result<bool, String> {
    // 服务证书不是信任锚；任何一侧缺失均可安全地用现有根 CA 成对重签。
    if !(server_cert_path(dir).is_file() && server_key_path(dir).is_file()) {
        return Ok(false);
    }
    if !(cert_host_matches(dir) && cert_profile_matches(dir)) {
        return Ok(false);
    }
    let server_pem = match fs::read_to_string(server_cert_path(dir)) {
        Ok(value) => value,
        Err(_) => return Ok(false),
    };
    let server_der = match certificate_pem_to_der(server_pem.as_str()) {
        Ok(value) => value,
        Err(_) => return Ok(false),
    };
    let (_, server) = match parse_x509_certificate(server_der.as_slice()) {
        Ok(value) => value,
        Err(_) => return Ok(false),
    };
    let ca_der = certificate_pem_to_der(ca_pem)?;
    let (_, ca) = parse_x509_certificate(ca_der.as_slice())
        .map_err(|e| format!("parse organization CA for server validation failed: {e}"))?;
    let server_key = match load_key_pair(&server_key_path(dir), "server TLS") {
        Ok(value) => value,
        Err(_) => return Ok(false),
    };
    if server.public_key().raw != server_key.public_key_der().as_slice() {
        return Ok(false);
    }
    if server.issuer() != ca.subject()
        || server.verify_signature(Some(ca.public_key())).is_err()
        || server.is_ca()
    {
        return Ok(false);
    }
    let has_host = server
        .subject_alternative_name()
        .ok()
        .flatten()
        .is_some_and(|extension| {
            extension.value.general_names.iter().any(
                |name| matches!(name, GeneralName::DNSName(value) if *value == ONCHINA_TLS_HOST),
            )
        });
    if !has_host {
        return Ok(false);
    }
    let server_auth = server
        .extended_key_usage()
        .ok()
        .flatten()
        .is_some_and(|extension| extension.value.server_auth);
    if !server_auth {
        return Ok(false);
    }
    let renew_at = now + Duration::days(SERVER_RENEW_BEFORE_DAYS);
    Ok(
        now.unix_timestamp() >= server.validity().not_before.timestamp()
            && renew_at.unix_timestamp() < server.validity().not_after.timestamp(),
    )
}

fn ensure_certificate_material_at(dir: &Path, now: OffsetDateTime) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("create tls dir failed: {e}"))?;
    let (ca_cert, ca_key, ca_initialized) = load_or_initialize_ca(dir, now)?;
    let ca_pem = fs::read_to_string(ca_cert_path(dir))
        .map_err(|e| format!("read persisted organization CA certificate failed: {e}"))?;
    if ca_initialized || !server_certificate_is_reusable(dir, ca_pem.as_str(), now)? {
        generate_server_material(dir, &ca_cert, &ca_key, now)?;
        write_cert_markers(dir)?;
        tracing::info!(
            dir = %dir.display(),
            host = ONCHINA_TLS_HOST,
            profile = TLS_PROFILE_ID,
            server_valid_days = SERVER_VALID_DAYS,
            "onchina server TLS certificate issued from persistent organization CA"
        );
    }
    Ok(())
}

fn ensure_certificate_material() -> Result<(), String> {
    ensure_certificate_material_at(&tls_dir(), OffsetDateTime::now_utc())
}

/// 读取机构 CA 公钥证书,用于员工浏览器下载并安装到受信任根证书。
pub(crate) fn organization_ca_certificate_pem() -> Result<String, String> {
    ensure_certificate_material()?;
    fs::read_to_string(ca_cert_path(&tls_dir())).map_err(|e| format!("read CA cert failed: {e}"))
}

fn certificate_pem_to_der(pem: &str) -> Result<Vec<u8>, String> {
    let body = pem
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("-----"))
        .collect::<String>();
    base64::engine::general_purpose::STANDARD
        .decode(body.as_bytes())
        .map_err(|e| format!("decode CA certificate PEM failed: {e}"))
}

pub(crate) fn organization_ca_certificate_info() -> Result<CaCertificateInfo, String> {
    let pem = organization_ca_certificate_pem()?;
    let der = certificate_pem_to_der(pem.as_str())?;
    let sha256 = hex::encode(Sha256::digest(der.as_slice()));
    Ok(CaCertificateInfo {
        filename: ORG_CA_CERT_FILE,
        sha256,
        subject: ORG_CA_COMMON_NAME,
        valid_until: ORG_CA_VALID_UNTIL.to_string(),
    })
}

/// 加载已有机构 CA 签发证书;无则生成机构 CA + onchina.local 服务证书。
pub(crate) async fn load_or_generate_rustls_config() -> Result<RustlsConfig, String> {
    // rustls 0.23 需要进程级 CryptoProvider;幂等安装 ring 实现。
    let _ = rustls::crypto::ring::default_provider().install_default();

    ensure_certificate_material()?;

    let dir = tls_dir();
    RustlsConfig::from_pem_file(server_cert_path(&dir), server_key_path(&dir))
        .await
        .map_err(|e| format!("load onchina tls cert failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_tls_dir(label: &str) -> Result<PathBuf, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("clock before unix epoch: {e}"))?
            .as_nanos();
        Ok(std::env::temp_dir().join(format!(
            "onchina-tls-{label}-{}-{nonce}",
            std::process::id()
        )))
    }

    fn sha256_file(path: &Path) -> Result<String, String> {
        let bytes = fs::read(path)
            .map_err(|e| format!("read fingerprint input {} failed: {e}", path.display()))?;
        Ok(hex::encode(Sha256::digest(bytes)))
    }

    #[test]
    fn restart_and_profile_change_never_rotate_root_ca() -> Result<(), String> {
        let dir = test_tls_dir("persistent-root")?;
        let now = OffsetDateTime::now_utc();
        ensure_certificate_material_at(&dir, now)?;
        let ca_cert_hash = sha256_file(&ca_cert_path(&dir))?;
        let ca_key_hash = sha256_file(&ca_key_path(&dir))?;
        let server_cert_hash = sha256_file(&server_cert_path(&dir))?;

        ensure_certificate_material_at(&dir, now)?;
        assert_eq!(sha256_file(&ca_cert_path(&dir))?, ca_cert_hash);
        assert_eq!(sha256_file(&ca_key_path(&dir))?, ca_key_hash);
        assert_eq!(sha256_file(&server_cert_path(&dir))?, server_cert_hash);

        fs::write(profile_marker_path(&dir), "renamed-profile")
            .map_err(|e| format!("change profile marker failed: {e}"))?;
        ensure_certificate_material_at(&dir, now)?;
        assert_eq!(sha256_file(&ca_cert_path(&dir))?, ca_cert_hash);
        assert_eq!(sha256_file(&ca_key_path(&dir))?, ca_key_hash);
        assert_ne!(sha256_file(&server_cert_path(&dir))?, server_cert_hash);
        fs::remove_dir_all(dir).map_err(|e| format!("clean test tls dir failed: {e}"))?;
        Ok(())
    }

    #[test]
    fn incomplete_root_ca_pair_fails_without_overwrite() -> Result<(), String> {
        let dir = test_tls_dir("missing-root")?;
        let now = OffsetDateTime::now_utc();
        ensure_certificate_material_at(&dir, now)?;
        let key_hash = sha256_file(&ca_key_path(&dir))?;
        fs::remove_file(ca_cert_path(&dir))
            .map_err(|e| format!("remove test certificate failed: {e}"))?;

        let error = ensure_certificate_material_at(&dir, now)
            .err()
            .ok_or_else(|| "incomplete root CA pair did not fail closed".to_string())?;
        assert!(error.contains("certificate is missing while private key exists"));
        assert_eq!(sha256_file(&ca_key_path(&dir))?, key_hash);
        assert!(!ca_cert_path(&dir).exists());
        fs::remove_dir_all(dir).map_err(|e| format!("clean test tls dir failed: {e}"))?;
        Ok(())
    }

    #[test]
    fn mismatched_root_ca_key_fails_without_certificate_overwrite() -> Result<(), String> {
        let dir = test_tls_dir("mismatched-root")?;
        let other_dir = test_tls_dir("other-root")?;
        let now = OffsetDateTime::now_utc();
        ensure_certificate_material_at(&dir, now)?;
        ensure_certificate_material_at(&other_dir, now)?;
        let cert_hash = sha256_file(&ca_cert_path(&dir))?;
        fs::copy(ca_key_path(&other_dir), ca_key_path(&dir))
            .map_err(|e| format!("install mismatched test key failed: {e}"))?;

        let error = ensure_certificate_material_at(&dir, now)
            .err()
            .ok_or_else(|| "mismatched root CA key was accepted".to_string())?;
        assert!(error.contains("certificate and private key do not match"));
        assert_eq!(sha256_file(&ca_cert_path(&dir))?, cert_hash);
        fs::remove_dir_all(dir).map_err(|e| format!("clean test tls dir failed: {e}"))?;
        fs::remove_dir_all(other_dir)
            .map_err(|e| format!("clean second test tls dir failed: {e}"))?;
        Ok(())
    }
}
